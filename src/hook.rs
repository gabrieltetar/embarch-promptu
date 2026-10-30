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
    fn findings_become_one_line_each() {
        let f = Finding { check: "I1", severity: Severity::Error, line: Some(3), message: "too long".into() };
        assert_eq!(bullets(&[&f]), "- I1 (line 3): too long");
    }
}
