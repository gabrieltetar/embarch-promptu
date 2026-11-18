//! `promptu hook`: what the plugin's PreToolUse and PostToolUse hooks run
//! (docs/contract.md §3).
//!
//! **PreToolUse** on a Write, Edit or MultiEdit into an auto-memory folder:
//! work out the file as the write would leave it, run the checks on what the
//! write touches, and answer by mode — warn lets the write go ahead and tells
//! Claude what to fix (`additionalContext`); block refuses a write with an
//! error (exit 2, the reason on stderr, which Claude reads) and lets one with
//! only warnings through. **PostToolUse** on a `MEMORY.md` write: name the
//! topic files the index no longer links (O1), warn only.
//!
//! **PreToolUse** on a Bash command that looks like it writes a memory index:
//! the index checks cannot run on free text, so the hook warns (never refuses)
//! and names Edit as the checked route. A command that only reads gets nothing.
//!
//! **It fails open.** Bad input, an unreadable file, an edit that would not
//! apply, a panic: exit 0 and print nothing, so the write goes ahead exactly
//! as it would without the plugin. No model, no network.

use crate::checks::{self, Before, Finding, Severity};
use crate::claude;
use crate::config::{self, HookMode};
use chrono::NaiveDate;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Past this the input is not read whole, does not parse, and the write
/// goes ahead unchecked.
const MAX_INPUT: u64 = 16 << 20;

/// What the hook prints.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// Exit 0 with this JSON on stdout.
    Context(String),
    /// Exit 2 with this text on stderr: the write is refused.
    Block(String),
}

pub fn run() -> i32 {
    // A panic must not print a backtrace into the session or exit non-zero.
    std::panic::set_hook(Box::new(|_| {}));
    let code = std::panic::catch_unwind(|| {
        let mut input = String::new();
        if std::io::stdin().take(MAX_INPUT).read_to_string(&mut input).is_err() {
            return 0;
        }
        let Ok(ev) = serde_json::from_str::<Value>(&input) else { return 0 };
        match respond(&ev, config::hook_mode(), config::today()) {
            None => 0,
            Some(Reply::Context(out)) => {
                println!("{out}");
                0
            }
            Some(Reply::Block(reason)) => {
                eprintln!("{reason}");
                2
            }
        }
    });
    code.unwrap_or(0)
}

const WRITE_TOOLS: [&str; 3] = ["Write", "Edit", "MultiEdit"];

pub fn respond(ev: &Value, mode: HookMode, today: NaiveDate) -> Option<Reply> {
    if mode == HookMode::Off {
        return None;
    }
    let event = ev["hook_event_name"].as_str()?;
    let tool = ev["tool_name"].as_str()?;
    if tool == "Bash" {
        return if event == "PreToolUse" { bash(ev) } else { None };
    }
    if !WRITE_TOOLS.contains(&tool) {
        return None;
    }
    let input = &ev["tool_input"];
    let file_path = input["file_path"].as_str()?;
    let cwd = ev["cwd"].as_str().map(PathBuf::from).or_else(|| std::env::current_dir().ok());
    let target = claude::memory_target(file_path, cwd.as_deref())?;
    let path = target.dir.join(&target.name);
    match event {
        "PreToolUse" => pre(tool, input, &path, &target, mode, today),
        "PostToolUse" if target.is_index => post(&path, &target),
        _ => None,
    }
}

/// Words and marks that make a shell command more than a read of a file.
const WRITE_WORDS: [&str; 9] = ["tee", "mv", "cp", "sed", "perl", "truncate", "install", "dd", "ed"];

/// Does the command look like it writes (a redirect, or a command that does)?
/// A heuristic on free text: a false positive costs one warning.
fn looks_like_write(cmd: &str) -> bool {
    let seps = |c: char| c.is_whitespace() || "\"'`;|&()<>=,".contains(c);
    let redirect = cmd.replace("2>&1", "").replace(">/dev/null", "").replace("> /dev/null", "").contains('>');
    redirect
        || cmd.split(seps).any(|w| WRITE_WORDS.contains(&w))
        || cmd.contains("write")
        || cmd.contains("open(")
        || cmd.contains("fs.")
}

/// The memory indexes a Bash command names, resolved the way a Write's
/// `file_path` is (`claude::memory_target`).
fn named_indexes(cmd: &str, cwd: Option<&Path>) -> Vec<claude::Target> {
    let home = std::env::var("HOME").ok();
    let seps = |c: char| c.is_whitespace() || "\"'`;|&()<>=,".contains(c);
    let mut out: Vec<claude::Target> = vec![];
    for tok in cmd.split(seps).filter(|t| t.ends_with("MEMORY.md")) {
        let expanded = match (&home, tok.strip_prefix("~/").or_else(|| tok.strip_prefix("$HOME/")).or_else(|| tok.strip_prefix("${HOME}/"))) {
            (Some(h), Some(rest)) => format!("{h}/{rest}"),
            _ => tok.to_string(),
        };
        if let Some(t) = claude::memory_target(&expanded, cwd) {
            if t.is_index && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

fn bash(ev: &Value) -> Option<Reply> {
    let cmd = ev["tool_input"]["command"].as_str()?;
    if !cmd.contains("MEMORY.md") || !looks_like_write(cmd) {
        return None;
    }
    let cwd = ev["cwd"].as_str().map(PathBuf::from).or_else(|| std::env::current_dir().ok());
    let found = named_indexes(cmd, cwd.as_deref());
    let first = found.first()?;
    let text = format!(
        "Promptu: this Bash command looks like it writes the memory index {}. A shell write skips the index checks (line length, domain header, one line per file). It goes ahead; write index lines with the Edit tool instead, which is checked, and re-read the file after this command.",
        first.dir.join(&first.name).display()
    );
    Some(Reply::Context(context("PreToolUse", &text)))
}

fn pre(tool: &str, input: &Value, path: &Path, target: &claude::Target, mode: HookMode, today: NaiveDate) -> Option<Reply> {
    let before = read_before(path)?;
    let after = after_write(tool, input, before.as_deref())?;
    let b = match &before {
        Some(t) => Before::Text(t),
        None => Before::Absent,
    };
    let findings = if target.is_index { checks::index(b, &after, Some(&target.dir)) } else { checks::topic(b, &after, today) };
    if findings.is_empty() {
        return None;
    }
    let errors: Vec<&Finding> = findings.iter().filter(|f| f.severity == Severity::Error).collect();
    let warnings: Vec<&Finding> = findings.iter().filter(|f| f.severity == Severity::Warning).collect();
    if mode == HookMode::Block && !errors.is_empty() {
        let mut text = format!("Promptu refused this write to the memory file {}. Fix these and write it again:\n{}", target.name, bullets(&errors));
        if !warnings.is_empty() {
            text.push_str(&format!("\nAlso worth fixing (not blocking):\n{}", bullets(&warnings)));
        }
        return Some(Reply::Block(text));
    }
    let lead = if mode == HookMode::Warn && !errors.is_empty() {
        format!("Promptu checked this write to the memory file {} (warn mode): it goes ahead, but fix these with your next edit to it:", target.name)
    } else {
        format!("Promptu checked this write to the memory file {}: it goes ahead. Worth fixing:", target.name)
    };
    let all: Vec<&Finding> = findings.iter().collect();
    Some(Reply::Context(context("PreToolUse", &format!("{lead}\n{}", bullets(&all)))))
}

fn post(path: &Path, target: &claude::Target) -> Option<Reply> {
    let index = std::fs::read_to_string(path).ok()?;
    let lost = checks::orphans(&target.dir, &index);
    if lost.is_empty() {
        return None;
    }
    Some(Reply::Context(context(
        "PostToolUse",
        &format!("Promptu (O1): MEMORY.md links none of these memory files; delete each, or give it an index line: {}", lost.join(", ")),
    )))
}

fn context(event: &str, text: &str) -> String {
    json!({ "hookSpecificOutput": { "hookEventName": event, "additionalContext": text } }).to_string()
}

fn bullets(findings: &[&Finding]) -> String {
    findings
        .iter()
        .map(|f| match f.line {
            Some(n) => format!("- {} (line {n}): {}", f.check, f.message),
            None => format!("- {}: {}", f.check, f.message),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The file before the write: `Some(None)` when it does not exist yet,
/// `None` when it cannot be read as a plain text file (the hook then stays
/// out of the way).
fn read_before(path: &Path) -> Option<Option<String>> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(None),
        Err(_) => None,
        Ok(m) if !m.is_file() => None,
        Ok(_) => std::fs::read_to_string(path).ok().map(Some),
    }
}

/// The file as the write would leave it, or `None` when Claude Code would
/// refuse the write itself (an Edit whose `old_string` is missing or
/// ambiguous) or the input is not the shape expected.
pub fn after_write(tool: &str, input: &Value, before: Option<&str>) -> Option<String> {
    match tool {
        "Write" => input["content"].as_str().map(str::to_string),
        "Edit" => edit(before, input),
        "MultiEdit" => {
            let mut now = before.map(str::to_string);
            for e in input["edits"].as_array()? {
                now = Some(edit(now.as_deref(), e)?);
            }
            now
        }
        _ => None,
    }
}

fn edit(before: Option<&str>, e: &Value) -> Option<String> {
    let old = e["old_string"].as_str()?;
    let new = e["new_string"].as_str()?;
    let all = e["replace_all"].as_bool().unwrap_or(false);
    let Some(text) = before else {
        // An empty old_string on a missing file creates it.
        return old.is_empty().then(|| new.to_string());
    };
    if old.is_empty() {
        return None;
    }
    match text.matches(old).count() {
        0 => None,
        1 => Some(text.replacen(old, new, 1)),
        _ if all => Some(text.replace(old, new)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_apply_as_claude_code_applies_them() {
        let e = |o: &str, n: &str, all: bool| json!({ "old_string": o, "new_string": n, "replace_all": all });
        assert_eq!(edit(Some("a b a"), &e("b", "c", false)).as_deref(), Some("a c a"));
        assert_eq!(edit(Some("a b a"), &e("a", "x", false)), None);
        assert_eq!(edit(Some("a b a"), &e("a", "x", true)).as_deref(), Some("x b x"));
        assert_eq!(edit(Some("a"), &e("z", "x", false)), None);
        assert_eq!(edit(None, &e("", "new", false)).as_deref(), Some("new"));
        assert_eq!(edit(None, &e("a", "new", false)), None);
        let multi = json!({ "edits": [ {"old_string": "a", "new_string": "b"}, {"old_string": "b", "new_string": "c", "replace_all": true} ] });
        assert_eq!(after_write("MultiEdit", &multi, Some("a.b")).as_deref(), Some("c.c"));
        assert_eq!(after_write("Write", &json!({ "content": "x" }), None).as_deref(), Some("x"));
        assert_eq!(after_write("NotebookEdit", &json!({}), None), None);
    }

    #[test]
    fn write_heuristic_separates_reads_from_writes() {
        for w in ["echo '- x' >> MEMORY.md", "sed -i 's/a/b/' MEMORY.md", "python3 -c \"open('MEMORY.md','w').write(t)\"", "cp /tmp/x MEMORY.md", "cat x | tee MEMORY.md"] {
            assert!(looks_like_write(w), "{w}");
        }
        for r in ["cat MEMORY.md", "grep -n tooling MEMORY.md 2>&1", "wc -l MEMORY.md > /dev/null", "head -5 MEMORY.md; ls"] {
            assert!(!looks_like_write(r), "{r}");
        }
    }

    #[test]
    fn findings_become_one_line_each() {
        let f = Finding { check: "I1", severity: Severity::Error, line: Some(3), message: "too long".into() };
        assert_eq!(bullets(&[&f]), "- I1 (line 3): too long");
    }
}
