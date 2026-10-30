//! "Where it's used" (docs/contract.md §5 `used`): the footers of the last
//! 30 days that name a memory entry, read from Claude Code's transcripts.
//!
//! A run is counted where its footer reaches a transcript of this project in
//! a prompt the owner sent: his own message that carries it (the chat puts
//! the rendered prompt in the draft and he sends it). A prompt only shown —
//! `promptu render`'s output in a tool result, the terminal skill's — is not
//! a use (the owner's pick, forks.md P3, decision 33). One footer counts
//! once per transcript. Only the session's own `cwd` decides the project: a
//! transcript whose folder no longer exists counts when that folder was the
//! main checkout or under it.

use crate::claude;
use crate::footer::{self, LEAD};
use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How far back a footer counts, in days.
pub const WINDOW_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Use {
    pub pattern: String,
    pub question: String,
    pub date: String,
    /// The owner changed the fill (`!` in the footer).
    pub changed: bool,
}

#[derive(Debug, Default)]
pub struct Uses {
    /// By memory id (six hex digits), newest first.
    pub by_id: BTreeMap<String, Vec<Use>>,
    /// `memory.claude-md` fills: the global `CLAUDE.md`, not one section of it.
    pub claude_md: Vec<Use>,
}

/// The transcripts Claude Code keeps: `<config home>/projects/<folder>/*.jsonl`,
/// top level only (a subagent's own transcript sits in a subfolder).
fn transcripts(oldest: NaiveDate) -> Vec<PathBuf> {
    let Some(projects) = claude::config_dir().map(|c| c.join("projects")) else { return vec![] };
    let mut out = vec![];
    for dir in std::fs::read_dir(&projects).into_iter().flatten().flatten() {
        if !dir.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if !p.extension().is_some_and(|x| x == "jsonl") || !f.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if mtime_date(&p).is_some_and(|d| d >= oldest) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn mtime_date(p: &Path) -> Option<NaiveDate> {
    let t = std::fs::metadata(p).and_then(|m| m.modified()).ok()?;
    Some(chrono::DateTime::<chrono::Local>::from(t).date_naive())
}

/// A record's `timestamp` as a local date.
fn record_date(v: &Value) -> Option<NaiveDate> {
    let t = v.get("timestamp")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(t.trim()).ok().map(|t| t.with_timezone(&chrono::Local).date_naive())
}

/// The text of a message's content: a string, or its `text` blocks.
fn texts(content: &Value) -> Vec<&str> {
    match content {
        Value::String(s) => vec![s.as_str()],
        Value::Array(a) => a.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("text")).filter_map(|b| b.get("text").and_then(Value::as_str)).collect(),
        _ => vec![],
    }
}

/// The footers a prompt the owner sent carries.
fn footers(v: &Value) -> Vec<String> {
    let mut out = vec![];
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return out;
    }
    let content = &v["message"]["content"];
    let owners = v.get("type").and_then(Value::as_str) == Some("user")
        && v.get("isMeta").and_then(Value::as_bool) != Some(true)
        && v.pointer("/origin/kind").and_then(Value::as_str).is_none_or(|k| k == "human");
    if owners {
        for t in texts(content) {
            out.extend(t.lines().map(str::trim_end).filter(|l| l.starts_with(LEAD)).map(str::to_string));
        }
    }
    out
}

/// Every use of a memory entry in this project's footers of the last 30 days.
pub fn scan(project_root: &Path, today: NaiveDate) -> Uses {
    let oldest = today - chrono::Duration::days(WINDOW_DAYS);
    let root = claude::normalize(project_root);
    let mut ours: HashMap<String, bool> = HashMap::new();
    let mut belongs = |cwd: &str| -> bool {
        if let Some(b) = ours.get(cwd) {
            return *b;
        }
        let p = Path::new(cwd);
        let b = if p.is_dir() { claude::normalize(&claude::project_root(p)) == root } else { claude::normalize(p).starts_with(&root) };
        ours.insert(cwd.to_string(), b);
        b
    };
    let mut found: Vec<(NaiveDate, usize, footer::Footer)> = vec![];
    let mut order = 0usize;
    for file in transcripts(oldest) {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let fallback = mtime_date(&file);
        let mut seen_lines: HashSet<usize> = HashSet::new();
        let mut seen_footers: HashSet<String> = HashSet::new();
        // The lead first, cheaply; a record is parsed only when it holds one.
        for (at, _) in text.match_indices(LEAD) {
            let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
            if !seen_lines.insert(start) {
                continue;
            }
            let end = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
            let Ok(v) = serde_json::from_str::<Value>(&text[start..end]) else { continue };
            let Some(cwd) = v.get("cwd").and_then(Value::as_str) else { continue };
            if !belongs(cwd) {
                continue;
            }
            let Some(date) = record_date(&v).or(fallback) else { continue };
            if date < oldest {
                continue;
            }
            for line in footers(&v) {
                if !seen_footers.insert(line.clone()) {
                    continue;
                }
                if let Ok(f) = footer::parse(&line) {
                    order += 1;
                    found.push((date, order, f));
                }
            }
        }
    }
    // Newest first; within a day, the later one first.
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut uses = Uses::default();
    for (date, _, f) in found {
        let Some((pattern, _)) = &f.pattern else { continue };
        for (q, source, changed) in &f.filled {
            let Some(id) = source.strip_prefix("memory.") else { continue };
            let u = Use { pattern: pattern.clone(), question: q.clone(), date: date.to_string(), changed: *changed };
            if id == "claude-md" {
                uses.claude_md.push(u);
            } else {
                uses.by_id.entry(id.to_string()).or_default().push(u);
            }
        }
    }
    uses
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_footer_comes_from_a_prompt_the_owner_sent_only() {
        let user = json!({"type": "user", "message": {"role": "user", "content": "## Goal\nfix it\n\n— promptu debug-failure@1 filled=symptom:memory.de8cea"}});
        assert_eq!(footers(&user), vec!["— promptu debug-failure@1 filled=symptom:memory.de8cea"]);
        let blocks = json!({"type": "user", "message": {"content": [{"type": "text", "text": "x\n— promptu none"}]}});
        assert_eq!(footers(&blocks), vec!["— promptu none"]);
        let meta = json!({"type": "user", "isMeta": true, "message": {"content": "— promptu none"}});
        assert!(footers(&meta).is_empty());
        let side = json!({"type": "user", "isSidechain": true, "message": {"content": "— promptu none"}});
        assert!(footers(&side).is_empty());
        let cli = json!({"type": "user", "origin": {"kind": "task-notification"}, "message": {"content": "— promptu none"}});
        assert!(footers(&cli).is_empty());
        let rendered = "{\n  \"footer\": \"— promptu handoff@1 asked=reader:rec filled=save_to:memory.claude-md\",\n  \"prompt\": \"## Goal\\nx\\n\\n— promptu handoff@1 asked=reader:rec filled=save_to:memory.claude-md\"\n}";
        let tool = json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": rendered}]}});
        assert!(footers(&tool).is_empty(), "render's output, shown and not sent, is no use (P3)");
        // Shown in a reply, it is Claude's text, not a use.
        let reply = json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "— promptu none"}]}});
        assert!(footers(&reply).is_empty());
    }
}
