//! The session events of `promptu week pack` (docs/contract.md §14.5):
//! `session` (with its friction candidates), `unmatched` and `outcome`, read
//! from the transcripts with no model and no network.
//!
//! A session is a top-level `<id>.jsonl` whose first record falls in the
//! window and that holds an owner message. Its subagents' transcripts are read
//! only for commits (an outcome).

use crate::state::LogEntry;
use crate::{calls, footer, used};
use chrono::{DateTime, FixedOffset, NaiveDate};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// A task start of at most this many words is a question, not a task (§14.5).
const TASK_START_MIN_WORDS: usize = 10;
/// Text kept in a record, in characters; a longer message is cut with `…`.
const TEXT_CAP: usize = 1500;
/// Owner messages that are a nudge, not a correction: an interrupt followed by
/// one of these is not friction (forks W2). `[assumed]`.
const NUDGES: [&str; 12] = ["continue", "go", "yes", "y", "ok", "okay", "go on", "go ahead", "proceed", "yep", "sure", "do it"];

fn negation() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // `[assumed]` phrase list (forks W2): generic, not measured.
    R.get_or_init(|| Regex::new(r"^(no|nope|not that|not what|don't|do not|dont|stop|wrong|wait|actually|that's not|that is not|that isn't|revert|undo|instead)\b").unwrap())
}

fn cut(s: &str) -> String {
    if s.chars().count() > TEXT_CAP {
        format!("{}…", s.chars().take(TEXT_CAP).collect::<String>())
    } else {
        s.to_string()
    }
}

fn is_nudge(text: &str) -> bool {
    let t: String = text.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    NUDGES.contains(&t.as_str())
}

fn is_interrupt(text: &str) -> bool {
    text.trim().starts_with("[Request interrupted by user")
}

struct Redirect {
    at: String,
    after_assistant: usize,
    text: String,
    signs: Vec<&'static str>,
}

struct Session {
    id: String,
    project: String,
    started: String,
    started_at: DateTime<FixedOffset>,
    ended: String,
    cwd: String,
    owner_messages: usize,
    interrupts: usize,
    first: Option<(String, String)>,
    redirects: Vec<Redirect>,
}

fn read(path: &Path) -> Option<Session> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut s = Session {
        id: path.file_stem()?.to_string_lossy().into_owned(),
        project: path.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        started: String::new(),
        started_at: DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").ok()?,
        ended: String::new(),
        cwd: String::new(),
        owner_messages: 0,
        interrupts: 0,
        first: None,
        redirects: vec![],
    };
    let mut plans: Vec<String> = vec![];
    let (mut after_interrupt, mut plan_rejected, mut assistants) = (false, false, 0usize);
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let at = v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();
        if let Ok(t) = DateTime::parse_from_rfc3339(at.trim()) {
            if s.started.is_empty() {
                s.started = at.clone();
                s.started_at = t;
            }
            s.ended = at.clone();
        }
        let blocks = v["message"]["content"].as_array().into_iter().flatten();
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                assistants += 1;
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use") && b.get("name").and_then(Value::as_str) == Some("ExitPlanMode")) {
                    if let Some(id) = b.get("id").and_then(Value::as_str) {
                        plans.push(id.to_string());
                    }
                }
            }
            Some("user") => {
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                    let id = b.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                    let body = used::texts(&b["content"]).join("\n");
                    let refused = b.get("is_error").and_then(Value::as_bool) == Some(true) || body.contains("doesn't want to proceed");
                    if refused && plans.iter().any(|p| p == id) {
                        plan_rejected = true;
                    }
                }
                if v.get("isMeta").and_then(Value::as_bool) == Some(true) || v.pointer("/origin/kind").and_then(Value::as_str).is_some_and(|k| k != "human") {
                    continue;
                }
                let body = used::texts(&v["message"]["content"]).join("\n");
                let body = body.trim();
                if body.is_empty() {
                    continue;
                }
                if is_interrupt(body) {
                    s.interrupts += 1;
                    after_interrupt = true;
                    continue;
                }
                s.owner_messages += 1;
                if s.cwd.is_empty() {
                    s.cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
                }
                if s.first.is_none() {
                    s.first = Some((at.clone(), body.to_string()));
                } else {
                    let mut signs = vec![];
                    if after_interrupt && !is_nudge(body) {
                        signs.push("interrupt");
                    }
                    if plan_rejected {
                        signs.push("plan_rejected");
                    }
                    if negation().is_match(&body.to_lowercase()) {
                        signs.push("negation");
                    }
                    if !signs.is_empty() {
                        s.redirects.push(Redirect { at: at.clone(), after_assistant: assistants, text: cut(body), signs });
                    }
                }
                after_interrupt = false;
                plan_rejected = false;
                assistants = 0;
            }
            _ => {}
        }
    }
    (!s.started.is_empty() && s.owner_messages > 0).then_some(s)
}

/// The run a footer line belongs to: the newest log entry before `before`
/// whose rendered prompt ends in exactly this footer.
fn run_of(line: &str, before: DateTime<FixedOffset>, logs: &[LogEntry]) -> Option<String> {
    logs.iter()
        .filter(|l| l.rendered.lines().rev().map(str::trim_end).find(|x| x.starts_with(footer::LEAD)) == Some(line))
        .filter(|l| DateTime::parse_from_rfc3339(l.date.trim()).is_ok_and(|d| d <= before))
        .max_by(|a, b| a.date.cmp(&b.date))
        .map(|l| l.run.clone())
}

/// The session, unmatched and outcome events of the sessions in `files` that
/// began between `from` and `to` (local days), oldest first, numbered from
/// `first_id`.
pub fn events(files: &[std::path::PathBuf], from: NaiveDate, to: NaiveDate, logs: &[LogEntry], first_id: usize) -> Vec<Value> {
    let mut sessions: Vec<(Session, &Path)> = files
        .iter()
        .filter_map(|f| read(f).map(|s| (s, f.as_path())))
        .filter(|(s, _)| {
            let d = s.started_at.with_timezone(&chrono::Local).date_naive();
            d >= from && d <= to
        })
        .collect();
    sessions.sort_by_key(|(s, _)| s.started_at);
    let mut out: Vec<Value> = vec![];
    let mut next = first_id;
    let mut id = |out: &mut Vec<Value>, mut e: Value| {
        e["id"] = json!(format!("e{next}"));
        next += 1;
        out.push(e);
    };
    let mut runs: HashMap<String, Option<String>> = HashMap::new();
    for (s, path) in &sessions {
        let (first_at, first_text) = s.first.clone().unwrap_or_default();
        let footer_line = first_text.lines().rev().map(str::trim_end).find(|l| l.starts_with(footer::LEAD)).map(str::to_string);
        let parsed = footer_line.as_deref().and_then(|l| footer::parse(l).ok());
        let pattern = parsed.as_ref().and_then(|f| f.pattern.as_ref().map(|(p, _)| p.clone()));
        let run = match (&pattern, &footer_line) {
            (Some(_), Some(l)) => runs.entry(l.clone()).or_insert_with(|| run_of(l, s.started_at, logs)).clone(),
            _ => None,
        };
        let redirects: Vec<Value> = s.redirects.iter().map(|r| json!({ "at": r.at, "after_assistant_messages": r.after_assistant, "text": r.text, "signs": r.signs })).collect();
        id(
            &mut out,
            json!({
                "kind": "session", "session": s.id, "project": s.project, "started": s.started, "ended": s.ended,
                "owner_messages": s.owner_messages, "pattern": pattern, "run": run, "promptu": pattern.is_some(),
                "interrupts": s.interrupts, "redirects": redirects,
            }),
        );
        let words = first_text.split_whitespace().count();
        if pattern.is_none() && words > TASK_START_MIN_WORDS {
            let why = if parsed.is_some() { "footer_none" } else { "no_footer" };
            id(&mut out, json!({ "kind": "unmatched", "session": s.id, "project": s.project, "at": first_at, "text": cut(&first_text), "why": why, "words": words }));
        }
        let commits = calls::session_commits(path);
        if let Some((first, _)) = commits.first() {
            id(
                &mut out,
                json!({
                    "kind": "outcome", "session": s.id, "run": run, "type": "commit", "repo": s.cwd,
                    "commits": commits.len(), "in_subagents": commits.iter().filter(|(_, sub)| *sub).count(), "first": first.sha, "at": first.at,
                }),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nudge_is_not_a_correction_and_a_negation_opens_one() {
        for t in ["continue", "Go!", "yes.", "go ahead", "Do it"] {
            assert!(is_nudge(t), "{t}");
        }
        assert!(!is_nudge("go to the other repo"));
        for t in ["no, the other repo", "don't touch that", "actually use sed", "wait"] {
            assert!(negation().is_match(&t.to_lowercase()), "{t}");
        }
        for t in ["nothing else needed", "note the path", "also add a test"] {
            assert!(!negation().is_match(&t.to_lowercase()), "{t}");
        }
        assert!(is_interrupt("[Request interrupted by user for tool use]"));
    }
}
