//! promptu: Promptu's CLI. Build step 1 is memory: the hook every memory
//! write passes through, the checks it runs, and the inventory the memory
//! menu draws. docs/contract.md is the interface; embarch-doc's
//! embarch-promptu/spec.md is current truth.

mod checks;
mod claude;
mod config;
mod frontmatter;
mod hook;
mod inventory;
mod sha1;

use checks::{Before, Finding, Severity};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "promptu", version, about = "Promptu: Claude Code memory checks and inventory (prompt patterns from build step 2)")]
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
            std::panic::set_hook(Box::new(|info| eprintln!("promptu: internal error: {info}")));
            std::panic::catch_unwind(|| match cmd {
                Cmd::Memory { action: Some(MemoryAction::Check { paths, json }), .. } => check(&paths, json),
                Cmd::Memory { action: None, json } => show_inventory(json),
                Cmd::Hook => 0,
            })
            .unwrap_or(INTERNAL_ERROR)
        }
    };
    std::process::exit(code);
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn show_inventory(json: bool) -> i32 {
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
