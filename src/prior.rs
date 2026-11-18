//! `promptu prior`: what the earlier runs of a prompt asked and were
//! answered, read from a Claude Code transcript folder (docs/contract.md §13).
//!
//! A transcript belongs to the prompt when its first user turn is that prompt
//! (see [`same_prompt`]). Every `AskUserQuestion` call of such a session is
//! listed in order with the `tool_result` that answered it. Read-only: only
//! the top-level `*.jsonl` of the one folder it is given, never a subagent's.

use crate::footer::LEAD;
use crate::used::texts;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq)]
pub struct Option_ {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<Option_>,
    pub multi_select: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Call {
    pub questions: Vec<Question>,
    /// The `tool_result`'s text; `None` when the session ended before an answer.
    pub answer: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Session {
    pub session: String,
    /// The first user turn's `timestamp`, as written.
    pub started: String,
    pub calls: Vec<Call>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Prior {
    pub sessions: Vec<Session>,
    /// Sessions whose first turn is not the prompt but ends in the same footer
    /// line: the prompt edited, or a different run of the same pattern. Not listed.
    pub near_matches: usize,
}

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").trim().to_string()
}

/// The prompt's own footer: its last non-empty line, when that is one.
fn footer_of(text: &str) -> Option<&str> {
    text.lines().rev().map(str::trim).find(|l| !l.is_empty()).filter(|l| l.starts_with(LEAD))
}

/// How a first turn matches the prompt. `Some(true)`: it is the prompt (the
/// text equal after trimming and `\r\n` folding; a prompt that is only a
/// footer line matches a first turn ending in that footer). `Some(false)`: a
/// near-match, the same footer under other text. `None`: unrelated.
fn same_prompt(prompt: &str, first: &str) -> Option<bool> {
    let (p, f) = (norm(prompt), norm(first));
    if p == f {
        return Some(true);
    }
    let pf = footer_of(&p)?;
    let ff = footer_of(&f);
    if p == pf && ff == Some(pf) {
        return Some(true);
    }
    (ff == Some(pf)).then_some(false)
}

fn is_owner_turn(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("user")
        && v.get("isSidechain").and_then(Value::as_bool) != Some(true)
        && v.get("isMeta").and_then(Value::as_bool) != Some(true)
        && v.pointer("/origin/kind").and_then(Value::as_str).is_none_or(|k| k == "human")
}

fn questions(input: &Value) -> Vec<Question> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    input
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|q| Question {
            question: s(q, "question"),
            header: s(q, "header"),
            options: q
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|o| Option_ { label: s(o, "label"), description: s(o, "description") })
                .collect(),
            multi_select: q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

/// A tool_result's text: a string, or its `text` blocks joined.
fn result_text(block: &Value) -> String {
    texts(&block["content"]).join("\n")
}

/// One transcript: `None` is unrelated, `Some(None)` a near-match.
fn read_session(name: &str, text: &str, prompt: &str) -> Option<Option<Session>> {
    let mut first: Option<(String, String)> = None;
    let mut order: Vec<String> = vec![];
    let mut calls: HashMap<String, Vec<Question>> = HashMap::new();
    let mut answers: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if first.is_none() {
            if is_owner_turn(&v) {
                let t = texts(&v["message"]["content"]).join("\n");
                if !t.trim().is_empty() {
                    match same_prompt(prompt, &t) {
                        Some(true) => first = Some((v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string(), t)),
                        Some(false) => return Some(None),
                        None => return None,
                    }
                }
            }
            continue;
        }
        let blocks = v["message"]["content"].as_array().into_iter().flatten();
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use") && b.get("name").and_then(Value::as_str) == Some("AskUserQuestion")) {
                    let id = b.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                    if let std::collections::hash_map::Entry::Vacant(e) = calls.entry(id.clone()) {
                        order.push(id);
                        e.insert(questions(&b["input"]));
                    }
                }
            }
            Some("user") => {
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                    if let Some(id) = b.get("tool_use_id").and_then(Value::as_str) {
                        answers.entry(id.to_string()).or_insert_with(|| result_text(b));
                    }
                }
            }
            _ => {}
        }
    }
    let (started, _) = first?;
    let calls = order.into_iter().map(|id| Call { answer: answers.remove(&id), questions: calls.remove(&id).unwrap_or_default() }).collect();
    Some(Some(Session { session: name.to_string(), started, calls }))
}

/// The earlier runs of `prompt` in `folder`, oldest first.
pub fn scan(folder: &Path, prompt: &str) -> Result<Prior, String> {
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .map_err(|e| format!("{}: {e}", folder.display()))?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.path())
        .collect();
    files.sort();
    let mut out = Prior::default();
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let name = f.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        match read_session(&name, &String::from_utf8_lossy(&bytes), prompt) {
            Some(Some(s)) => out.sessions.push(s),
            Some(None) => out.near_matches += 1,
            None => {}
        }
    }
    out.sessions.sort_by(|a, b| a.started.cmp(&b.started).then(a.session.cmp(&b.session)));
    Ok(out)
}

/// The text view: per session, each call's questions, options and answer.
pub fn render(p: &Prior) -> String {
    let mut s = String::new();
    for sess in &p.sessions {
        s.push_str(&format!("session {}  started {}\n", sess.session, sess.started));
        for (i, c) in sess.calls.iter().enumerate() {
            for q in &c.questions {
                s.push_str(&format!("  {}. {}\n", i + 1, q.question));
                if !q.options.is_empty() {
                    s.push_str(&format!("     options: {}\n", q.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>().join(" | ")));
                }
            }
            match &c.answer {
                Some(a) => s.push_str(&format!("     answer: {}\n", a.replace('\n', "\n             "))),
                None => s.push_str("     answer: (none recorded)\n"),
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROMPT: &str = "## Goal\ninterview me\n\n— promptu research-explain@1 asked=a:rec";

    fn line(v: Value) -> String {
        v.to_string()
    }

    fn session(first: &str) -> String {
        [
            line(json!({"type": "user", "timestamp": "2026-11-01T10:00:00Z", "message": {"content": first}})),
            line(json!({"type": "assistant", "message": {"content": [
                {"type": "text", "text": "asking"},
                {"type": "tool_use", "id": "t1", "name": "AskUserQuestion", "input": {"questions": [
                    {"question": "Which board?", "header": "Board", "multiSelect": false, "options": [{"label": "A", "description": "first"}, {"label": "B", "description": "second"}]}]}},
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "secret"}},
                {"type": "tool_use", "id": "t3", "name": "AskUserQuestion", "input": {"questions": [{"question": "Unanswered?", "options": []}]}}]}})),
            line(json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t2", "content": "BASH OUTPUT"},
                {"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "User answered: \"Which board?\"=\"B\""}]}]}})),
        ]
        .join("\n")
    }

    #[test]
    fn lists_questions_options_and_the_answer_by_id_never_another_tools_output() {
        let s = read_session("s1", &session(PROMPT), PROMPT).unwrap().unwrap();
        assert_eq!(s.started, "2026-11-01T10:00:00Z");
        assert_eq!(s.calls.len(), 2);
        assert_eq!(s.calls[0].questions[0].options[1].label, "B");
        assert_eq!(s.calls[0].answer.as_deref(), Some("User answered: \"Which board?\"=\"B\""));
        assert_eq!(s.calls[1].answer, None);
        let text = render(&Prior { sessions: vec![s], near_matches: 0 });
        assert!(text.contains("options: A | B") && !text.contains("BASH OUTPUT") && !text.contains("secret"), "{text}");
    }

    #[test]
    fn another_prompt_is_unrelated_and_the_same_footer_under_other_text_is_a_near_match() {
        assert!(read_session("s", &session("## Goal\nsomething else\n\n— promptu debug-failure@1"), PROMPT).is_none());
        assert!(read_session("s", &session("## Goal\nedited\n\n— promptu research-explain@1 asked=a:rec"), PROMPT).unwrap().is_none());
        // CRLF and surrounding whitespace do not matter.
        let crlf = format!("{}\r\n", PROMPT.replace('\n', "\r\n"));
        assert!(read_session("s", &session(&crlf), PROMPT).unwrap().is_some());
    }

    #[test]
    fn a_footer_alone_matches_the_run_that_ended_in_it() {
        let footer = "— promptu research-explain@1 asked=a:rec";
        assert!(read_session("s", &session(PROMPT), footer).unwrap().is_some());
    }
}
