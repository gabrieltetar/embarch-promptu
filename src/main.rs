//! promptu: Promptu's CLI. Memory (build step 1): the hook every memory
//! write passes through, the checks it runs, and the inventory the memory
//! menu draws. Prompts (build step 2): `prep` picks a pattern and fills what
//! the sources say, `next` takes the answers page by page, `render` writes
//! the prompt and its footer, `lint` checks the pattern files.
//! docs/contract.md is the interface; embarch-doc's embarch-promptu/spec.md
//! is current truth.

mod checks;
mod claude;
mod config;
mod footer;
mod frontmatter;
mod hook;
mod inventory;
mod lint;
mod mustache;
mod pass;
mod pattern;
mod prep;
mod run;
mod sha1;
mod state;

use checks::{Before, Finding, Severity};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "promptu", version, about = "Promptu: Claude Code memory checks and inventory, and a rough prompt turned into a good one")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// The memory that loads in this folder, scope → domain → kind, with stale flags.
    Memory {
        #[command(subcommand)]
        action: Option<MemoryAction>,
        /// Print JSON (docs/contract.md §5).
        #[arg(long)]
        json: bool,
    },
    /// Check a memory write: Claude Code hook JSON on stdin (what the plugin runs).
    Hook,
    /// Pick a pattern for the rough prompt on stdin, fill what its sources say, plan the questions (docs/contract.md §7).
    Prep {
        /// Use this pattern (or `none`) instead of picking one.
        #[arg(long)]
        pattern: Option<String>,
        /// Take every recommendation: no questions.
        #[arg(long)]
        rec: bool,
        /// The work item: a JSON file (key, title, url, id, text) or plain text.
        #[arg(long)]
        desk: Option<PathBuf>,
        /// Skip the model: the keyword pick, nothing filled.
        #[arg(long)]
        offline: bool,
        /// The EmbArch parts this session has: desk, embarch-api (comma-separated; `none`).
        #[arg(long, value_delimiter = ',')]
        have: Vec<String>,
    },
    /// Record a page of answers (JSON on stdin) and print the pages left (§7.5).
    Next {
        /// The run id `prep` printed.
        run: String,
    },
    /// Write the prompt and its footer from a run and its answers (§8).
    Render {
        /// The run id `prep` printed.
        run: String,
        /// End the dialog: the questions left take their recommendation, or become stated assumptions (Esc).
        #[arg(long, value_parser = ["recommended", "assume"])]
        rest: Option<String>,
        /// Print the prompt alone instead of JSON.
        #[arg(long)]
        text: bool,
    },
    /// Check pattern files: schema, AskUserQuestion's limits, slots, examples (§7.7).
    Lint {
        /// Pattern files or a folder; none: the patterns this binary carries (or PROMPTU_PATTERNS).
        paths: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
        /// Also print every example as rendered.
        #[arg(long)]
        show: bool,
        /// Render the examples with these EmbArch parts only: desk, embarch-api (default: both; `none`).
        #[arg(long, value_delimiter = ',')]
        have: Option<Vec<String>>,
    },
}

#[derive(Subcommand)]
enum MemoryAction {
    /// Run the T and I checks on memory files as they stand; with no path, this project's auto-memory folder.
    Check {
        /// Memory files or folders.
        paths: Vec<PathBuf>,
        /// Print JSON (docs/contract.md §4).
        #[arg(long)]
        json: bool,
    },
}

/// A bug, not a finding: what `memory` and `memory check` exit with when they
/// panic, instead of Rust's 101 (docs/contract.md §0).
const INTERNAL_ERROR: i32 = 3;

fn main() {
    let cli = Cli::parse();
    let code = match cli.cmd {
        // The hook fails open on its own terms: exit 0, nothing printed.
        Cmd::Hook => hook::run(),
        cmd => {
            std::panic::set_hook(Box::new(|info| eprintln!("{}", internal_error_line(info))));
            std::panic::catch_unwind(|| match cmd {
                Cmd::Memory { action: Some(MemoryAction::Check { paths, json }), .. } => check(&paths, json),
                Cmd::Memory { action: None, json } => show_inventory(json),
                Cmd::Prep { pattern, rec, desk, offline, have } => run_prep(prep::PrepArgs { pattern, rec, desk, offline, have }),
                Cmd::Next { run } => run_next(&run),
                Cmd::Render { run, rest, text } => run_render(&run, rest, text),
                Cmd::Lint { paths, json, show, have } => run_lint(&paths, json, show, have),
                Cmd::Hook => 0,
            })
            .unwrap_or(INTERNAL_ERROR)
        }
    };
    std::process::exit(code);
}

/// One line for a panic, the message and where it happened: Rust's own
/// rendering of a panic is two lines (contract §0 promises one).
fn internal_error_line(info: &std::panic::PanicHookInfo) -> String {
    let payload = info.payload();
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".into());
    let msg = msg.split_whitespace().collect::<Vec<_>>().join(" ");
    let at = info.location().map(|l| format!(" (at {}:{}:{})", l.file(), l.line(), l.column())).unwrap_or_default();
    format!("promptu: internal error: {msg}{at}")
}

/// A debug build only: `PROMPTU_DEBUG_PANIC` makes the `memory` commands
/// panic, so a test can see the exit-3 path. Compiled out of a release
/// build, where the variable means nothing.
#[cfg(debug_assertions)]
fn debug_panic() {
    if std::env::var_os("PROMPTU_DEBUG_PANIC").is_some() {
        panic!("PROMPTU_DEBUG_PANIC is set:\na test asked for this panic");
    }
}

#[cfg(not(debug_assertions))]
fn debug_panic() {}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn read_stdin() -> String {
    use std::io::Read;
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

/// A command's JSON input on stdin; empty means `{}`.
fn stdin_json() -> Result<serde_json::Value, prep::Fail> {
    let s = read_stdin();
    if s.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&s).map_err(|e| prep::Fail::Usage(format!("stdin is not JSON: {e}")))
}

fn finish(r: Result<serde_json::Value, prep::Fail>, text: bool) -> i32 {
    match r {
        Ok(v) => {
            if text {
                println!("{}", v["prompt"].as_str().unwrap_or_default());
            } else {
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            }
            0
        }
        Err(prep::Fail::Usage(m)) => {
            eprintln!("promptu: {m}");
            2
        }
        Err(prep::Fail::Other(m)) => {
            eprintln!("promptu: {m}");
            1
        }
    }
}

fn run_prep(args: prep::PrepArgs) -> i32 {
    debug_panic();
    finish(prep::prep(&read_stdin(), args), false)
}

fn run_next(run: &str) -> i32 {
    debug_panic();
    finish(stdin_json().and_then(|v| prep::next(run, &v)), false)
}

fn run_render(run: &str, rest: Option<String>, text: bool) -> i32 {
    debug_panic();
    let r = stdin_json().and_then(|mut v| {
        if let (Some(r), Some(o)) = (rest, v.as_object_mut()) {
            o.insert("rest".into(), serde_json::json!(r));
        }
        prep::render(run, &v)
    });
    finish(r, text)
}

fn run_lint(paths: &[PathBuf], json: bool, show: bool, have: Option<Vec<String>>) -> i32 {
    debug_panic();
    let set = if paths.is_empty() {
        match pattern::Set::load() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("promptu: {e}");
                return 2;
            }
        }
    } else {
        // A folder is the set; files are linted against their own folder's set.
        let dir = if paths[0].is_dir() { paths[0].clone() } else { paths[0].parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")) };
        for p in paths {
            if !p.exists() {
                eprintln!("{}: no such file or folder", p.display());
                return 2;
            }
        }
        match pattern::Set::from_dir(&dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", dir.display());
                return 2;
            }
        }
    };
    let only = (!paths.is_empty() && !paths[0].is_dir()).then_some(paths);
    // Only detected parts: `item` follows from the desk and an example's own
    // work item, never from the command line.
    let parts: Vec<String> = match have {
        None => pattern::DETECTED.iter().map(|p| p.to_string()).collect(),
        Some(h) => h.into_iter().filter(|p| pattern::DETECTED.contains(&p.as_str())).collect(),
    };
    let (report, renders) = lint::lint(&set, only, &config::today().to_string(), &parts);
    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        if show {
            for r in &renders {
                println!("{r}");
            }
        }
        println!("{}", lint::text(&report));
    }
    if report.errors > 0 {
        1
    } else {
        0
    }
}

fn show_inventory(json: bool) -> i32 {
    debug_panic();
    let inv = inventory::build(&cwd(), config::today());
    if json {
        println!("{}", serde_json::to_string_pretty(&inv).unwrap_or_default());
    } else {
        println!("{}", inventory::render(&inv));
    }
    0
}

#[derive(Serialize)]
struct FileReport {
    path: String,
    /// `topic` or `index`.
    role: &'static str,
    findings: Vec<Finding>,
}

#[derive(Serialize)]
struct CheckReport {
    promptu: u32,
    today: String,
    files: Vec<FileReport>,
    errors: usize,
    warnings: usize,
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
        .collect();
    // The index first, then the topic files by name.
    out.sort_by_key(|p| (p.file_name().is_none_or(|n| n != "MEMORY.md"), p.clone()));
    out
}

fn check(paths: &[PathBuf], json: bool) -> i32 {
    debug_panic();
    let today = config::today();
    // Each file, and whether it came from a folder: O1 is a check on a
    // folder, so an index named on its own gets I1, I2, I4 and I5 only
    // (contract §4).
    let mut targets: Vec<(PathBuf, bool)> = vec![];
    if paths.is_empty() {
        let here = cwd();
        match claude::auto_memory_dir(&here) {
            Some((dir, _)) if dir.is_dir() => targets.extend(files_in(&dir).into_iter().map(|f| (f, true))),
            Some((dir, _)) => {
                eprintln!("no auto-memory folder yet for {}: {}", claude::project_root(&here).display(), dir.display());
                return 0;
            }
            None => {
                eprintln!("cannot work out the auto-memory folder (is HOME set?)");
                return 2;
            }
        }
    }
    for p in paths {
        if p.is_dir() {
            targets.extend(files_in(p).into_iter().map(|f| (f, true)));
        } else if p.is_file() {
            targets.push((p.clone(), false));
        } else {
            eprintln!("{}: no such file or folder", p.display());
            return 2;
        }
    }

    let mut files = vec![];
    for (t, from_folder) in &targets {
        let Ok(text) = std::fs::read_to_string(t) else {
            eprintln!("{}: cannot be read as text", t.display());
            return 2;
        };
        let dir = t.parent().unwrap_or(Path::new("."));
        let is_index = t.file_name().is_some_and(|n| n == "MEMORY.md");
        let mut findings = if is_index { checks::index(Before::Unknown, &text, Some(dir)) } else { checks::topic(Before::Unknown, &text, today) };
        if is_index && *from_folder {
            for orphan in checks::orphans(dir, &text) {
                findings.push(Finding { check: "O1", severity: Severity::Warning, line: None, message: format!("{orphan} is not linked from the index: delete it or give it an index line") });
            }
        }
        files.push(FileReport { path: t.display().to_string(), role: if is_index { "index" } else { "topic" }, findings });
    }
    let errors = files.iter().flat_map(|f| &f.findings).filter(|f| f.severity == Severity::Error).count();
    let warnings = files.iter().flat_map(|f| &f.findings).filter(|f| f.severity == Severity::Warning).count();
    let report = CheckReport { promptu: 1, today: today.to_string(), files, errors, warnings };

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        for f in report.files.iter().filter(|f| !f.findings.is_empty()) {
            println!("{}", f.path);
            for x in &f.findings {
                let sev = if x.severity == Severity::Error { "error" } else { "warn " };
                let at = x.line.map(|n| format!("line {n}: ")).unwrap_or_default();
                println!("  {} {}  {at}{}", x.check, sev, x.message);
            }
        }
        let with = report.files.iter().filter(|f| !f.findings.is_empty()).count();
        println!("{} file(s) checked: {errors} error(s), {warnings} warning(s), in {with} file(s)", report.files.len());
    }
    if errors > 0 {
        1
    } else {
        0
    }
}
