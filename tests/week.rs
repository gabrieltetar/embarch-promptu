//! `promptu week pack` end to end (docs/contract.md §14.4, §14.5), against a
//! scratch Claude Code config folder, scratch Promptu state folder and a pinned
//! day, with invented log entries and transcripts. The binary makes no model
//! call here: `claude` is not on the cleared `PATH`, so a call would fail.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");
/// A Wednesday: the window is 2026-11-18 to 2026-11-24.
const TODAY: &str = "2026-11-25";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "promptu-week-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for d in ["home/work/app/.git", "claude/projects", "cfg", "state/log", "state/drafts"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        Scratch { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.join("home/work/app")
    }

    fn key(&self) -> String {
        self.repo().to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    fn log(&self, run: &str, date: &str, pattern: &str, rendered: &str) {
        let e = json!({
            "promptu": 1, "run": run, "date": date, "cwd": self.repo().to_string_lossy(), "project_key": self.key(),
            "pattern": pattern, "version": 1, "mode": "prompt", "original": "rough", "rendered": rendered,
            "rewritten": true, "rewrite_skipped": null, "rewrite_model": "sonnet", "rewrite_effort": "low", "rewrite_ms": 900, "wait_ms": 1200
        });
        std::fs::write(self.state().join("log").join(format!("{run}.json")), e.to_string()).unwrap();
    }

    fn transcript(&self, session: &str, lines: &[Value]) {
        let dir = self.root.join("claude/projects").join(self.key());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{session}.jsonl")), lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).unwrap();
    }

    fn pack(&self, extra: &[&str]) -> Output {
        Command::new(BIN)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.state())
            .env("PROMPTU_TODAY", TODAY)
            .env("TZ", "UTC")
            .current_dir(self.repo())
            .args(["week", "pack", "--json"])
            .args(extra)
            .output()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn rendered(goal: &str, footer: &str) -> String {
    format!("## Goal\n{goal}\n\n{footer}")
}

fn owner(s: &Scratch, at: &str, text: &str) -> Value {
    json!({"type": "user", "timestamp": at, "cwd": s.repo().to_string_lossy(), "message": {"role": "user", "content": text}})
}

/// A terminal run's two transcript records: `render`'s output, then the
/// "Send as is / Edit first" question and the answer it got.
fn terminal(run: &str, tag: &str, at: &str, answer: &str) -> Vec<Value> {
    vec![
        json!({"type": "user", "timestamp": at, "message": {"content": [
            {"type": "tool_result", "tool_use_id": format!("r-{tag}"), "content": format!("{{\n  \"run\": \"{run}\",\n  \"prompt\": \"…\"\n}}")}]}}),
        json!({"type": "assistant", "timestamp": at, "message": {"content": [
            {"type": "tool_use", "id": format!("q-{tag}"), "name": "AskUserQuestion", "input": {"questions": [
                {"question": "Run this prompt?", "header": "Draft", "multiSelect": false, "options": [
                    {"label": "Send as is (Recommended)", "description": "Work on it now, as written."},
                    {"label": "Edit first", "description": "Edit the draft file, then say go."},
                    {"label": "Cancel", "description": "Do nothing with it."}]}]}}]}}),
        json!({"type": "user", "timestamp": at, "message": {"content": [
            {"type": "tool_result", "tool_use_id": format!("q-{tag}"), "content": [{"type": "text", "text": format!("User answered: \"Run this prompt?\"=\"{answer}\"")}]}]}}),
    ]
}

fn read_pack(s: &Scratch, week: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(s.state().join("week").join(week).join("pack.json")).unwrap()).unwrap()
}

fn event<'a>(pack: &'a Value, run: &str) -> &'a Value {
    pack["events"].as_array().unwrap().iter().find(|e| e["run"] == run).unwrap_or_else(|| panic!("no event for {run}"))
}

#[test]
fn a_chat_run_and_a_terminal_run_are_each_found_and_an_edit_has_its_diff() {
    let s = Scratch::new("pack");
    let fa = "— promptu alpha@1 asked=board:other,pass:rec filled=flash:memory.de8cea! skipped=timebox assume=heavy pick=owner";
    let (ra, rb, rc, rd, rf) = ("20261120-090000-aaaa", "20261120-100000-bbbb", "20261121-090000-cccc", "20261121-100000-dddd", "20261122-090000-eeee");
    let ta = rendered("fix the flash", fa);
    let tb = rendered("write the report\nin two paragraphs", "— promptu beta@1 asked=tone:rec");
    let tc = rendered("review the diff", "— promptu gamma@1 asked=scope:rec");
    let td = rendered("plan the week", "— promptu delta@1 asked=scope:rec");
    s.log(ra, "2026-11-20T09:00:00+00:00", "alpha", &ta);
    s.log(rb, "2026-11-20T10:00:00+00:00", "beta", &tb);
    s.log(rc, "2026-11-21T09:00:00+00:00", "gamma", &tc);
    s.log(rd, "2026-11-21T10:00:00+00:00", "delta", &td);
    s.log(rf, "2026-11-22T09:00:00+00:00", "alpha", &rendered("nobody sent this", "— promptu alpha@1 asked=board:rec"));
    // Outside the window: not in the pack.
    s.log("20261110-090000-ffff", "2026-11-10T09:00:00+00:00", "alpha", &ta);
    // The owner's edit of the terminal run D, kept in its draft file.
    std::fs::write(s.state().join("drafts").join(format!("{rd}.md")), rendered("plan the week, Monday first", "— promptu delta@1 asked=scope:rec")).unwrap();

    let mut lines = vec![
        owner(&s, "2026-11-20T09:05:00Z", &ta),
        owner(&s, "2026-11-20T10:03:00Z", &tb.replace("in two paragraphs", "in three bullets")),
        // A tool result that only shows the prompt is not a message he sent.
        json!({"type": "user", "timestamp": "2026-11-21T08:00:00Z", "message": {"content": [{"type": "tool_result", "tool_use_id": "x", "content": tc.clone()}]}}),
    ];
    lines.extend(terminal(rc, "c", "2026-11-21T09:02:00Z", "Send as is (Recommended)"));
    lines.extend(terminal(rd, "d", "2026-11-21T10:02:00Z", "Edit first"));
    s.transcript("sess-1", &lines);

    let o = s.pack(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["week"], "2026-W48");
    assert_eq!(out["window"], json!({"from": "2026-11-18", "to": "2026-11-24"}));
    assert_eq!(out["runs"], 5);
    assert_eq!(out["events"]["run"], 5);
    let pack = read_pack(&s, "2026-W48");
    assert_eq!(pack["promptu"], 1);
    assert_eq!(pack["events"].as_array().unwrap().len(), 5);
    assert_eq!(pack["events"][0]["id"], "e1");
    assert_eq!(pack["events"][4]["id"], "e5");

    // The footer's side.
    let a = event(&pack, ra);
    assert_eq!((a["asked"].as_u64(), a["filled"].as_u64(), a["skipped"].as_u64(), a["assumed"].as_u64()), (Some(2), Some(1), Some(1), Some(1)));
    assert_eq!(a["changed_fills"], json!(["flash"]));
    assert_eq!(a["picked_by_owner"], true);
    assert_eq!(a["skipped_questions"], json!(["timebox"]));
    assert_eq!(a["assumed_questions"], json!(["heavy"]));
    assert_eq!(a["not_recommended"], json!([{"question": "board", "answer": "other"}]));
    assert_eq!(a["footer"], fa);

    // A chat-sent run, unedited: found, and no diff.
    assert_eq!(a["sent"]["source"], "transcript");
    assert_eq!(a["sent"]["session"], "sess-1");
    assert_eq!(a["edit"], "unchanged");
    assert!(a.get("diff").is_none(), "{a}");

    // A chat-sent run, edited: his edit as a diff against `rendered`.
    let b = event(&pack, rb);
    assert_eq!(b["edit"], "edited");
    let diff = b["diff"].as_str().unwrap();
    assert!(diff.contains("-in two paragraphs\n+in three bullets\n"), "{diff}");
    assert!(b["sent"]["text"].as_str().unwrap().contains("in three bullets"));

    // A terminal run sent as is: found through the answer, unchanged.
    let c = event(&pack, rc);
    assert_eq!((c["sent"]["source"].as_str(), c["sent"]["choice"].as_str(), c["edit"].as_str()), (Some("terminal"), Some("send_as_is"), Some("unchanged")));
    assert!(c.get("diff").is_none());

    // A terminal run edited first: the draft file against `rendered`.
    let d = event(&pack, rd);
    assert_eq!((d["sent"]["choice"].as_str(), d["edit"].as_str()), (Some("edit_first"), Some("edited")));
    assert!(d["diff"].as_str().unwrap().contains("+plan the week, Monday first"), "{d}");

    // Nothing says it was sent.
    let f = event(&pack, rf);
    assert!(f["sent"].is_null());
    assert_eq!(f["edit"], "unknown");

    // The pack covers through the window's last day.
    let packed: Value = serde_json::from_str(&std::fs::read_to_string(s.state().join("week/packed.json")).unwrap()).unwrap();
    assert_eq!(packed["through"], "2026-11-24");

    // The text view, and a second pack for the same week rewrites it.
    let o = Command::new(BIN)
        .env_clear()
        .env("HOME", s.root.join("home"))
        .env("CLAUDE_CONFIG_DIR", s.root.join("claude"))
        .env("PROMPTU_STATE_DIR", s.state())
        .env("PROMPTU_TODAY", TODAY)
        .args(["week", "pack"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains("5 run(s)"), "{}", String::from_utf8_lossy(&o.stdout));
}

#[test]
fn a_tie_goes_to_the_text_closest_to_rendered_and_is_said() {
    let s = Scratch::new("tie");
    let t = rendered("ship it", "— promptu alpha@1 asked=board:rec");
    s.log("20261120-090000-aaaa", "2026-11-20T09:00:00+00:00", "alpha", &t);
    s.transcript("sess-far", &[owner(&s, "2026-11-20T09:01:00Z", &t.replace("ship it", "ship it\nand then tag it\nand announce it"))]);
    s.transcript("sess-near", &[owner(&s, "2026-11-20T09:02:00Z", &t)]);
    let o = s.pack(&[]);
    assert_eq!(o.status.code(), Some(0));
    let e = &read_pack(&s, "2026-W48")["events"][0];
    assert_eq!(e["sent"]["session"], "sess-near");
    assert_eq!(e["sent"]["tie"], true);
    assert_eq!(e["edit"], "unchanged");
}

#[test]
fn a_message_before_the_run_or_in_another_project_is_not_its_send() {
    let s = Scratch::new("other");
    let t = rendered("ship it", "— promptu alpha@1 asked=board:rec");
    s.log("20261120-090000-aaaa", "2026-11-20T09:00:00+00:00", "alpha", &t);
    let mut elsewhere = owner(&s, "2026-11-20T09:05:00Z", &t);
    elsewhere["cwd"] = json!("/somewhere/else/entirely");
    s.transcript("sess-1", &[owner(&s, "2026-11-20T08:00:00Z", &t), elsewhere]);
    assert_eq!(s.pack(&[]).status.code(), Some(0));
    let e = &read_pack(&s, "2026-W48")["events"][0];
    assert!(e["sent"].is_null(), "{e}");
}

#[test]
fn a_past_week_reads_the_seven_days_before_its_scheduled_day_and_a_bad_week_or_folder_is_exit_2() {
    let s = Scratch::new("weeks");
    s.log("20261113-090000-aaaa", "2026-11-13T09:00:00+00:00", "alpha", &rendered("old", "— promptu alpha@1 asked=board:rec"));
    s.log("20261120-090000-bbbb", "2026-11-20T09:00:00+00:00", "alpha", &rendered("new", "— promptu alpha@1 asked=board:rec"));
    // 2026-W47's run day is its Wednesday, 2026-11-18: the window is 11-11 to 11-17.
    let o = s.pack(&["--week", "2026-W47"]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["window"], json!({"from": "2026-11-11", "to": "2026-11-17"}));
    assert_eq!(out["runs"], 1);
    let o = s.pack(&["--week", "2026-W48"]);
    assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap()["runs"], 1);
    assert_eq!(s.pack(&["--week", "next"]).status.code(), Some(2));
    let nope = s.root.join("nope");
    assert_eq!(s.pack(&["--projects", &nope.to_string_lossy()]).status.code(), Some(2));
    // A schedule on another weekday moves a named week's window.
    std::fs::write(s.root.join("cfg/config.toml"), "week_schedule = \"fri 08:00\"\n").unwrap();
    let out: Value = serde_json::from_slice(&s.pack(&["--week", "2026-W47"]).stdout).unwrap();
    assert_eq!(out["window"], json!({"from": "2026-11-13", "to": "2026-11-19"}));
    assert_eq!(out["runs"], 1);
    // A bad one falls back to Wednesday and says so.
    std::fs::write(s.root.join("cfg/config.toml"), "week_schedule = \"someday\"\n").unwrap();
    let out: Value = serde_json::from_slice(&s.pack(&["--week", "2026-W47"]).stdout).unwrap();
    assert_eq!(out["window"]["from"], "2026-11-11");
    assert!(out["warnings"][0].as_str().unwrap().contains("someday"));
}
