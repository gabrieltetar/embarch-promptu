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
    assert_eq!(events_of(&pack, "run").len(), 5);
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

// ---- sessions: friction, unmatched starts, outcomes, carried lines ----

fn assistant(at: &str) -> Value {
    json!({"type": "assistant", "timestamp": at, "message": {"content": [{"type": "text", "text": "working"}]}})
}

fn interrupt(at: &str) -> Value {
    json!({"type": "user", "timestamp": at, "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}})
}

fn plan(id: &str, at: &str, refused: bool) -> Vec<Value> {
    vec![
        json!({"type": "assistant", "timestamp": at, "message": {"content": [{"type": "tool_use", "id": id, "name": "ExitPlanMode", "input": {"plan": "do it"}}]}}),
        json!({"type": "user", "timestamp": at, "message": {"content": [{"type": "tool_result", "tool_use_id": id, "is_error": refused, "content": if refused { "The user doesn't want to proceed with this tool use." } else { "User has approved your plan." }}]}}),
    ]
}

fn commit(id: &str, at: &str, sha: &str) -> Vec<Value> {
    vec![
        json!({"type": "assistant", "timestamp": at, "message": {"content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "git commit -am work"}}]}}),
        json!({"type": "user", "timestamp": at, "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": format!("[main {sha}] work\n 1 file changed")}]}}),
    ]
}

fn events_of<'a>(pack: &'a Value, kind: &str) -> Vec<&'a Value> {
    pack["events"].as_array().unwrap().iter().filter(|e| e["kind"] == kind).collect()
}

const LONG: &str = "please look into why the flash step fails on the second board and report what you find";

#[test]
fn a_session_has_its_friction_candidates_with_the_follow_up_text() {
    let s = Scratch::new("friction");
    let mut lines = vec![owner(&s, "2026-11-20T09:00:00Z", LONG), assistant("2026-11-20T09:01:00Z"), assistant("2026-11-20T09:02:00Z")];
    // An interrupt, then a real next prompt: friction. Then an interrupt and a nudge: not.
    lines.push(interrupt("2026-11-20T09:03:00Z"));
    lines.push(owner(&s, "2026-11-20T09:04:00Z", "use the other board"));
    lines.push(interrupt("2026-11-20T09:05:00Z"));
    lines.push(owner(&s, "2026-11-20T09:06:00Z", "Continue."));
    // A rejected plan, then a prompt: friction; an approved plan: none.
    lines.extend(plan("p1", "2026-11-20T09:07:00Z", true));
    lines.push(owner(&s, "2026-11-20T09:08:00Z", "plan it in two steps"));
    lines.extend(plan("p2", "2026-11-20T09:09:00Z", false));
    lines.push(owner(&s, "2026-11-20T09:10:00Z", "also add a test"));
    // A typed correction with no interrupt.
    lines.push(assistant("2026-11-20T09:11:00Z"));
    lines.push(owner(&s, "2026-11-20T09:12:00Z", "No, the other repo"));
    s.transcript("sess-f", &lines);
    let o = s.pack(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let pack = read_pack(&s, "2026-W48");
    let sessions = events_of(&pack, "session");
    assert_eq!(sessions.len(), 1);
    let e = sessions[0];
    assert_eq!((e["session"].as_str(), e["owner_messages"].as_u64(), e["interrupts"].as_u64()), (Some("sess-f"), Some(6), Some(2)));
    assert_eq!(e["promptu"], false);
    let r: Vec<(&str, Vec<&str>)> = e["redirects"].as_array().unwrap().iter().map(|r| (r["text"].as_str().unwrap(), r["signs"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect())).collect();
    assert_eq!(r, vec![("use the other board", vec!["interrupt"]), ("plan it in two steps", vec!["plan_rejected"]), ("No, the other repo", vec!["negation"])]);
    assert_eq!(e["redirects"][0]["after_assistant_messages"], 2);
    assert_eq!(e["redirects"][0]["at"], "2026-11-20T09:04:00Z");
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((out["sessions"].as_u64(), out["events"]["session"].as_u64()), (Some(1), Some(1)));
}

#[test]
fn an_unmatched_start_has_no_footer_or_footer_none_and_is_a_task_not_a_question() {
    let s = Scratch::new("unmatched");
    s.transcript("sess-a", &[owner(&s, "2026-11-19T10:00:00Z", LONG)]);
    s.transcript("sess-b", &[owner(&s, "2026-11-19T11:00:00Z", &format!("{LONG}\n\n— promptu none"))]);
    s.transcript("sess-c", &[owner(&s, "2026-11-19T12:00:00Z", "what does the flash step do?")]);
    s.transcript("sess-d", &[owner(&s, "2026-11-19T13:00:00Z", &format!("{LONG}\n\n— promptu alpha@1 asked=board:rec"))]);
    // Not in the window: 2026-11-10.
    s.transcript("sess-old", &[owner(&s, "2026-11-10T13:00:00Z", LONG)]);
    s.pack(&[]);
    let pack = read_pack(&s, "2026-W48");
    let un: Vec<(&str, &str)> = events_of(&pack, "unmatched").iter().map(|e| (e["session"].as_str().unwrap(), e["why"].as_str().unwrap())).collect();
    assert_eq!(un, vec![("sess-a", "no_footer"), ("sess-b", "footer_none")]);
    assert_eq!(events_of(&pack, "unmatched")[0]["words"], 17);
    assert_eq!(events_of(&pack, "unmatched")[0]["at"], "2026-11-19T10:00:00Z");
    assert!(events_of(&pack, "unmatched")[0]["text"].as_str().unwrap().starts_with("please look into"));
    assert_eq!(events_of(&pack, "session").len(), 4, "sessions oldest first, the old one out");
    assert_eq!(events_of(&pack, "session")[0]["session"], "sess-a");
}

#[test]
fn a_session_says_whether_it_started_with_promptu_and_which_run() {
    let s = Scratch::new("started");
    let f = "— promptu alpha@1 asked=board:rec";
    s.log("20261120-090000-aaaa", "2026-11-20T09:00:00+00:00", "alpha", &rendered("fix the flash", f));
    s.transcript("sess-p", &[owner(&s, "2026-11-20T09:05:00Z", &rendered("fix the flash", f)), assistant("2026-11-20T09:06:00Z")]);
    s.transcript("sess-n", &[owner(&s, "2026-11-20T09:07:00Z", "fix the flash"), assistant("2026-11-20T09:08:00Z")]);
    s.pack(&[]);
    let pack = read_pack(&s, "2026-W48");
    let sessions = events_of(&pack, "session");
    let p = sessions.iter().find(|e| e["session"] == "sess-p").unwrap();
    assert_eq!((p["promptu"].as_bool(), p["pattern"].as_str(), p["run"].as_str()), (Some(true), Some("alpha"), Some("20261120-090000-aaaa")));
    let n = sessions.iter().find(|e| e["session"] == "sess-n").unwrap();
    assert_eq!((n["promptu"].as_bool(), n["pattern"].is_null(), n["run"].is_null()), (Some(false), true, true));
    assert!(events_of(&pack, "unmatched").is_empty(), "a short start is no task");
}

#[test]
fn a_commit_in_a_session_or_its_subagent_is_its_outcome() {
    let s = Scratch::new("outcome");
    let mut main = vec![owner(&s, "2026-11-20T09:00:00Z", LONG)];
    main.extend(commit("c1", "2026-11-20T09:30:00Z", "1111111"));
    s.transcript("sess-m", &main);
    // A session whose only commit was made by a subagent.
    s.transcript("sess-s", &[owner(&s, "2026-11-20T10:00:00Z", LONG), assistant("2026-11-20T10:01:00Z")]);
    let sub = s.root.join("claude/projects").join(s.key()).join("sess-s/subagents");
    std::fs::create_dir_all(&sub).unwrap();
    let mut sublines = vec![];
    sublines.extend(commit("c2", "2026-11-20T10:20:00Z", "2222222"));
    sublines.extend(commit("c3", "2026-11-20T10:25:00Z", "3333333"));
    // A failed commit counts for nothing.
    sublines.push(json!({"type": "assistant", "timestamp": "2026-11-20T10:26:00Z", "message": {"content": [{"type": "tool_use", "id": "c4", "name": "Bash", "input": {"command": "git commit -m x"}}]}}));
    sublines.push(json!({"type": "user", "timestamp": "2026-11-20T10:26:01Z", "message": {"content": [{"type": "tool_result", "tool_use_id": "c4", "is_error": true, "content": "nothing to commit"}]}}));
    std::fs::write(sub.join("agent-a1.jsonl"), sublines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).unwrap();
    // No commit: no outcome.
    s.transcript("sess-x", &[owner(&s, "2026-11-20T11:00:00Z", LONG)]);
    let o = s.pack(&[]);
    let pack = read_pack(&s, "2026-W48");
    let out: Vec<(&str, u64, u64, &str)> = events_of(&pack, "outcome").iter().map(|e| (e["session"].as_str().unwrap(), e["commits"].as_u64().unwrap(), e["in_subagents"].as_u64().unwrap(), e["first"].as_str().unwrap())).collect();
    assert_eq!(out, vec![("sess-m", 1, 0, "1111111"), ("sess-s", 2, 2, "2222222")]);
    assert_eq!(events_of(&pack, "outcome")[1]["type"], "commit");
    assert_eq!(events_of(&pack, "outcome")[1]["at"], "2026-11-20T10:20:00Z");
    assert_eq!(events_of(&pack, "outcome")[1]["repo"], s.repo().to_string_lossy().as_ref());
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((out["events"]["outcome"].as_u64(), out["events"]["session"].as_u64(), out["events"]["carried"].as_u64()), (Some(2), Some(3), Some(0)));
}

#[test]
fn an_open_or_failed_line_of_the_earlier_sheet_is_carried_with_its_events() {
    let s = Scratch::new("carried");
    let put = |rel: &str, v: Value| {
        let p = s.state().join("week").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v.to_string()).unwrap();
    };
    let line = |id: &str, status: &str, events: Value| json!({"id": id, "kind": "pattern", "status": status, "verdict": "approve", "target": {"kind": "question"}, "change": "add option", "text": format!("text {id}"), "events": events, "error": null});
    put("2026-W47/pack.json", json!({"promptu": 1, "week": "2026-W47", "events": [{"id": "e1", "kind": "run", "run": "r1"}, {"id": "e2", "kind": "run", "run": "r2"}]}));
    put("2026-W47/sheet.json", json!({"promptu": 1, "week": "2026-W47", "superseded_by": null, "groups": [{"id": "g1", "lines": [
        line("l1", "open", json!(["e1"])), line("l2", "failed", json!(["e2", "e9"])), line("l3", "applied", json!(["e1"])), line("l4", "cut", json!(["e2"]))]}]}));
    let o = s.pack(&[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let pack = read_pack(&s, "2026-W48");
    let c = events_of(&pack, "carried");
    assert_eq!(c.iter().map(|e| e["from"].as_str().unwrap()).collect::<Vec<_>>(), vec!["2026-W47:l1", "2026-W47:l2"]);
    assert_eq!(c[0]["events"], json!([{"id": "e1", "kind": "run", "run": "r1"}]));
    assert_eq!(c[1]["events"], json!([{"id": "e2", "kind": "run", "run": "r2"}]), "an id the old pack lacks is left out");
    assert_eq!((c[1]["line"]["status"].as_str(), c[1]["line"]["text"].as_str()), (Some("failed"), Some("text l2")));
    assert_eq!(c[0]["id"], "e1");
    assert_eq!(c[1]["id"], "e2");
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["events"]["carried"], 2);
    // A superseded sheet is not carried; the same week's own sheet is not either.
    put("2026-W47/sheet.json", json!({"promptu": 1, "week": "2026-W47", "superseded_by": "2026-W48", "groups": [{"id": "g1", "lines": [line("l1", "open", json!(["e1"]))]}]}));
    s.pack(&[]);
    assert!(events_of(&read_pack(&s, "2026-W48"), "carried").is_empty());
}
