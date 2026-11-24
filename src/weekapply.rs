//! `promptu week mark` and `promptu week apply` (docs/contract.md §14.4): the
//! owner's verdicts on the sheet, and what an approved line becomes. No model
//! and no network. A memory line is written into a memory folder only when the
//! hook's own checks pass on both files; a pattern line is committed on a
//! branch in the configured checkout (never pushed) or written as a patch.

use crate::checks::{self, Before, Severity};
use crate::week::Fail;
use crate::weekread::{self, is_week_id};
use crate::{claude, config, draft, lint, pattern, state};
use pattern::Set;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fail(code: i32, msg: impl Into<String>) -> Fail {
    Fail { code, msg: msg.into() }
}

fn internal(msg: impl Into<String>) -> Fail {
    fail(3, msg)
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

// ---- the sheet on disk ----

struct Loaded {
    week: String,
    dir: PathBuf,
    sheet: Value,
}

/// The sheet `--week` names, else the open one; a superseded sheet is refused (exit 2).
fn load(week: Option<&str>) -> Result<Loaded, Fail> {
    let (id, sheet) = match week {
        Some(w) => {
            if !is_week_id(w) {
                return Err(fail(2, format!("`{w}` is not a week like 2026-W48")));
            }
            let path = weekread::week_dir()?.join(w).join("sheet.json");
            match weekread::read_json(&path)? {
                Some(sh) => (w.to_string(), sh),
                None => return Err(fail(2, format!("no sheet for {w}"))),
            }
        }
        None => weekread::open_sheet()?.ok_or_else(|| fail(2, "no open sheet"))?,
    };
    if !sheet["superseded_by"].is_null() {
        return Err(fail(2, format!("the {id} sheet is superseded by {}", s(&sheet["superseded_by"]))));
    }
    let dir = weekread::week_dir()?.join(&id);
    Ok(Loaded { week: id, dir, sheet })
}

fn save(l: &Loaded) -> Result<(), Fail> {
    let text = serde_json::to_string_pretty(&l.sheet).map_err(|e| internal(e.to_string()))?;
    state::write_atomic(&l.dir.join("sheet.json"), &text).map_err(internal)?;
    state::write_atomic(&l.dir.join("sheet.md"), &draft::sheet_md(&l.sheet)).map_err(internal)
}

fn each_line(sheet: &mut Value) -> impl Iterator<Item = &mut Value> {
    sheet["groups"].as_array_mut().into_iter().flatten().flat_map(|g| g["lines"].as_array_mut().into_iter().flatten())
}

fn refresh_group_cuts(sheet: &mut Value) {
    for g in sheet["groups"].as_array_mut().into_iter().flatten() {
        let lines = g["lines"].as_array().map_or(&[][..], |a| a.as_slice());
        g["cut"] = json!(!lines.is_empty() && lines.iter().all(|l| l["verdict"] == "cut"));
    }
}

fn open_count(sheet: &Value) -> usize {
    weekread::sheet_lines(sheet).iter().filter(|l| l["status"] == "open").count()
}

fn cut_line(l: &mut Value) {
    l["verdict"] = json!("cut");
    l["status"] = json!("cut");
}

// ---- sheet.md read back (forks W5) ----

struct MdLine {
    id: String,
    ticked: bool,
    note: String,
}

/// The `- [x] l3 …` lines of `sheet.md`: a box that is not `x`, `X` or `✓` is a ✕, and the
/// indented text after a line (other than the `proposed:` and `replay:` pointers) is its note.
fn parse_md(text: &str) -> Vec<MdLine> {
    let mut out: Vec<MdLine> = vec![];
    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(rest) = line.strip_prefix("- [") {
            let Some((mark, after)) = rest.split_once("] ") else { continue };
            let id = after.split_whitespace().next().unwrap_or("");
            let numbered = id.strip_prefix('l').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if numbered {
                out.push(MdLine { id: id.to_string(), ticked: matches!(mark, "x" | "X" | "✓"), note: String::new() });
                continue;
            }
        }
        let indented = line.starts_with("  ") || line.starts_with('\t');
        if let (true, Some(cur)) = (indented, out.last_mut()) {
            let t = line.trim();
            if t.is_empty() || t.starts_with("- proposed:") || t.starts_with("- replay:") {
                continue;
            }
            let t = t.strip_prefix("- ").unwrap_or(t);
            let t = t.strip_prefix("note:").unwrap_or(t).trim();
            if !t.is_empty() {
                if !cur.note.is_empty() {
                    cur.note.push(' ');
                }
                cur.note.push_str(t);
            }
        } else if !indented {
            // Anything unindented ends the note of the line above.
            if let Some(cur) = out.last_mut() {
                cur.note.push('\0');
            }
        }
    }
    for m in &mut out {
        m.note = m.note.trim_end_matches('\0').to_string();
    }
    out
}

/// Fold the owner's edits to `sheet.md` into the sheet: a ✕ on an open or failed line cuts it,
/// and a note written there replaces the line's note. Returns whether anything changed.
fn read_back_md(l: &mut Loaded) -> bool {
    let Ok(text) = std::fs::read_to_string(l.dir.join("sheet.md")) else { return false };
    let marks = parse_md(&text);
    let mut changed = false;
    for line in each_line(&mut l.sheet) {
        let Some(m) = marks.iter().find(|m| m.id == s(&line["id"])) else { continue };
        let status = s(&line["status"]).to_string();
        if status == "applied" {
            continue;
        }
        if !m.note.is_empty() && m.note != s(&line["note"]) {
            line["note"] = json!(m.note);
            changed = true;
        }
        if !m.ticked && matches!(status.as_str(), "open" | "failed") && line["verdict"] == "approve" {
            cut_line(line);
            changed = true;
        }
    }
    if changed {
        refresh_group_cuts(&mut l.sheet);
    }
    changed
}

// ---- week mark ----

pub struct MarkArgs {
    pub ids: Vec<String>,
    pub verdict: Option<String>,
    pub note: Option<String>,
    pub feel: Option<String>,
    pub week: Option<String>,
}

/// A sheet whose every line is applied has nothing left to mark.
fn settled(sheet: &Value) -> bool {
    let lines = weekread::sheet_lines(sheet);
    !lines.is_empty() && lines.iter().all(|l| l["status"] == "applied")
}

pub fn mark(a: &MarkArgs) -> Result<Value, Fail> {
    let verdict = a.verdict.as_deref().unwrap_or("cut");
    if !matches!(verdict, "approve" | "cut") {
        return Err(fail(2, format!("`{verdict}` is not a verdict: approve or cut (a line is applied by `week apply`)")));
    }
    if let Some(f) = &a.feel {
        if !matches!(f.as_str(), "better" | "same" | "worse") {
            return Err(fail(2, format!("`{f}` is not a feel: better, same or worse")));
        }
    }
    if a.ids.is_empty() && a.feel.is_none() {
        return Err(fail(2, "name a line or group id, or pass --feel"));
    }
    if a.note.is_some() && a.ids.iter().any(|i| i.starts_with('g')) {
        return Err(fail(2, "a note belongs to a line, not a group"));
    }
    let mut l = load(a.week.as_deref())?;
    read_back_md(&mut l);
    if !a.ids.is_empty() && settled(&l.sheet) {
        return Err(fail(2, format!("the {} sheet is settled: every line is applied", l.week)));
    }

    // Resolve every id first, so an unknown one changes nothing.
    let mut targets: Vec<String> = vec![];
    for id in &a.ids {
        let is_group = l.sheet["groups"].as_array().into_iter().flatten().any(|g| s(&g["id"]) == id);
        let members: Vec<String> = if is_group {
            l.sheet["groups"].as_array().into_iter().flatten().filter(|g| s(&g["id"]) == id).flat_map(|g| g["lines"].as_array().into_iter().flatten().map(|x| s(&x["id"]).to_string())).collect()
        } else if weekread::sheet_lines(&l.sheet).iter().any(|x| s(&x["id"]) == id) {
            vec![id.clone()]
        } else {
            return Err(fail(2, format!("no line or group `{id}` on the {} sheet", l.week)));
        };
        for m in members {
            if !targets.contains(&m) {
                targets.push(m);
            }
        }
    }
    let named_lines: Vec<&String> = a.ids.iter().filter(|i| i.starts_with('l')).collect();
    for line in weekread::sheet_lines(&l.sheet) {
        if line["status"] == "applied" && named_lines.iter().any(|i| s(&line["id"]) == i.as_str()) {
            return Err(fail(2, format!("line {} is already applied and cannot be marked", s(&line["id"]))));
        }
    }

    let mut changed = vec![];
    for line in each_line(&mut l.sheet) {
        if !targets.iter().any(|t| t == s(&line["id"])) || line["status"] == "applied" {
            continue;
        }
        if verdict == "cut" {
            cut_line(line);
        } else {
            line["verdict"] = json!("approve");
            if line["status"] == "cut" {
                line["status"] = json!("open");
                line["error"] = Value::Null;
            }
        }
        if let Some(n) = &a.note {
            line["note"] = json!(n.trim());
        }
        changed.push(json!({ "id": line["id"], "verdict": line["verdict"], "status": line["status"] }));
    }
    if let Some(f) = &a.feel {
        l.sheet["feel"] = json!(f);
    }
    refresh_group_cuts(&mut l.sheet);
    save(&l)?;
    Ok(json!({ "promptu": 1, "week": l.week, "changed": changed, "feel": l.sheet["feel"], "open": open_count(&l.sheet) }))
}

pub fn mark_text(v: &Value) -> String {
    let mut out = String::new();
    for c in v["changed"].as_array().into_iter().flatten() {
        out.push_str(&format!("{}: {} ({})\n", s(&c["id"]), s(&c["verdict"]), s(&c["status"])));
    }
    if let Some(f) = v["feel"].as_str() {
        out.push_str(&format!("feel: {f}\n"));
    }
    out.push_str(&format!("{} open ({})\n", v["open"], s(&v["week"])));
    out
}

// ---- helpers for the files an apply writes ----

fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git").args(args).current_dir(dir).output().map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&o.stderr).trim()))
    }
}

fn safe_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 80 && !n.starts_with('.') && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !n.contains("..")
}

fn scratch_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("promptu-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())))
}

// ---- a memory line ----

/// The domain header a line gets in an index that has none (I2 wants every touched line under one).
const DOMAIN: &str = "## promptu";

fn index_with(old: &str, line: &str, file: &str) -> String {
    let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
    match lines.iter().position(|l| checks::is_index_line(l) && checks::link_target(l).as_deref() == Some(file)) {
        Some(i) => lines[i] = line.to_string(),
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            if !lines.iter().any(|l| l.starts_with("## ")) {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push(DOMAIN.to_string());
                lines.push(String::new());
            }
            lines.push(line.to_string());
        }
    }
    lines.join("\n") + "\n"
}

fn errors(findings: Vec<checks::Finding>) -> Vec<String> {
    findings.into_iter().filter(|f| f.severity == Severity::Error).map(|f| format!("{}: {}", f.check, f.message)).collect()
}

/// Backup then write, or only check with `write` false. Nothing is written unless both files pass.
fn apply_memory(line: &Value, week_dir: &Path, write: bool) -> Result<(), String> {
    let t = &line["target"];
    let (project, file) = (s(&t["project"]), s(&t["file"]));
    if !safe_name(project) || !safe_name(file) || !file.ends_with(".md") || file == "MEMORY.md" {
        return Err(format!("`{project}/{file}` is not a project folder and topic file"));
    }
    let text = match line["proposed_text"].as_str().filter(|x| !x.trim().is_empty()) {
        Some(x) => x.to_string(),
        None => std::fs::read_to_string(week_dir.join(s(&line["proposed"]))).map_err(|e| format!("no proposed topic file: {e}"))?,
    };
    let index_line = s(&line["index_line"]).trim().to_string();
    if !checks::is_index_line(&index_line) {
        return Err("the line has no `MEMORY.md` index line".into());
    }
    let base = claude::memory_base().ok_or("no Claude Code config folder")?;
    let project_dir = base.join("projects").join(project);
    if !project_dir.is_dir() {
        return Err(format!("no memory folder for project `{project}`"));
    }
    let dir = project_dir.join("memory");
    let topic_path = dir.join(file);
    let index_path = dir.join("MEMORY.md");
    let old_topic = std::fs::read_to_string(&topic_path).ok();
    let old_index = std::fs::read_to_string(&index_path).ok();
    let today = config::today();

    let topic_errors = errors(checks::topic(old_topic.as_deref().map_or(Before::Absent, Before::Text), &text, today));
    if !topic_errors.is_empty() {
        return Err(topic_errors.join("; "));
    }
    let new_index = index_with(old_index.as_deref().unwrap_or(""), &index_line, file);
    let index_errors = errors(checks::index(old_index.as_deref().map_or(Before::Absent, Before::Text), &new_index, Some(&dir)));
    if !index_errors.is_empty() {
        return Err(index_errors.join("; "));
    }
    if !write {
        return Ok(());
    }
    let backup = week_dir.join("backup").join(project);
    for (path, old) in [(&topic_path, &old_topic), (&index_path, &old_index)] {
        if let (Some(old), Some(name)) = (old, path.file_name()) {
            let b = backup.join(name);
            if !b.exists() {
                state::write_atomic(&b, old)?;
            }
        }
    }
    state::write_atomic(&topic_path, &text)?;
    state::write_atomic(&index_path, &new_index)
}

// ---- a pattern line ----

/// `text` with the frontmatter's `version` set (added after `name:` when absent).
fn with_version(text: &str, version: u32) -> String {
    let Some((front, _)) = pattern::split(text) else { return text.to_string() };
    let new_line = format!("version: {version}");
    let mut done = false;
    let mut lines: Vec<String> = front
        .lines()
        .map(|l| {
            if !done && l.starts_with("version:") {
                done = true;
                new_line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !done {
        let at = lines.iter().position(|l| l.starts_with("name:")).map_or(0, |i| i + 1);
        lines.insert(at, new_line);
    }
    let rest = &text["---\n".len() + front.len()..];
    format!("---\n{}{rest}", lines.join("\n"))
}

struct Group {
    name: String,
    /// Indices of its lines: (group index, line index).
    at: Vec<(usize, usize)>,
    ids: Vec<String>,
    title: String,
    proposed: String,
}

/// The files an approved pattern change makes in `root` (a checkout or a scratch copy): the
/// pattern with its version bumped, and its golden example renders when `tests/golden` exists.
fn write_pattern(root: &Path, g: &Group, today: &str) -> Result<(), String> {
    let file = root.join("patterns").join(format!("{}.md", g.name));
    let version = match std::fs::read_to_string(&file) {
        Ok(old) => pattern::parse(&file.to_string_lossy(), &old).map_err(|e| format!("the current {} does not parse: {e}", g.name))?.front.version + 1,
        Err(_) => pattern::parse(&format!("{}.md", g.name), &g.proposed).map(|p| p.front.version.max(1)).unwrap_or(1),
    };
    state::write_atomic(&file, &with_version(&g.proposed, version))?;
    let set = Set::from_dir(&root.join("patterns")).map_err(|e| e.to_string())?;
    let parts: Vec<String> = pattern::DETECTED.iter().map(|p| p.to_string()).collect();
    let (report, show) = lint::lint(&set, None, today, &parts);
    let own = format!("{}.md", g.name);
    let bad: Vec<String> = report
        .files
        .iter()
        .filter(|f| f.name.as_deref() == Some(g.name.as_str()) || Path::new(&f.path).file_name().is_some_and(|n| n.to_string_lossy() == own))
        .flat_map(|f| &f.findings)
        .filter(|f| f.severity == "error")
        .map(|f| format!("{}: {}", f.check, f.message))
        .collect();
    if !bad.is_empty() {
        return Err(bad.join("; "));
    }
    let golden = root.join("tests/golden");
    if golden.is_dir() {
        let head = format!("######## {} example ", g.name);
        let mut n = 0;
        for entry in show.iter().filter(|e| e.starts_with(&head)) {
            n += 1;
            let body = entry.split_once('\n').map_or("", |x| x.1).trim_end().to_string() + "\n";
            state::write_atomic(&golden.join(format!("{}-{n}.md", g.name)), &body)?;
        }
        while golden.join(format!("{}-{}.md", g.name, n + 1)).exists() {
            n += 1;
            let _ = std::fs::remove_file(golden.join(format!("{}-{n}.md", g.name)));
        }
    }
    Ok(())
}

/// The configured public-safety command, run without a shell in `root`; its output is the error.
fn public_check(table: &toml::Table, root: &Path) -> Result<(), String> {
    let Some(cmd) = table.get("week_public_check").and_then(|v| v.as_str()).filter(|c| !c.trim().is_empty()) else { return Ok(()) };
    let words: Vec<String> = cmd.split_whitespace().map(|w| w.replace("{checkout}", &root.to_string_lossy())).collect();
    let o = Command::new(&words[0]).args(&words[1..]).current_dir(root).output().map_err(|e| format!("public-safety command `{}`: {e}", words[0]))?;
    if o.status.success() {
        return Ok(());
    }
    let out = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    Err(format!("public-safety command failed: {}", if out.is_empty() { format!("exit {}", o.status.code().unwrap_or(-1)) } else { out }))
}

/// The pattern changes as a patch against the patterns this binary uses (`git diff --no-index`).
fn make_patch(groups: &[&Group], base: &Set, today: &str) -> Result<String, String> {
    let tmp = scratch_dir("patch");
    let result = (|| {
        let mut patch = String::new();
        for g in groups {
            let (a, b) = (tmp.join("a/patterns"), tmp.join("b/patterns"));
            std::fs::create_dir_all(&a).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&b).map_err(|e| e.to_string())?;
            let old = base.get(&g.name).and_then(|p| match p.file.strip_prefix("embedded:") {
                Some(n) => pattern::EMBEDDED.iter().find(|(x, _)| *x == n).map(|(_, t)| t.to_string()),
                None => std::fs::read_to_string(&p.file).ok(),
            });
            let version = match &old {
                Some(o) => pattern::parse(&format!("{}.md", g.name), o).map_err(|e| e.to_string())?.front.version + 1,
                None => pattern::parse(&format!("{}.md", g.name), &g.proposed).map(|p| p.front.version.max(1)).unwrap_or(1),
            };
            let new = with_version(&g.proposed, version);
            let parsed = pattern::parse(&format!("{}.md", g.name), &new).map_err(|e| format!("the proposed file does not parse: {e}"))?;
            let mut set = base.clone();
            set.patterns.insert(g.name.clone(), parsed);
            let parts: Vec<String> = pattern::DETECTED.iter().map(|p| p.to_string()).collect();
            let (report, _) = lint::lint(&set, None, today, &parts);
            let own = report.files.iter().filter(|f| f.name.as_deref() == Some(g.name.as_str())).flat_map(|f| &f.findings).filter(|f| f.severity == "error");
            let bad: Vec<String> = own.map(|f| format!("{}: {}", f.check, f.message)).collect();
            if !bad.is_empty() {
                return Err(bad.join("; "));
            }
            let name = format!("{}.md", g.name);
            std::fs::write(b.join(&name), &new).map_err(|e| e.to_string())?;
            let from = match &old {
                Some(o) => {
                    std::fs::write(a.join(&name), o).map_err(|e| e.to_string())?;
                    format!("a/patterns/{name}")
                }
                None => "/dev/null".to_string(),
            };
            let o = Command::new("git").args(["diff", "--no-index", "--no-color", &from, &format!("b/patterns/{name}")]).current_dir(&tmp).output().map_err(|e| format!("git: {e}"))?;
            patch.push_str(&String::from_utf8_lossy(&o.stdout));
            std::fs::remove_dir_all(tmp.join("a")).ok();
            std::fs::remove_dir_all(tmp.join("b")).ok();
        }
        Ok(patch)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// Commit each pattern group on `promptu/week-<week>` in `checkout`. A group that fails is
/// reverted; the branch the owner was on is checked out again at the end. `Err` stops the whole apply.
fn commit_groups(checkout: &Path, week: &str, groups: &[Group], table: &toml::Table, today: &str) -> Result<Vec<Result<String, String>>, String> {
    if !checkout.join(".git").exists() {
        return Err(format!("week_checkout {} is not a git checkout", checkout.display()));
    }
    if !run_git(checkout, &["status", "--porcelain"])?.is_empty() {
        return Err(format!("week_checkout {} is dirty; commit or stash first (nothing was changed)", checkout.display()));
    }
    let branch = format!("promptu/week-{week}");
    let original = run_git(checkout, &["symbolic-ref", "--short", "-q", "HEAD"]).or_else(|_| run_git(checkout, &["rev-parse", "HEAD"]))?;
    if run_git(checkout, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]).is_ok() {
        run_git(checkout, &["checkout", "-q", &branch])?;
    } else {
        run_git(checkout, &["checkout", "-q", "-b", &branch])?;
    }
    let mut out = vec![];
    for g in groups {
        let r = (|| {
            write_pattern(checkout, g, today)?;
            public_check(table, checkout)?;
            run_git(checkout, &["add", "-A"])?;
            if run_git(checkout, &["status", "--porcelain"])?.is_empty() {
                return run_git(checkout, &["rev-parse", "HEAD"]);
            }
            let title: String = g.title.lines().next().unwrap_or("").chars().take(72).collect();
            run_git(checkout, &["commit", "-q", "-m", &format!("promptu: {title} ({week} {})", g.ids.join(", "))])?;
            run_git(checkout, &["rev-parse", "HEAD"])
        })();
        if r.is_err() {
            let _ = run_git(checkout, &["reset", "-q", "--hard"]);
            let _ = run_git(checkout, &["clean", "-fdq"]);
        }
        out.push(r);
    }
    run_git(checkout, &["checkout", "-q", &original])?;
    Ok(out)
}

/// `--dry-run`: the same steps on a copy of the checkout's tracked files, which is then thrown away.
fn dry_groups(checkout: &Path, groups: &[Group], table: &toml::Table, today: &str) -> Result<Vec<Result<String, String>>, String> {
    let tmp = scratch_dir("dry");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        let tar = tmp.join("copy.tar");
        run_git(checkout, &["archive", "-o", &tar.to_string_lossy(), "HEAD"])?;
        let work = tmp.join("work");
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let o = Command::new("tar").args(["-xf", &tar.to_string_lossy(), "-C", &work.to_string_lossy()]).output().map_err(|e| format!("tar: {e}"))?;
        if !o.status.success() {
            return Err(format!("tar: {}", String::from_utf8_lossy(&o.stderr).trim()));
        }
        Ok(groups.iter().map(|g| write_pattern(&work, g, today).and_then(|_| public_check(table, &work)).map(|_| String::new())).collect())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

// ---- week apply ----

pub struct ApplyArgs {
    pub week: Option<String>,
    pub dry_run: bool,
}

/// `promptu week apply`: the output and the exit code (1 when a line failed).
pub fn apply(a: &ApplyArgs) -> Result<(Value, i32), Fail> {
    let table = config::table();
    let mut l = load(a.week.as_deref())?;
    let md_changed = read_back_md(&mut l);
    let today = config::today().to_string();
    let write = !a.dry_run;

    // Every approved line still open or failed, in sheet order.
    let mut todo: Vec<(usize, usize)> = vec![];
    let groups_json = l.sheet["groups"].as_array().cloned().unwrap_or_default();
    for (gi, g) in groups_json.iter().enumerate() {
        for (li, line) in g["lines"].as_array().into_iter().flatten().enumerate() {
            if line["verdict"] == "approve" && matches!(s(&line["status"]), "open" | "failed") {
                todo.push((gi, li));
            }
        }
    }
    let line_at = |gi: usize, li: usize| -> &Value { &groups_json[gi]["lines"][li] };

    let mut results: Vec<(usize, usize, Result<Option<String>, String>)> = vec![];
    let mut skipped = 0;
    let mut pattern_groups: Vec<Group> = vec![];
    for &(gi, li) in &todo {
        let line = line_at(gi, li);
        match s(&line["kind"]) {
            "memory" => results.push((gi, li, apply_memory(line, &l.dir, write).map(|_| None))),
            "pattern" | "new-pattern" => {
                let name = s(&line["target"]["pattern"]).to_string();
                let proposed = std::fs::read_to_string(l.dir.join(s(&line["proposed"])));
                match (safe_name(&name), proposed) {
                    (true, Ok(p)) => {
                        let id = s(&line["id"]).to_string();
                        match pattern_groups.iter_mut().find(|g| g.name == name) {
                            Some(g) => {
                                g.at.push((gi, li));
                                g.ids.push(id);
                            }
                            None => pattern_groups.push(Group { name, at: vec![(gi, li)], ids: vec![id], title: s(&line["text"]).to_string(), proposed: p }),
                        }
                    }
                    (false, _) => results.push((gi, li, Err(format!("`{name}` is not a pattern name")))),
                    (true, Err(e)) => results.push((gi, li, Err(format!("no proposed file {}: {e}", s(&line["proposed"]))))),
                }
            }
            _ => skipped += 1,
        }
    }

    let checkout = table.get("week_checkout").and_then(|v| v.as_str()).filter(|c| !c.trim().is_empty()).map(PathBuf::from);
    let mut branch = None;
    let mut patch = None;
    if !pattern_groups.is_empty() {
        match &checkout {
            Some(c) if write => {
                let rs = commit_groups(c, &l.week, &pattern_groups, &table, &today).map_err(|e| fail(1, e))?;
                branch = Some(format!("promptu/week-{}", l.week));
                for (g, r) in pattern_groups.iter().zip(rs) {
                    for &(gi, li) in &g.at {
                        results.push((gi, li, r.clone().map(Some)));
                    }
                }
            }
            Some(c) => {
                let rs = dry_groups(c, &pattern_groups, &table, &today).map_err(|e| fail(1, e))?;
                for (g, r) in pattern_groups.iter().zip(rs) {
                    for &(gi, li) in &g.at {
                        results.push((gi, li, r.clone().map(|_| None)));
                    }
                }
            }
            None => {
                let base = Set::load().map_err(|e| fail(2, e))?;
                let mut ok: Vec<&Group> = vec![];
                for g in &pattern_groups {
                    match make_patch(&[g], &base, &today) {
                        Ok(_) => ok.push(g),
                        Err(e) => {
                            for &(gi, li) in &g.at {
                                results.push((gi, li, Err(e.clone())));
                            }
                        }
                    }
                }
                if !ok.is_empty() {
                    // The patch holds this sheet's earlier applied changes too, so it is the whole of what was approved.
                    let mut all: Vec<Group> = vec![];
                    for (gi, g) in groups_json.iter().enumerate() {
                        for (li, line) in g["lines"].as_array().into_iter().flatten().enumerate() {
                            let done_before = line["status"] == "applied" && line["commit"].is_null() && matches!(s(&line["kind"]), "pattern" | "new-pattern");
                            let now = ok.iter().any(|g| g.at.contains(&(gi, li)));
                            let name = s(&line["target"]["pattern"]).to_string();
                            if !(done_before || now) || !safe_name(&name) {
                                continue;
                            }
                            if all.iter().any(|x| x.name == name) {
                                continue;
                            }
                            let proposed = ok.iter().find(|g| g.name == name).map(|g| g.proposed.clone()).or_else(|| std::fs::read_to_string(l.dir.join(s(&line["proposed"]))).ok());
                            if let Some(proposed) = proposed {
                                all.push(Group { name, at: vec![], ids: vec![], title: String::new(), proposed });
                            }
                        }
                    }
                    let refs: Vec<&Group> = all.iter().collect();
                    match make_patch(&refs, &base, &today) {
                        Ok(p) if write => {
                            let path = l.dir.join("patterns.patch");
                            state::write_atomic(&path, &p).map_err(internal)?;
                            patch = Some(path.to_string_lossy().to_string());
                            for g in &ok {
                                for &(gi, li) in &g.at {
                                    results.push((gi, li, Ok(None)));
                                }
                            }
                        }
                        Ok(_) => {
                            for g in &ok {
                                for &(gi, li) in &g.at {
                                    results.push((gi, li, Ok(None)));
                                }
                            }
                        }
                        Err(e) => {
                            for g in &ok {
                                for &(gi, li) in &g.at {
                                    results.push((gi, li, Err(e.clone())));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    results.sort_by_key(|r| (r.0, r.1));
    let (mut applied, mut failed) = (0, 0);
    let mut out_lines = vec![];
    for (gi, li, r) in &results {
        let line = &mut l.sheet["groups"][*gi]["lines"][*li];
        match r {
            Ok(commit) => {
                applied += 1;
                if write {
                    line["status"] = json!("applied");
                    line["error"] = Value::Null;
                    line["commit"] = json!(commit);
                }
                out_lines.push(json!({ "id": line["id"], "status": "applied", "commit": commit, "error": null }));
            }
            Err(e) => {
                failed += 1;
                if write {
                    line["status"] = json!("failed");
                    line["error"] = json!(e);
                }
                out_lines.push(json!({ "id": line["id"], "status": "failed", "commit": null, "error": e }));
            }
        }
    }
    if write && (!results.is_empty() || md_changed) {
        save(&l)?;
    }
    let v = json!({ "promptu": 1, "week": l.week, "applied": applied, "failed": failed, "skipped": skipped, "branch": branch, "patch": patch, "lines": out_lines });
    Ok((v, if failed > 0 { 1 } else { 0 }))
}

pub fn apply_text(v: &Value) -> String {
    let mut out = String::new();
    for l in v["lines"].as_array().into_iter().flatten() {
        match (l["commit"].as_str(), l["error"].as_str()) {
            (_, Some(e)) => out.push_str(&format!("{}: failed: {e}\n", s(&l["id"]))),
            (Some(c), _) if !c.is_empty() => out.push_str(&format!("{}: applied ({})\n", s(&l["id"]), &c[..c.len().min(10)])),
            _ => out.push_str(&format!("{}: applied\n", s(&l["id"]))),
        }
    }
    out.push_str(&format!("{} applied, {} failed, {} skipped ({})\n", v["applied"], v["failed"], v["skipped"], s(&v["week"])));
    if let Some(b) = v["branch"].as_str() {
        out.push_str(&format!("branch: {b} (not pushed)\n"));
    }
    if let Some(p) = v["patch"].as_str() {
        out.push_str(&format!("patch: {p}\n"));
    }
    out
}
