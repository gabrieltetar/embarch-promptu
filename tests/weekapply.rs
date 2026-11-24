//! `promptu week mark` and `promptu week apply` end to end (docs/contract.md
//! §14.4), against a scratch state folder, a scratch memory folder, a hand-built
//! sheet and a scratch git checkout the test makes. Nothing reads the real
//! `~/.claude`, `~/.local/state` or a real pattern checkout, and nothing is pushed.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");
const TODAY: &str = "2026-11-25";
const WEEK: &str = "2026-W48";
const PROJECT: &str = "-home-u-app";
const HANDOFF: &str = include_str!("../patterns/handoff.md");
const GOOD_TOPIC: &str = "---\nname: flash-flags\ndescription: the board needs --erase before the first flash\nkind: rule\ntype: feedback\n---\nUse --erase.\n";
const GOOD_INDEX: &str = "- [Flash flags](flash-flags.md) — the board needs --erase first";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!("promptu-apply-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        for d in ["home", "cfg", "state/week/2026-W48", format!("claude/projects/{PROJECT}/memory").as_str()] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        Scratch { root }
    }

    fn state_week(&self) -> PathBuf {
        self.root.join("state/week").join(WEEK)
    }

    fn memory(&self) -> PathBuf {
        self.root.join("claude/projects").join(PROJECT).join("memory")
    }

    fn config(&self, text: &str) {
        std::fs::write(self.root.join("cfg/config.toml"), text).unwrap();
    }

    fn write_sheet(&self, lines: Vec<Value>) {
        let lines: Vec<Value> = lines.into_iter().enumerate().map(|(i, mut l)| {
            l["id"] = json!(format!("l{}", i + 1));
            l
        }).collect();
        let sheet = json!({ "promptu": 1, "week": WEEK, "generated": "2026-11-25T08:00:00+00:00", "feel": null, "superseded_by": null, "supersedes": null, "numbers": [],
            "groups": [ { "id": "g1", "kind": "mixed", "pattern": null, "title": "all", "cut": false, "lines": lines } ] });
        std::fs::write(self.state_week().join("sheet.json"), serde_json::to_string_pretty(&sheet).unwrap()).unwrap();
        // The view the owner reads, as `week draft` writes it.
        let md = format!("# sheet\n\n## g1 all\n\n{}", lines_md(&sheet));
        std::fs::write(self.state_week().join("sheet.md"), md).unwrap();
    }

    fn sheet(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.state_week().join("sheet.json")).unwrap()).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.root.join("state"))
            .env("PROMPTU_TODAY", TODAY)
            .env("TZ", "UTC")
            .args(args)
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let o = self.run(args);
        let v = serde_json::from_slice(&o.stdout).unwrap_or_else(|_| panic!("not JSON: {} / {}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)));
        (o.status.code().unwrap(), v)
    }

    /// A git checkout of the patterns' repo with `handoff.md` committed on `main`.
    fn checkout(&self) -> PathBuf {
        let c = self.root.join("checkout");
        std::fs::create_dir_all(c.join("patterns")).unwrap();
        std::fs::create_dir_all(c.join("tests/golden")).unwrap();
        std::fs::write(c.join("patterns/handoff.md"), HANDOFF).unwrap();
        std::fs::write(c.join("tests/golden/handoff-1.md"), "stale\n").unwrap();
        git(&c, &["init", "-q", "-b", "main"]);
        git(&c, &["config", "user.name", "Test"]);
        git(&c, &["config", "user.email", "test@example.invalid"]);
        git(&c, &["add", "-A"]);
        git(&c, &["commit", "-q", "-m", "start"]);
        c
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lines_md(sheet: &Value) -> String {
    let mut out = String::new();
    for l in sheet["groups"][0]["lines"].as_array().unwrap() {
        out.push_str(&format!("- [{}] {} {} [e1]\n", if l["verdict"] == "approve" { "x" } else { " " }, l["id"].as_str().unwrap(), l["text"].as_str().unwrap()));
    }
    out
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn base(kind: &str, target: Value, text: &str) -> Value {
    json!({ "kind": kind, "verdict": "approve", "note": "", "target": target, "change": "c", "text": text, "count": 1, "events": ["e1"], "carried_from": null,
            "proposed": null, "check": null, "status": "open", "error": null, "commit": null })
}

fn memory_line(file: &str, topic: &str, index: &str) -> Value {
    let mut l = base("memory", json!({ "kind": "memory", "project": PROJECT, "file": file }), "a fact");
    l["proposed_text"] = json!(topic);
    l["index_line"] = json!(index);
    l
}

fn pattern_line(s: &Scratch, proposed: &str) -> Value {
    std::fs::create_dir_all(s.state_week().join("proposed")).unwrap();
    std::fs::write(s.state_week().join("proposed/handoff.md"), proposed).unwrap();
    let mut l = base("pattern", json!({ "kind": "question", "pattern": "handoff", "question": "reader" }), "reword why");
    l["proposed"] = json!("proposed/handoff.md");
    l
}

/// handoff with the `reader` question's `why` reworded (the version stays as shipped: apply bumps it).
fn reworded() -> String {
    HANDOFF.replace("Who picks the work up decides how much the brief must explain.", "Who picks the work up decides how much the brief must explain, so it is asked.")
}

fn handoff_version() -> u32 {
    HANDOFF.lines().find_map(|l| l.strip_prefix("version: ")).unwrap().trim().parse().unwrap()
}

#[test]
fn an_approved_memory_line_writes_a_file_memory_check_passes_and_a_failing_one_writes_nothing() {
    let s = Scratch::new("memory");
    s.write_sheet(vec![memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX), memory_line("bad.md", "no frontmatter at all\n", "- [Bad](bad.md) — nothing")]);
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!((out["applied"].as_u64(), out["failed"].as_u64()), (Some(1), Some(1)));
    assert_eq!(std::fs::read_to_string(s.memory().join("flash-flags.md")).unwrap(), GOOD_TOPIC);
    assert!(std::fs::read_to_string(s.memory().join("MEMORY.md")).unwrap().contains("flash-flags.md"));
    assert!(!s.memory().join("bad.md").exists());
    let o = s.run(&["memory", "check", s.memory().to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stdout));
    let sheet = s.sheet();
    let ls = sheet["groups"][0]["lines"].as_array().unwrap();
    assert_eq!((ls[0]["status"].as_str(), ls[1]["status"].as_str()), (Some("applied"), Some("failed")));
    assert!(ls[1]["error"].as_str().unwrap().contains("T1"), "{}", ls[1]["error"]);
}

#[test]
fn a_memory_line_backs_up_the_index_it_changes() {
    let s = Scratch::new("backup");
    std::fs::write(s.memory().join("MEMORY.md"), "## tools\n\n- [Old](old.md) — an older fact\n").unwrap();
    s.write_sheet(vec![memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX)]);
    let (code, _) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 0);
    let backup = std::fs::read_to_string(s.state_week().join("backup").join(PROJECT).join("MEMORY.md")).unwrap();
    assert!(backup.contains("old.md") && !backup.contains("flash-flags"));
    let index = std::fs::read_to_string(s.memory().join("MEMORY.md")).unwrap();
    assert!(index.contains("old.md") && index.contains("flash-flags.md"));
}

#[test]
fn an_approved_pattern_line_is_a_commit_on_a_branch_with_the_version_bumped_and_nothing_pushed() {
    let s = Scratch::new("commit");
    let c = s.checkout();
    s.config(&format!("week_checkout = \"{}\"\n", c.display()));
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l]);
    let main_before = git(&c, &["rev-parse", "main"]);
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["branch"], "promptu/week-2026-W48");
    let commit = out["lines"][0]["commit"].as_str().unwrap().to_string();
    assert_eq!(git(&c, &["rev-parse", "promptu/week-2026-W48"]), commit);
    assert_eq!(git(&c, &["rev-parse", "main"]), main_before, "main is never touched");
    assert_eq!(git(&c, &["symbolic-ref", "--short", "HEAD"]), "main", "the owner's branch is checked out again");
    assert_eq!(git(&c, &["remote"]), "", "nothing is pushed: there is no remote to push to");
    let file = git(&c, &["show", &format!("{commit}:patterns/handoff.md")]);
    assert!(file.contains(&format!("version: {}", handoff_version() + 1)) && file.contains("so it is asked"));
    let golden = git(&c, &["show", &format!("{commit}:tests/golden/handoff-1.md")]);
    assert_ne!(golden, "stale", "the example render is updated in the same commit");
    let sheet = s.sheet();
    assert_eq!((sheet["groups"][0]["lines"][0]["status"].as_str(), sheet["groups"][0]["lines"][0]["commit"].as_str()), (Some("applied"), Some(commit.as_str())));
}

#[test]
fn a_dirty_checkout_stops_apply_before_it_writes() {
    let s = Scratch::new("dirty");
    let c = s.checkout();
    std::fs::write(c.join("scratch.txt"), "owner's work\n").unwrap();
    s.config(&format!("week_checkout = \"{}\"\n", c.display()));
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l, memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX)]);
    let o = s.run(&["week", "apply", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(c.join("patterns/handoff.md")).unwrap(), HANDOFF);
    assert_eq!(s.sheet()["groups"][0]["lines"][0]["status"], "open");
}

#[test]
fn a_pattern_line_that_fails_the_public_check_fails_and_leaves_the_checkout_as_it_was() {
    let s = Scratch::new("public");
    let c = s.checkout();
    let script = s.root.join("fail.sh");
    std::fs::write(&script, "#!/bin/sh\necho found a name in $1\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    s.config(&format!("week_checkout = \"{}\"\nweek_public_check = \"{} {{checkout}}\"\n", c.display(), script.display()));
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l]);
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out["failed"], 1);
    assert!(out["lines"][0]["error"].as_str().unwrap().contains("found a name"), "{out}");
    assert_eq!(std::fs::read_to_string(c.join("patterns/handoff.md")).unwrap(), HANDOFF);
    assert_eq!(git(&c, &["status", "--porcelain"]), "");
    assert_eq!(s.sheet()["groups"][0]["lines"][0]["status"], "failed");
}

#[test]
fn with_no_checkout_an_approved_pattern_line_is_written_as_a_patch() {
    let s = Scratch::new("patch");
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l]);
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 0, "{out}");
    let path = out["patch"].as_str().unwrap();
    let patch = std::fs::read_to_string(path).unwrap();
    assert!(patch.contains("patterns/handoff.md") && patch.contains(&format!("+version: {}", handoff_version() + 1)) && patch.contains("so it is asked"), "{patch}");
    assert!(out["lines"][0]["commit"].is_null());
    assert_eq!(s.sheet()["groups"][0]["lines"][0]["status"], "applied");
}

#[test]
fn a_cross_written_in_sheet_md_is_honoured_and_a_note_is_read_back() {
    let s = Scratch::new("md");
    s.write_sheet(vec![memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX), memory_line("second.md", GOOD_TOPIC.replace("flash-flags", "second").as_str(), "- [Second](second.md) — another")]);
    let md = std::fs::read_to_string(s.state_week().join("sheet.md")).unwrap();
    let edited = md.replacen("- [x] l1 ", "- [ ] l1 ", 1).replacen("- [x] l2 a fact [e1]\n", "- [x] l2 a fact [e1]\n  keep it short\n", 1);
    std::fs::write(s.state_week().join("sheet.md"), edited).unwrap();
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out["applied"], 1);
    assert!(!s.memory().join("flash-flags.md").exists(), "the cut line wrote nothing");
    assert!(s.memory().join("second.md").exists());
    let sheet = s.sheet();
    let ls = sheet["groups"][0]["lines"].as_array().unwrap();
    assert_eq!((ls[0]["verdict"].as_str(), ls[0]["status"].as_str()), (Some("cut"), Some("cut")));
    assert_eq!(ls[1]["note"], "keep it short");
}

#[test]
fn applying_the_same_sheet_twice_changes_nothing_the_second_time() {
    let s = Scratch::new("twice");
    let c = s.checkout();
    s.config(&format!("week_checkout = \"{}\"\n", c.display()));
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l, memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX)]);
    let (code, _) = s.json(&["week", "apply", "--json"]);
    assert_eq!(code, 0);
    let snapshot = (git(&c, &["rev-parse", "promptu/week-2026-W48"]), std::fs::read_to_string(s.memory().join("MEMORY.md")).unwrap(), std::fs::read_to_string(s.state_week().join("sheet.json")).unwrap(), std::fs::read_to_string(s.memory().join("flash-flags.md")).unwrap());
    let (code, out) = s.json(&["week", "apply", "--json"]);
    assert_eq!((code, out["applied"].as_u64(), out["failed"].as_u64()), (0, Some(0), Some(0)));
    assert_eq!(git(&c, &["rev-parse", "promptu/week-2026-W48"]), snapshot.0);
    assert_eq!(std::fs::read_to_string(s.memory().join("MEMORY.md")).unwrap(), snapshot.1);
    assert_eq!(std::fs::read_to_string(s.state_week().join("sheet.json")).unwrap(), snapshot.2);
    assert_eq!(std::fs::read_to_string(s.memory().join("flash-flags.md")).unwrap(), snapshot.3);
    assert_eq!(git(&c, &["rev-list", "--count", "main..promptu/week-2026-W48"]), "1");
}

#[test]
fn dry_run_writes_nothing() {
    let s = Scratch::new("dry");
    let c = s.checkout();
    s.config(&format!("week_checkout = \"{}\"\n", c.display()));
    let l = pattern_line(&s, &reworded());
    s.write_sheet(vec![l, memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX)]);
    let before = std::fs::read_to_string(s.state_week().join("sheet.json")).unwrap();
    let (code, out) = s.json(&["week", "apply", "--dry-run", "--json"]);
    assert_eq!((code, out["applied"].as_u64()), (0, Some(2)));
    assert_eq!(git(&c, &["branch", "--list", "promptu/*"]), "");
    assert!(!s.memory().join("flash-flags.md").exists());
    assert_eq!(std::fs::read_to_string(s.state_week().join("sheet.json")).unwrap(), before);
}

#[test]
fn mark_cuts_a_line_a_group_and_sets_the_feel() {
    let s = Scratch::new("mark");
    s.write_sheet(vec![memory_line("a.md", GOOD_TOPIC, GOOD_INDEX), memory_line("b.md", GOOD_TOPIC, GOOD_INDEX)]);
    let (code, out) = s.json(&["week", "mark", "l1", "--note", "not now", "--json"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!((out["changed"][0]["id"].as_str(), out["changed"][0]["status"].as_str(), out["open"].as_u64()), (Some("l1"), Some("cut"), Some(1)));
    let (_, out) = s.json(&["week", "mark", "l1", "--verdict", "approve", "--json"]);
    assert_eq!(out["changed"][0]["status"], "open");
    let (_, out) = s.json(&["week", "mark", "g1", "--json"]);
    assert_eq!((out["changed"].as_array().unwrap().len(), out["open"].as_u64()), (2, Some(0)));
    assert_eq!(s.sheet()["groups"][0]["cut"], true);
    let (_, out) = s.json(&["week", "mark", "--feel", "better", "--json"]);
    assert_eq!(out["feel"], "better");
    assert_eq!(s.sheet()["feel"], "better");
    let md = std::fs::read_to_string(s.state_week().join("sheet.md")).unwrap();
    assert!(md.contains("- [ ] l1"));
}

#[test]
fn mark_refuses_an_unknown_id_a_bad_verdict_and_an_applied_line() {
    let s = Scratch::new("refuse");
    s.write_sheet(vec![memory_line("flash-flags.md", GOOD_TOPIC, GOOD_INDEX)]);
    assert_eq!(s.run(&["week", "mark", "l9"]).status.code(), Some(2));
    assert_eq!(s.run(&["week", "mark", "l1", "--verdict", "applied"]).status.code(), Some(2));
    assert_eq!(s.run(&["week", "mark"]).status.code(), Some(2));
    assert_eq!(s.json(&["week", "apply", "--json"]).0, 0);
    assert_eq!(s.run(&["week", "mark", "l1"]).status.code(), Some(2), "the sheet is settled");
    assert_eq!(s.sheet()["groups"][0]["lines"][0]["status"], "applied");
}

#[test]
fn apply_with_no_sheet_is_a_usage_error() {
    let s = Scratch::new("none");
    std::fs::remove_dir_all(s.state_week()).unwrap();
    assert_eq!(s.run(&["week", "apply"]).status.code(), Some(2));
}
