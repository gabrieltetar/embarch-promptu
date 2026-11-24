//! `promptu week run` and `week timer` (docs/contract.md §14.4, forks W19, W20)
//! against a scratch state folder, a pinned day and a fake `claude`
//! (`PROMPTU_CLAUDE`). No model is called, nothing is installed, and nothing
//! reads the real `~/.claude` or `~/.local/state`.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");
const TODAY: &str = "2026-11-25";
const WEEK: &str = "2026-W48";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "promptu-weekrun-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for d in ["home", "claude/projects", "cfg", "state/log", "fake"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let fake = root.join("fake/claude");
        std::fs::write(&fake, "#!/bin/sh\ncat > /dev/null\nif [ -f \"$PROMPTU_FAKE_DIR/exit\" ]; then exit 1; fi\ncat \"$PROMPTU_FAKE_DIR/reply.json\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Scratch { root }
    }

    fn week_dir(&self) -> PathBuf {
        self.root.join("state/week")
    }

    /// One run in the window, so the pack has an event and the drafter is called.
    fn log(&self) {
        let e = json!({
            "promptu": 1, "run": "20261120-090000-aaaa", "date": "2026-11-20T09:00:00+00:00", "cwd": "/x", "project_key": "-x",
            "pattern": "handoff", "version": 1, "mode": "prompt", "original": "rough", "rendered": "r",
            "rewritten": true, "rewrite_skipped": null, "rewrite_model": "sonnet", "rewrite_effort": "low", "rewrite_ms": 900, "wait_ms": 1200
        });
        std::fs::write(self.root.join("state/log/20261120-090000-aaaa.json"), e.to_string()).unwrap();
    }

    fn reply(&self, v: Value) {
        let env = json!({"type": "result", "is_error": false, "terminal_reason": "completed", "result": v.to_string(), "total_cost_usd": 1.25});
        std::fs::write(self.root.join("fake/reply.json"), env.to_string()).unwrap();
    }

    fn promptu(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.root.join("state"))
            .env("PROMPTU_CLAUDE", self.root.join("fake/claude"))
            .env("PROMPTU_FAKE_DIR", self.root.join("fake"))
            .env("PROMPTU_TODAY", TODAY)
            .env("PROMPTU_NOW", "2026-11-25T08:30:00+00:00")
            .env("TZ", "UTC")
            .args(args)
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let o = self.promptu(args);
        (o.status.code().unwrap(), serde_json::from_slice(&o.stdout).unwrap_or(Value::Null))
    }

    fn record(&self, week: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.week_dir().join(week).join("run.json")).unwrap()).unwrap()
    }

    fn lock(&self, pid: u32, week: &str, started: &str) {
        std::fs::create_dir_all(self.week_dir()).unwrap();
        std::fs::write(self.week_dir().join("run.lock"), json!({"pid": pid, "week": week, "started": started}).to_string()).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A pid that is not running: a child that has been waited for.
fn dead_pid() -> u32 {
    let mut c = Command::new("true").spawn().unwrap();
    let pid = c.id();
    c.wait().unwrap();
    pid
}

#[test]
fn a_run_packs_drafts_and_records_itself_for_status() {
    let s = Scratch::new("ok");
    s.log();
    s.reply(json!({ "lines": [] }));
    let (code, v) = s.json(&["week", "run", "--json"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["events"]["run"], 1);
    let r = s.record(WEEK);
    assert_eq!((r["status"].as_str(), r["kind"].as_str()), (Some("ok"), Some("manual")));
    assert!(r["finished"].is_string() && r["scheduled_for"].is_null());
    assert_eq!(r["cost_usd"], 1.25);
    assert!(!s.week_dir().join("run.lock").exists(), "the lock is released");
    let (_, st) = s.json(&["week", "status", "--json"]);
    assert_eq!(st["last"]["status"], "ok");
    assert_eq!(st["last"]["cost_usd"], 1.25);
    assert_eq!(st["running"], false);
}

#[test]
fn a_scheduled_run_says_so_and_names_its_instant() {
    let s = Scratch::new("sched");
    let (code, _) = s.json(&["week", "run", "--scheduled", "--json"]);
    assert_eq!(code, 0);
    let r = s.record(WEEK);
    assert_eq!(r["kind"], "scheduled");
    assert_eq!(r["scheduled_for"].as_str().map(|t| &t[..16]), Some("2026-11-25T08:00"));
}

#[test]
fn a_second_run_while_one_runs_exits_4_and_changes_nothing() {
    let s = Scratch::new("busy");
    let started = chrono_now();
    s.lock(std::process::id(), WEEK, &started);
    let before = std::fs::read_to_string(s.week_dir().join("run.lock")).unwrap();
    let o = s.promptu(&["week", "run"]);
    assert_eq!(o.status.code(), Some(4), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("already in progress"));
    assert!(!s.week_dir().join(WEEK).exists(), "no record, no pack, no sheet");
    assert_eq!(std::fs::read_to_string(s.week_dir().join("run.lock")).unwrap(), before);
}

#[test]
fn a_run_killed_part_way_leaves_no_lock_that_blocks_the_next() {
    let s = Scratch::new("killed");
    // The dead run's record says running, as a kill -9 leaves it.
    let started = chrono_now();
    s.lock(dead_pid(), "2026-W47", &started);
    std::fs::create_dir_all(s.week_dir().join("2026-W47")).unwrap();
    std::fs::write(s.week_dir().join("2026-W47/run.json"), json!({"promptu": 1, "week": "2026-W47", "started": started, "finished": null, "status": "running", "error": null}).to_string()).unwrap();
    let (code, _) = s.json(&["week", "run", "--json"]);
    assert_eq!(code, 0);
    assert!(!s.week_dir().join("run.lock").exists());
    let old = s.record("2026-W47");
    assert_eq!(old["status"], "failed");
    assert!(old["error"].as_str().unwrap().starts_with("interrupted"));
    assert_eq!(s.record(WEEK)["status"], "ok");
}

#[test]
fn a_lock_older_than_two_hours_is_stale_even_with_a_live_pid() {
    let s = Scratch::new("old");
    s.lock(std::process::id(), WEEK, "2020-01-01T00:00:00+00:00");
    let (code, _) = s.json(&["week", "run", "--json"]);
    assert_eq!(code, 0);
}

#[test]
fn a_failed_drafter_leaves_a_failed_record_status_shows_and_no_lock() {
    let s = Scratch::new("fail");
    s.log();
    std::fs::write(s.root.join("fake/exit"), "").unwrap();
    let o = s.promptu(&["week", "run"]);
    assert_eq!(o.status.code(), Some(1));
    let r = s.record(WEEK);
    assert_eq!(r["status"], "failed");
    assert!(r["error"].is_string() && r["finished"].is_string());
    assert!(!s.week_dir().join("run.lock").exists());
    let (_, st) = s.json(&["week", "status", "--json"]);
    assert_eq!(st["last"]["status"], "failed");
    assert_eq!(st["running"], false);
}

#[test]
fn a_bad_week_is_a_usage_error_and_takes_no_lock() {
    let s = Scratch::new("bad");
    let o = s.promptu(&["week", "run", "--week", "nope"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(!s.week_dir().join("run.lock").exists());
}

#[test]
fn the_timer_prints_the_units_and_installs_nothing() {
    let s = Scratch::new("timer");
    let (code, v) = s.json(&["week", "timer", "--json"]);
    assert_eq!(code, 0);
    let timer = v["timer"].as_str().unwrap();
    assert!(timer.contains("\nOnCalendar=Wed *-*-* 08:00:00\n"), "{timer}");
    assert!(timer.contains("\nPersistent=true\n"), "{timer}");
    let exec = v["service"].as_str().unwrap().lines().find(|l| l.starts_with("ExecStart=")).unwrap().to_string();
    assert!(exec.contains(BIN) && exec.ends_with("week run --scheduled"), "{exec}");
    assert_eq!(v["recorded"], false);
    assert!(!s.week_dir().join("timer.json").exists());
    assert!(!s.root.join("home/.config").exists(), "nothing written to the user's systemd folder");
    let o = s.promptu(&["week", "timer"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("systemctl --user enable --now promptu-week.timer"));
}

#[test]
fn the_timer_follows_the_configured_schedule_and_enabled_records_the_instant() {
    let s = Scratch::new("sched-cfg");
    std::fs::write(s.root.join("cfg/config.toml"), "week_schedule = \"fri 17:30\"\n").unwrap();
    let (_, v) = s.json(&["week", "timer", "--enabled", "--json"]);
    assert!(v["timer"].as_str().unwrap().contains("OnCalendar=Fri *-*-* 17:30:00"));
    assert_eq!(v["recorded"], true);
    let (_, st) = s.json(&["week", "status", "--json"]);
    assert_eq!(st["timer"]["installed"], true);
    assert_eq!(st["schedule"], "fri 17:30");
}

fn chrono_now() -> String {
    // RFC 3339 UTC with seconds, from the system clock: enough for a lock's age.
    let out = Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%S+00:00"]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}
