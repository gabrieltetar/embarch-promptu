//! `promptu week draft` end to end (docs/contract.md §14.4, §10), against a
//! scratch state folder, a pinned day, a hand-built pack and a fake `claude`
//! (`PROMPTU_CLAUDE`) that records what it was sent and prints a canned reply.
//! No model is called and nothing reads the real `~/.claude` or `~/.local/state`.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");
const TODAY: &str = "2026-11-25";
const WEEK: &str = "2026-W48";
const HANDOFF: &str = include_str!("../patterns/handoff.md");

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "promptu-draft-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for d in ["home", "claude/projects", "cfg", "state/week/2026-W48", "fake"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let fake = root.join("fake/claude");
        std::fs::write(
            &fake,
            "#!/bin/sh\ncat > \"$PROMPTU_FAKE_DIR/stdin.txt\"\nprintf '%s\\n' \"$@\" > \"$PROMPTU_FAKE_DIR/argv.txt\"\nenv > \"$PROMPTU_FAKE_DIR/env.txt\"\nif [ -n \"$PROMPTU_FAKE_SLEEP\" ]; then sleep \"$PROMPTU_FAKE_SLEEP\"; fi\nif [ -f \"$PROMPTU_FAKE_DIR/exit\" ]; then exit 1; fi\ncat \"$PROMPTU_FAKE_DIR/reply.json\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Scratch { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    fn week(&self) -> PathBuf {
        self.state().join("week").join(WEEK)
    }

    /// An invented pack: three run events.
    fn pack(&self) {
        let run = |id: &str, skipped: &[&str]| json!({ "id": id, "kind": "run", "run": format!("run-{id}"), "date": "2026-11-18", "pattern": "handoff", "asked": 2, "filled": 1, "skipped": skipped.len(), "assumed": 0, "skipped_questions": skipped, "changed_fills": [], "picked_by_owner": false, "edit": "unchanged" });
        let pack = json!({ "promptu": 1, "week": WEEK, "window": { "from": "2026-11-18", "to": "2026-11-24" }, "built": "2026-11-25T08:00:00+00:00", "events": [ run("e1", &["reader"]), run("e2", &["reader"]), run("e3", &[]) ] });
        std::fs::write(self.week().join("pack.json"), pack.to_string()).unwrap();
    }

    fn reply(&self, v: Value) {
        let env = json!({"type": "result", "is_error": false, "terminal_reason": "completed", "result": v.to_string(), "total_cost_usd": 1.25});
        std::fs::write(self.root.join("fake/reply.json"), env.to_string()).unwrap();
    }

    fn draft(&self, extra: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(BIN)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.state())
            .env("PROMPTU_CLAUDE", self.root.join("fake/claude"))
            .env("PROMPTU_FAKE_DIR", self.root.join("fake"))
            .env("PROMPTU_TODAY", TODAY)
            .env("TZ", "UTC")
            .envs(env.iter().copied())
            .args(["week", "draft", "--week", WEEK, "--json"])
            .args(extra)
            .output()
            .unwrap()
    }

    fn fake(&self, what: &str) -> String {
        std::fs::read_to_string(self.root.join("fake").join(what)).unwrap_or_default()
    }

    fn sheet(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.week().join("sheet.json")).unwrap()).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lines(sheet: &Value) -> Vec<Value> {
    sheet["groups"].as_array().unwrap().iter().flat_map(|g| g["lines"].as_array().unwrap().clone()).collect()
}

fn line_for<'a>(ls: &'a [Value], id: &str) -> &'a Value {
    ls.iter().find(|l| l["events"].as_array().unwrap().iter().any(|e| e == id)).unwrap_or_else(|| panic!("no line holds {id}"))
}

/// handoff, with the `reader` question's `why` changed and its version raised.
fn good_handoff() -> String {
    HANDOFF.replace("Who picks the work up decides how much the brief must explain.", "Who picks the work up decides how much the brief must explain, so it is asked.").replacen("version: 1", "version: 2", 1)
}

/// handoff with a header too long for AskUserQuestion: parses, fails lint.
fn bad_handoff() -> String {
    HANDOFF.replace("header: Picked up by", "header: A header far too long")
}

#[test]
fn a_pack_becomes_a_sheet_with_proposed_files_and_checks() {
    let s = Scratch::new("ok");
    s.pack();
    s.reply(json!({
        "lines": [
            { "kind": "pattern", "events": ["e1", "e2"], "target": { "kind": "question", "pattern": "handoff", "question": "reader" }, "change": "reword why", "text": "ask `reader` every time" },
            { "kind": "memory", "events": ["e3"], "target": { "kind": "memory", "project": "-home-u-app", "file": "flash-flags.md" }, "change": "new entry", "text": "the board needs --erase",
              "proposed_text": "---\nname: flash-flags\ndescription: the board needs --erase before the first flash\nkind: rule\ntype: feedback\n---\nUse --erase.\n",
              "index_line": "- [Flash flags](flash-flags.md) — the board needs --erase first" }
        ],
        "patterns": { "handoff": good_handoff() }
    }));
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((out["status"].as_str(), out["lines"].as_u64(), out["dropped"].as_u64(), out["week"].as_str()), (Some("ok"), Some(2), Some(0), Some(WEEK)));

    let sheet = s.sheet();
    assert_eq!((sheet["promptu"].as_u64(), sheet["week"].as_str()), (Some(1), Some(WEEK)));
    assert!(sheet["superseded_by"].is_null() && sheet["supersedes"].is_null() && sheet["feel"].is_null());
    let ls = lines(&sheet);
    let p = line_for(&ls, "e1");
    assert_eq!((p["kind"].as_str(), p["verdict"].as_str(), p["status"].as_str(), p["count"].as_u64()), (Some("pattern"), Some("approve"), Some("open"), Some(2)));
    assert_eq!(p["events"], json!(["e1", "e2"]));
    assert_eq!(p["proposed"], "proposed/handoff.md");
    assert_eq!((p["check"]["lint"].as_str(), p["check"]["examples"].as_str()), (Some("ok"), Some("ok")));
    assert!(p["error"].is_null() && p["commit"].is_null() && p["carried_from"].is_null());
    assert!(sheet["groups"][0]["title"].as_str().unwrap().starts_with("handoff — 2: 2 question"), "{}", sheet["groups"][0]["title"]);
    let m = line_for(&ls, "e3");
    assert_eq!((m["kind"].as_str(), m["status"].as_str(), m["proposed"].as_str()), (Some("memory"), Some("open"), Some("proposed/memory/-home-u-app/flash-flags.md")));

    // The proposed file, the old and new examples, the markdown view and the run record.
    assert_eq!(std::fs::read_to_string(s.week().join("proposed/handoff.md")).unwrap(), good_handoff());
    assert!(std::fs::read_to_string(s.week().join("examples/handoff.old.md")).unwrap().contains("######## handoff example 1"));
    assert!(std::fs::read_to_string(s.week().join("examples/handoff.new.md")).unwrap().contains("######## handoff example 1"));
    assert!(s.week().join("proposed/memory/-home-u-app/flash-flags.md").is_file());
    let md = std::fs::read_to_string(s.week().join("sheet.md")).unwrap();
    assert!(md.contains("- [x] l1 ask `reader` every time (x2) [e1, e2]"), "{md}");
    let run: Value = serde_json::from_str(&std::fs::read_to_string(s.week().join("run.json")).unwrap()).unwrap();
    assert_eq!((run["status"].as_str(), run["kind"].as_str(), run["model"].as_str(), run["events"].as_u64(), run["lines"].as_u64()), (Some("ok"), Some("manual"), Some("opus"), Some(3), Some(2)));
    assert_eq!(run["cost_usd"].as_f64(), Some(1.25));

    // `week show` reads what draft wrote.
    let shown = Command::new(BIN)
        .env_clear()
        .env("HOME", s.root.join("home"))
        .env("PROMPTU_STATE_DIR", s.state())
        .args(["week", "show", "--week", WEEK])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&shown.stdout), md);
}

#[test]
fn every_event_ends_in_a_line_and_a_no_change_line_is_cut() {
    let s = Scratch::new("cut");
    s.pack();
    s.reply(json!({ "lines": [
        { "kind": "no-change", "events": ["e1"], "text": "a one-off task" },
        { "kind": "bogus", "events": ["e2"], "text": "dropped" },
        { "kind": "external", "events": ["e2"], "target": null, "text": "add a rule to the global CLAUDE.md" }
    ], "patterns": {} }));
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((out["lines"].as_u64(), out["dropped"].as_u64()), (Some(3), Some(1)));
    let ls = lines(&s.sheet());
    let nc = line_for(&ls, "e1");
    assert_eq!((nc["kind"].as_str(), nc["verdict"].as_str(), nc["status"].as_str(), nc["text"].as_str()), (Some("no-change"), Some("cut"), Some("cut"), Some("a one-off task")));
    assert_eq!(line_for(&ls, "e2")["kind"], "external");
    let left = line_for(&ls, "e3");
    assert_eq!((left["kind"].as_str(), left["text"].as_str(), left["status"].as_str()), (Some("no-change"), Some("not addressed"), Some("cut")));
    for e in ["e1", "e2", "e3"] {
        line_for(&ls, e);
    }
}

#[test]
fn a_failing_pattern_an_unknown_pattern_and_an_unknown_question_stay_as_failed_lines() {
    let s = Scratch::new("failed");
    s.pack();
    s.reply(json!({
        "lines": [
            { "kind": "pattern", "events": ["e1"], "target": { "kind": "question", "pattern": "handoff", "question": "reader" }, "change": "header", "text": "rename the header" },
            { "kind": "pattern", "events": ["e2"], "target": { "kind": "template", "pattern": "no-such" }, "change": "x", "text": "change a pattern that is not there" },
            { "kind": "pattern", "events": ["e3"], "target": { "kind": "question", "pattern": "handoff", "question": "no_such_q" }, "change": "x", "text": "change a question that is not there" }
        ],
        "patterns": { "handoff": bad_handoff() }
    }));
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let ls = lines(&s.sheet());
    assert_eq!(ls.len(), 3);
    let l1 = line_for(&ls, "e1");
    assert_eq!((l1["status"].as_str(), l1["check"]["lint"].as_str()), (Some("failed"), Some("failed")));
    assert!(l1["error"].as_str().unwrap().contains("header"), "{l1}");
    assert_eq!(l1["proposed"], "proposed/handoff.md");
    assert_eq!((line_for(&ls, "e2")["status"].as_str(), line_for(&ls, "e2")["error"].as_str()), (Some("failed"), Some("no pattern `no-such`")));
    assert_eq!(line_for(&ls, "e3")["status"], "failed");
    assert!(line_for(&ls, "e3")["error"].as_str().unwrap().contains("no_such_q"));
    let md = std::fs::read_to_string(s.week().join("sheet.md")).unwrap();
    assert!(md.contains("— failed: no pattern `no-such`"), "{md}");
}

#[test]
fn a_failed_or_slow_call_writes_no_sheet_and_one_line_on_stderr() {
    // A non-zero exit.
    let s = Scratch::new("exit");
    s.pack();
    std::fs::write(s.root.join("fake/exit"), "").unwrap();
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(err.trim().lines().count(), 1, "{err}");
    assert!(err.starts_with("promptu: the drafter failed:"), "{err}");
    assert!(!s.week().join("sheet.json").exists() && !s.week().join("sheet.md").exists());
    let run: Value = serde_json::from_str(&std::fs::read_to_string(s.week().join("run.json")).unwrap()).unwrap();
    assert_eq!(run["status"], "failed");
    assert!(run["error"].is_string() && run["finished"].is_string());

    // A reply that is not the sheet.
    let s = Scratch::new("noise");
    s.pack();
    std::fs::write(s.root.join("fake/reply.json"), json!({"type": "result", "is_error": false, "terminal_reason": "completed", "result": "no json here"}).to_string()).unwrap();
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!s.week().join("sheet.json").exists());

    // A call that reports an error (the timeout path is `pass::call_model`'s, covered in
    // tests/prompts.rs; `week_timeout_s` cannot go under 60 s).
    let s = Scratch::new("err");
    s.pack();
    std::fs::write(s.root.join("fake/reply.json"), json!({"type": "result", "is_error": true, "result": "rate limited"}).to_string()).unwrap();
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("rate limited"));
}

#[test]
fn a_failed_draft_leaves_an_earlier_sheet_as_it_was() {
    let s = Scratch::new("keep");
    s.pack();
    s.reply(json!({ "lines": [ { "kind": "no-change", "events": ["e1", "e2", "e3"], "text": "nothing" } ], "patterns": {} }));
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let before = std::fs::read_to_string(s.week().join("sheet.json")).unwrap();
    std::fs::write(s.root.join("fake/exit"), "").unwrap();
    assert_eq!(s.draft(&[], &[]).status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(s.week().join("sheet.json")).unwrap(), before);
}

#[test]
fn the_call_has_no_tools_no_mcp_servers_and_the_configured_model() {
    let s = Scratch::new("argv");
    s.pack();
    std::fs::write(s.root.join("cfg/config.toml"), "week_model = \"sonnet\"\nweek_effort = \"high\"\n").unwrap();
    s.reply(json!({ "lines": [], "patterns": {} }));
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let argv: Vec<String> = s.fake("argv.txt").lines().map(str::to_string).collect();
    let after = |flag: &str| argv.iter().position(|a| a == flag).map(|i| argv[i + 1].clone());
    assert_eq!(after("--tools").as_deref(), Some(""), "{argv:?}");
    assert!(argv.contains(&"--strict-mcp-config".to_string()) && !argv.iter().any(|a| a == "--mcp-config"), "{argv:?}");
    assert_eq!(after("--setting-sources").as_deref(), Some(""));
    assert_eq!((after("--model").as_deref(), after("--effort").as_deref()), (Some("sonnet"), Some("high")));
    assert!(argv.contains(&"--no-session-persistence".to_string()));
    assert!(s.fake("env.txt").contains("PROMPTU_INTERNAL=1"));
    let stdin = s.fake("stdin.txt");
    assert!(stdin.contains("<pack>") && stdin.contains("\"e1\"") && stdin.contains("<pattern name=\"handoff\">"), "{}", &stdin[..200.min(stdin.len())]);
    // A reply with no line for any event still yields a sheet holding all of them.
    assert_eq!(lines(&s.sheet()).len(), 1);
}

#[test]
fn an_empty_pack_makes_an_empty_sheet_without_a_call() {
    let s = Scratch::new("empty");
    let pack = json!({ "promptu": 1, "week": WEEK, "window": { "from": "2026-11-18", "to": "2026-11-24" }, "built": "x", "events": [] });
    std::fs::write(s.week().join("pack.json"), pack.to_string()).unwrap();
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(s.sheet()["groups"], json!([]));
    assert_eq!(s.fake("argv.txt"), "", "no model call");
}

#[test]
fn a_second_draft_keeps_marks_and_a_new_week_supersedes_the_open_sheet() {
    let s = Scratch::new("next");
    s.pack();
    let reply = json!({ "lines": [ { "kind": "external", "events": ["e1", "e2", "e3"], "target": null, "change": "rule", "text": "add a global rule" } ], "patterns": {} });
    s.reply(reply.clone());
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    // The owner cut the line (what `week mark` will do), then the week is drafted again.
    let mut sheet = s.sheet();
    sheet["groups"][0]["lines"][0]["verdict"] = json!("cut");
    sheet["groups"][0]["lines"][0]["note"] = json!("not now");
    sheet["groups"][0]["lines"][0]["status"] = json!("cut");
    std::fs::write(s.week().join("sheet.json"), sheet.to_string()).unwrap();
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let l = lines(&s.sheet())[0].clone();
    assert_eq!((l["verdict"].as_str(), l["note"].as_str(), l["status"].as_str()), (Some("cut"), Some("not now"), Some("cut")));

    // The next week's draft supersedes this one and reports the baseline.
    let next = s.state().join("week/2026-W49");
    std::fs::create_dir_all(&next).unwrap();
    let pack = json!({ "promptu": 1, "week": "2026-W49", "window": { "from": "x", "to": "y" }, "built": "x", "events": [ { "id": "e1", "kind": "run", "pattern": "handoff", "asked": 3, "filled": 2, "changed_fills": [], "picked_by_owner": false } ] });
    std::fs::write(next.join("pack.json"), pack.to_string()).unwrap();
    s.reply(json!({ "lines": [ { "kind": "no-change", "events": ["e1"], "text": "fine" } ], "patterns": {} }));
    let o = Command::new(BIN)
        .env_clear()
        .env("HOME", s.root.join("home"))
        .env("PATH", "/usr/bin:/bin")
        .env("PROMPTU_CONFIG_DIR", s.root.join("cfg"))
        .env("PROMPTU_STATE_DIR", s.state())
        .env("PROMPTU_CLAUDE", s.root.join("fake/claude"))
        .env("PROMPTU_FAKE_DIR", s.root.join("fake"))
        .env("TZ", "UTC")
        .args(["week", "draft", "--week", "2026-W49", "--json"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["supersedes"], WEEK);
    assert_eq!(s.sheet()["superseded_by"], "2026-W49");
    let new: Value = serde_json::from_str(&std::fs::read_to_string(next.join("sheet.json")).unwrap()).unwrap();
    assert_eq!(new["supersedes"], WEEK);
    let q = new["numbers"].as_array().unwrap().iter().find(|n| n["check"] == "questions_per_run").unwrap();
    assert_eq!(q["value"].as_f64(), Some(3.0));
}

#[test]
fn a_bad_week_or_a_missing_pack_is_a_usage_error() {
    let s = Scratch::new("usage");
    let o = Command::new(BIN).env_clear().env("HOME", s.root.join("home")).env("PROMPTU_STATE_DIR", s.state()).args(["week", "draft", "--week", "nope"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    let o = s.draft(&["--pack", "/nonexistent/pack.json"], &[]);
    assert_eq!(o.status.code(), Some(2));
}

// ---- the replay of a proposed pattern (decision 47, forks W23, W24) ----

/// A log entry of a run of `handoff`, as `render` writes it.
fn write_log(s: &Scratch, run: &str, pattern: &str, mode: &str, original: &str) {
    let dir = s.state().join("log");
    std::fs::create_dir_all(&dir).unwrap();
    let e = json!({ "promptu": 1, "run": run, "date": "2026-11-18T09:00:00+00:00", "cwd": "/x", "project_key": null, "pattern": pattern, "version": 1, "mode": mode,
        "original": original, "rendered": "r", "rewritten": false, "rewrite_skipped": null, "rewrite_model": null, "rewrite_effort": null, "rewrite_ms": null, "wait_ms": 0 });
    std::fs::write(dir.join(format!("{run}.json")), e.to_string()).unwrap();
}

fn logs(s: &Scratch) {
    write_log(s, "20251118-091500-aaaa", "handoff", "prompt", "hand this to a colleague, secret-one");
    write_log(s, "20251118-101500-bbbb", "handoff", "prompt", "hand this off, secret-two");
    write_log(s, "20251118-111500-cccc", "handoff", "prompt", "write a handoff, secret-three");
    write_log(s, "20251118-121500-dddd", "implement-issue", "prompt", "other pattern, secret-four");
    write_log(s, "20251118-131500-eeee", "handoff", "brief", "a pasted brief, secret-five");
}

fn question_line() -> Value {
    json!({ "kind": "pattern", "events": ["e1", "e2"], "target": { "kind": "question", "pattern": "handoff", "question": "reader" }, "change": "reword why", "text": "ask `reader` every time" })
}

fn replay_reply(s: &Scratch, line: Value, file: String) {
    s.reply(json!({ "lines": [line], "patterns": { "handoff": file }, "pattern": "handoff", "confidence": 0.9 }));
}

fn first_line(s: &Scratch) -> Value {
    lines(&s.sheet()).into_iter().find(|l| l["kind"] == "pattern").unwrap()
}

#[test]
fn a_question_change_is_replayed_on_past_prompts_old_against_new() {
    let s = Scratch::new("replay");
    s.pack();
    logs(&s);
    std::fs::write(s.root.join("cfg/config.toml"), "week_replay_cap_usd = 100\n").unwrap();
    replay_reply(&s, question_line(), good_handoff());
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let l = first_line(&s);
    let replay = l["check"]["replay"].as_array().unwrap();
    // The three rough prompts of the pattern, newest first; not the brief, not the other pattern.
    let runs: Vec<&str> = replay.iter().map(|r| r["run"].as_str().unwrap()).collect();
    assert_eq!(runs, ["20251118-111500-cccc", "20251118-101500-bbbb", "20251118-091500-aaaa"]);
    assert!(l["check"]["replay_note"].is_null());
    for r in replay {
        let f = r["file"].as_str().unwrap();
        assert_eq!(f, format!("replay/handoff/{}.md", r["run"].as_str().unwrap()));
        let text = std::fs::read_to_string(s.week().join(f)).unwrap();
        assert!(text.contains("| picked | handoff | handoff |") && text.contains("| questions asked |") && text.contains("> "), "{text}");
        assert!(r["questions_before"].as_u64().is_some() && r["questions_after"].as_u64().is_some());
    }
    assert!(std::fs::read_to_string(s.week().join("replay/handoff/20251118-091500-aaaa.md")).unwrap().contains("secret-one"));
    let md = std::fs::read_to_string(s.week().join("sheet.md")).unwrap();
    assert!(md.contains("  - replay: replay/handoff/20251118-111500-cccc.md, "), "{md}");
    // The replay's file is a pointer, not a note: `week apply` must not read it as one.
    // Nothing is written outside the state folder and the fake's own folder.
    for d in ["home", "claude", "cfg"] {
        let n = std::fs::read_dir(s.root.join(d)).unwrap().flatten().filter(|e| d != "cfg" || e.file_name() != "config.toml").count();
        assert!(n == 0 || d == "claude", "{d} was written to");
    }
}

#[test]
fn a_template_only_change_is_not_replayed() {
    let s = Scratch::new("replay-template");
    s.pack();
    logs(&s);
    std::fs::write(s.root.join("cfg/config.toml"), "week_replay_cap_usd = 100\n").unwrap();
    let body_only = format!("{HANDOFF}\nOne more line of the template.\n");
    let line = json!({ "kind": "pattern", "events": ["e1"], "target": { "kind": "template", "pattern": "handoff", "question": "" }, "change": "add a line", "text": "add a line" });
    replay_reply(&s, line, body_only);
    let o = s.draft(&[], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let l = first_line(&s);
    assert_eq!(l["check"]["replay"], json!([]));
    assert!(l["check"]["replay_note"].as_str().unwrap().contains("nothing to replay"), "{l}");
    assert!(!s.week().join("replay").exists());
    // One model call only: the drafter's.
    assert!(!s.fake("stdin.txt").contains("<regex_hint>"));
}

#[test]
fn the_cap_stops_the_replay_and_the_line_says_so() {
    let s = Scratch::new("replay-cap");
    s.pack();
    logs(&s);
    // The fake reports $1.25 a call: one prompt (two calls) is $2.50, over a $1 cap.
    std::fs::write(s.root.join("cfg/config.toml"), "week_replay_cap_usd = 1\n").unwrap();
    replay_reply(&s, question_line(), good_handoff());
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let l = first_line(&s);
    assert_eq!(l["check"]["replay"].as_array().unwrap().len(), 1);
    let note = l["check"]["replay_note"].as_str().unwrap();
    assert!(note.contains("stopped by the replay cap of $1.00") && note.contains("1 of 3"), "{note}");
    let md = std::fs::read_to_string(s.week().join("sheet.md")).unwrap();
    assert!(md.contains("  - replay: stopped by the replay cap"), "{md}");
    // A cap of 0 turns the replay off.
    std::fs::write(s.root.join("cfg/config.toml"), "week_replay_cap_usd = 0\n").unwrap();
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let l = first_line(&s);
    assert_eq!(l["check"]["replay"], json!([]));
    assert!(l["check"]["replay_note"].as_str().unwrap().contains("0 of 3"));
}

#[test]
fn a_pattern_with_no_past_prompt_says_so() {
    let s = Scratch::new("replay-none");
    s.pack();
    write_log(&s, "20251118-121500-dddd", "implement-issue", "prompt", "other pattern");
    replay_reply(&s, question_line(), good_handoff());
    assert_eq!(s.draft(&[], &[]).status.code(), Some(0));
    let l = first_line(&s);
    assert!(l["check"]["replay_note"].as_str().unwrap().contains("no past prompt of `handoff`"), "{l}");
}
