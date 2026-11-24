//! `promptu week status`, `week show` and `desk` (docs/contract.md §9, §14.4)
//! against scratch state folders built by hand in the contract's shapes. No
//! model or network call: `claude` is not on the cleared `PATH`.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "promptu-weekread-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for d in ["cfg", "home"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        Scratch { root }
    }

    fn state(&self) -> PathBuf {
        self.root.join("state")
    }

    fn put(&self, rel: &str, v: &Value) {
        let p = self.state().join("week").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v.to_string()).unwrap();
    }

    fn sheet(&self, week: &str, superseded_by: Value, statuses: &[(&str, &str)]) {
        let lines: Vec<Value> = statuses.iter().map(|(g, st)| json!({"id": format!("l{g}"), "group": format!("g{g}"), "status": st, "text": "add option", "verdict": "approve"})).collect();
        let group = |id: &str, title: &str| {
            json!({"id": id, "title": title, "lines": lines.iter().filter(|l| l["group"] == id).cloned().collect::<Vec<_>>()})
        };
        self.put(
            &format!("{week}/sheet.json"),
            &json!({"promptu": 1, "week": week, "generated": "2026-11-25T08:03:00+00:00", "feel": null, "superseded_by": superseded_by,
                    "groups": [group("g1", "implement-issue"), group("g2", "memory")]}),
        );
    }

    fn run(&self, now: &str, args: &[&str]) -> Output {
        Command::new(BIN)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.state())
            .env("PROMPTU_NOW", now)
            .env("TZ", "UTC")
            .args(args)
            .output()
            .unwrap()
    }

    fn json(&self, now: &str, args: &[&str]) -> Value {
        let o = self.run(now, args);
        assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A Wednesday at noon: the schedule's 08:00 is four hours back.
const NOON: &str = "2026-11-25T12:00:00+00:00";
const TIMER: &str = "2026-11-01T09:00:00+00:00";

fn timer(s: &Scratch) {
    s.put("timer.json", &json!({"promptu": 1, "installed": TIMER, "schedule": "wed 08:00"}));
}

fn run_record(s: &Scratch, week: &str, started: &str, finished: Value, status: &str) {
    s.put(
        &format!("{week}/run.json"),
        &json!({"promptu": 1, "week": week, "kind": "scheduled", "scheduled_for": started, "started": started, "finished": finished, "status": status, "error": null}),
    );
}

#[test]
fn no_state_folder_desk_is_zero_status_is_quiet_and_show_is_a_usage_error() {
    let s = Scratch::new("none");
    assert!(!s.state().exists());
    assert_eq!(s.json(NOON, &["desk", "--json"]), json!({"promptu": 1, "open": 0}));
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!(st["promptu"], 1);
    assert_eq!(st["schedule"], "wed 08:00");
    assert_eq!(st["timer"], json!({"installed": false, "installed_at": null}));
    assert_eq!((&st["last"], &st["sheet"], &st["open"], &st["missed"], &st["running"]), (&Value::Null, &Value::Null, &json!(0), &json!(false), &json!(false)));
    assert_eq!(st["next"], "2026-12-02T08:00:00+00:00");
    assert_eq!(s.run(NOON, &["week", "show", "--json"]).status.code(), Some(2));
    assert_eq!(s.run(NOON, &["week", "show", "--week", "2026-W48", "--json"]).status.code(), Some(2));
    assert_eq!(s.run(NOON, &["week", "show", "--week", "nope"]).status.code(), Some(2));
    assert!(s.run(NOON, &["desk"]).stdout.is_empty());
    assert!(!s.state().exists(), "a read wrote something");
}

#[test]
fn a_waiting_sheet_counts_its_open_lines_per_group() {
    let s = Scratch::new("waiting");
    timer(&s);
    run_record(&s, "2026-W48", "2026-11-25T08:00:05+00:00", json!("2026-11-25T08:03:00+00:00"), "ok");
    s.sheet("2026-W47", json!("2026-W48"), &[("1", "open")]);
    s.sheet("2026-W48", Value::Null, &[("1", "open"), ("1", "open"), ("1", "cut"), ("2", "applied"), ("2", "failed")]);
    let d = s.json(NOON, &["desk", "--json"]);
    assert_eq!(
        d,
        json!({"promptu": 1, "open": 2, "week": "2026-W48", "generated": "2026-11-25T08:03:00+00:00", "groups": [{"id": "g1", "title": "implement-issue", "open": 2}], "feel": null})
    );
    assert_eq!(String::from_utf8_lossy(&s.run(NOON, &["desk"]).stdout), "promptu: 2 open (2026-W48)\n");
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!(st["open"], 2);
    assert_eq!(st["sheet"], json!({"week": "2026-W48", "generated": "2026-11-25T08:03:00+00:00"}));
    assert_eq!(st["last"]["status"], "ok");
    assert_eq!(st["timer"]["installed"], true);
    assert_eq!((&st["missed"], &st["running"]), (&json!(false), &json!(false)));
    // show: the open sheet is the newest unsuperseded one; a named week may be a superseded one.
    assert_eq!(s.json(NOON, &["week", "show", "--json"])["week"], "2026-W48");
    assert_eq!(s.json(NOON, &["week", "show", "--week", "2026-W47", "--json"])["superseded_by"], "2026-W48");
    let text = String::from_utf8_lossy(&s.run(NOON, &["week", "show"]).stdout).into_owned();
    assert!(text.contains("- [x] l1 add option"), "{text}");
    s.put("2026-W48/sheet.md", &json!("unused"));
    let t = String::from_utf8_lossy(&s.run(NOON, &["week", "status"]).stdout).into_owned();
    assert!(t.contains("waiting: 2 open in 2026-W48") && t.contains("next run: 2026-12-02T08:00:00+00:00"), "{t}");
}

#[test]
fn an_applied_sheet_is_zero_open_and_the_desk_stays_hidden() {
    let s = Scratch::new("applied");
    timer(&s);
    s.sheet("2026-W48", Value::Null, &[("1", "applied"), ("2", "cut")]);
    assert_eq!(s.json(NOON, &["desk", "--json"]), json!({"promptu": 1, "open": 0}));
    assert!(s.run(NOON, &["desk"]).stdout.is_empty());
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!((&st["open"], &st["sheet"]["week"]), (&json!(0), &json!("2026-W48")));
    assert_eq!(s.json(NOON, &["week", "show", "--json"])["groups"][0]["lines"][0]["status"], "applied");
}

#[test]
fn a_run_missed_by_two_hours_is_missed_and_a_run_that_started_or_failed_is_not() {
    let s = Scratch::new("missed");
    timer(&s);
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!((&st["missed"], &st["missed_since"]), (&json!(true), &json!("2026-11-25T08:00:00+00:00")));
    // Inside the grace: not missed yet.
    assert_eq!(s.json("2026-11-25T09:30:00+00:00", &["week", "status", "--json"])["missed"], false);
    // Before the timer was installed: not missed.
    let s2 = Scratch::new("missed-early");
    s2.put("timer.json", &json!({"promptu": 1, "installed": "2026-11-25T10:00:00+00:00", "schedule": "wed 08:00"}));
    assert_eq!(s2.json("2026-11-25T13:00:00+00:00", &["week", "status", "--json"])["missed"], false);
    // No timer: next is computed, missed is false.
    let s3 = Scratch::new("missed-none");
    assert_eq!(s3.json(NOON, &["week", "status", "--json"])["missed"], false);
    // A failed run started after S: reported failed, not missed.
    run_record(&s, "2026-W48", "2026-11-25T08:00:30+00:00", json!("2026-11-25T08:15:00+00:00"), "failed");
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!((&st["missed"], &st["last"]["status"]), (&json!(false), &json!("failed")));
    // The previous week's run does not cover this week's instant.
    let s4 = Scratch::new("missed-old");
    timer(&s4);
    run_record(&s4, "2026-W47", "2026-11-18T08:00:30+00:00", json!("2026-11-18T08:03:00+00:00"), "ok");
    assert_eq!(s4.json(NOON, &["week", "status", "--json"])["missed"], true);
    // PROMPTU_WEEK_SCHEDULE moves S, and a bad one falls back with a warning.
    let o = Command::new(BIN)
        .env_clear()
        .env("PROMPTU_CONFIG_DIR", s.root.join("cfg"))
        .env("PROMPTU_STATE_DIR", s.state())
        .env("PROMPTU_NOW", NOON)
        .env("PROMPTU_WEEK_SCHEDULE", "someday 8am")
        .env("TZ", "UTC")
        .args(["week", "status", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!((&v["schedule"], v["warnings"].as_array().unwrap().len()), (&json!("wed 08:00"), 1));
}

#[test]
fn a_run_in_progress_is_running_until_its_record_goes_stale() {
    let s = Scratch::new("running");
    timer(&s);
    run_record(&s, "2026-W48", "2026-11-25T11:50:00+00:00", Value::Null, "running");
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!((&st["running"], &st["last"]["status"], &st["missed"]), (&json!(true), &json!("running"), &json!(false)));
    assert!(String::from_utf8_lossy(&s.run(NOON, &["week", "status"]).stdout).contains("running now"));
    assert_eq!(s.json("2026-11-25T14:00:00+00:00", &["week", "status", "--json"])["running"], false);
}

#[test]
fn an_unreadable_sheet_is_an_internal_error_for_desk_and_a_warning_for_status() {
    let s = Scratch::new("broken");
    timer(&s);
    std::fs::create_dir_all(s.state().join("week/2026-W48")).unwrap();
    std::fs::write(s.state().join("week/2026-W48/sheet.json"), "{ not json").unwrap();
    assert_eq!(s.run(NOON, &["desk", "--json"]).status.code(), Some(3));
    let st = s.json(NOON, &["week", "status", "--json"]);
    assert_eq!(st["warnings"].as_array().unwrap().len(), 1);
}
