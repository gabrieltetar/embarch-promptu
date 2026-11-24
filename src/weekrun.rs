//! `promptu week run` and `promptu week timer` (docs/contract.md §14.4).
//!
//! `run` is `pack` then `draft` as one run, under a lock so the timer and a
//! click cannot run two at once (forks W19); it keeps `run.json` truthful
//! whatever fails. `timer` prints the systemd user units built from
//! `week_schedule` and installs nothing (decision 45).

use crate::week::{self, Fail};
use crate::{config, draft, state, weekread};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Exit code of a `week run` that finds another one running (contract §14).
pub const ALREADY_RUNNING: i32 = 4;

/// A lock older than this is stale even if its pid is alive (pid reuse after a
/// reboot): the drafter's timeout is at most an hour (§14.2, forks W12).
const LOCK_STALE_HOURS: i64 = 2;

fn fail(code: i32, msg: impl Into<String>) -> Fail {
    Fail { code, msg: msg.into() }
}

pub struct RunArgs {
    pub week: Option<String>,
    pub scheduled: bool,
}

/// The lock file `week/run.lock`, removed when the run ends. A run killed
/// before that leaves a file whose pid is gone, which the next run reclaims.
struct Lock {
    path: PathBuf,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Is the process behind this lock still there? `/proc` where it exists; else
/// only the lock's age speaks.
fn holder_alive(lock: &Value, path: &Path) -> bool {
    let started = lock["started"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    let fresh = match started {
        Some(t) => chrono::Local::now().signed_duration_since(t) < chrono::Duration::hours(LOCK_STALE_HOURS),
        None => std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).is_some_and(|d| d < std::time::Duration::from_secs(LOCK_STALE_HOURS as u64 * 3600)),
    };
    if !fresh {
        return false;
    }
    match lock["pid"].as_u64() {
        Some(pid) if Path::new("/proc/self").exists() => Path::new(&format!("/proc/{pid}")).exists(),
        _ => true,
    }
}

/// A run that died leaves `status: "running"` in its `run.json`: say so.
fn mark_interrupted(dir: &Path, lock: &Value) {
    let (Some(week), Some(started)) = (lock["week"].as_str(), lock["started"].as_str()) else { return };
    let path = dir.join(week).join("run.json");
    let Some(mut r) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return };
    if r["status"] == "running" && r["started"] == started {
        r["status"] = json!("failed");
        r["finished"] = json!(chrono::Local::now().to_rfc3339());
        r["error"] = json!("interrupted: the run's process ended before it finished");
        let _ = state::write_atomic(&path, &serde_json::to_string_pretty(&r).unwrap_or_default());
    }
}

fn acquire(dir: &Path, week: &str, started: &str) -> Result<Lock, Fail> {
    std::fs::create_dir_all(dir).map_err(|e| fail(3, format!("{}: {e}", dir.display())))?;
    let path = dir.join("run.lock");
    for _ in 0..3 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                let body = json!({ "pid": std::process::id(), "week": week, "started": started }).to_string();
                f.write_all(body.as_bytes()).map_err(|e| fail(3, format!("{}: {e}", path.display())))?;
                return Ok(Lock { path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let held: Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
                if holder_alive(&held, &path) {
                    let w = held["week"].as_str().unwrap_or("a week");
                    return Err(fail(ALREADY_RUNNING, format!("a run is already in progress ({w}, started {})", held["started"].as_str().unwrap_or("?"))));
                }
                // One reclaimer wins the rename; the other finds the file gone and retries.
                let stale = dir.join(format!("run.lock.stale-{}", std::process::id()));
                if std::fs::rename(&path, &stale).is_ok() {
                    mark_interrupted(dir, &held);
                    let _ = std::fs::remove_file(&stale);
                }
            }
            Err(e) => return Err(fail(3, format!("{}: {e}", path.display()))),
        }
    }
    Err(fail(ALREADY_RUNNING, "a run is already in progress"))
}

fn write_record(path: &Path, r: &Value) -> Result<(), Fail> {
    state::write_atomic(path, &serde_json::to_string_pretty(r).map_err(|e| fail(3, e.to_string()))?).map_err(|e| fail(3, e))
}

/// Settle `run.json` as `failed` unless `draft` already did.
fn settle_failed(path: &Path, msg: &str) {
    let Some(mut r) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return };
    if r["status"] == "running" {
        r["status"] = json!("failed");
        r["finished"] = json!(chrono::Local::now().to_rfc3339());
        r["error"] = json!(msg.split_whitespace().collect::<Vec<_>>().join(" "));
        let _ = write_record(path, &r);
    }
}

/// `pack`, then `draft`, under the lock. Output is `draft`'s with `pack`'s event counts.
pub fn run(args: &RunArgs) -> Result<Value, Fail> {
    let table = config::table();
    let week = args.week.clone().unwrap_or_else(|| week::week_of(config::today()));
    if !weekread::is_week_id(&week) || week::week_monday(&week).is_none() {
        return Err(fail(2, format!("`{week}` is not a week like 2026-W48")));
    }
    let weeks = weekread::week_dir()?;
    let started = chrono::Local::now().to_rfc3339();
    let _lock = acquire(&weeks, &week, &started)?;

    let (sched, _) = config::week_schedule(&table);
    let scheduled_for = if args.scheduled { weekread::latest_before(weekread::now(), sched).map(|t| t.to_rfc3339()) } else { None };
    let kind = if args.scheduled { "scheduled" } else { "manual" };
    let record = weeks.join(&week).join("run.json");
    write_record(
        &record,
        &json!({
            "promptu": 1, "week": week, "kind": kind, "scheduled_for": scheduled_for, "started": started, "finished": null,
            "status": "running", "error": null, "model": config::week_model(&table), "events": 0, "lines": 0, "dropped": 0,
        }),
    )?;

    let packed = match week::pack(&week::Args { week: args.week.clone(), projects: None }) {
        Ok(p) => p,
        Err(e) => {
            settle_failed(&record, &format!("pack: {}", e.msg));
            return Err(e);
        }
    };
    let meta = draft::RunMeta { kind, scheduled_for, started };
    match draft::draft(&draft::Args { week: Some(week), pack: None, run: Some(meta) }) {
        Ok(mut v) => {
            v["events"] = packed["events"].clone();
            Ok(v)
        }
        Err(e) => {
            settle_failed(&record, &e.msg);
            Err(e)
        }
    }
}

/// The text view of `run`'s output: `draft`'s, then the pack's counts.
pub fn text(v: &Value) -> String {
    let n = |k: &str| v["events"][k].as_u64().unwrap_or(0);
    format!("{}{} run(s), {} session(s), {} unmatched, {} outcome(s), {} carried\n", draft::text(v), n("run"), n("session"), n("unmatched"), n("outcome"), n("carried"))
}

// ---- the timer ----

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// `(promptu-week.service, promptu-week.timer)` for a schedule and a binary path.
pub fn units(sched: (chrono::Weekday, u32, u32), exe: &str) -> (String, String) {
    let day = WEEKDAYS[sched.0.num_days_from_monday() as usize];
    let service = format!("[Unit]\nDescription=Promptu weekly pass: pack the week and draft the sheet\n\n[Service]\nType=oneshot\nExecStart={} week run --scheduled\n", quote(exe));
    let timer = format!(
        "[Unit]\nDescription=Promptu weekly pass ({day} {:02}:{:02})\n\n[Timer]\nOnCalendar={day} *-*-* {:02}:{:02}:00\nPersistent=true\nUnit=promptu-week.service\n\n[Install]\nWantedBy=timers.target\n",
        sched.1, sched.2, sched.1, sched.2
    );
    (service, timer)
}

/// A path as one systemd word: quoted when it holds a space, `%` doubled.
fn quote(path: &str) -> String {
    let p = path.replace('%', "%%");
    if p.contains(' ') {
        format!("\"{}\"", p.replace('"', "\\\""))
    } else {
        p
    }
}

/// `promptu week timer`: the two units, printed. `enabled` also records that
/// the owner turned the timer on, in `week/timer.json`, which `status` reads;
/// nothing is installed either way (forks W20).
pub fn timer(enabled: bool) -> Result<Value, Fail> {
    let table = config::table();
    let (sched, warning) = config::week_schedule(&table);
    let exe = std::env::current_exe().map_err(|e| fail(3, e.to_string()))?;
    let (service, timer) = units(sched, &exe.to_string_lossy());
    let label = format!("{} {:02}:{:02}", format!("{:?}", sched.0).to_lowercase(), sched.1, sched.2);
    if enabled {
        let doc = json!({ "promptu": 1, "installed": chrono::Local::now().to_rfc3339(), "schedule": label });
        write_record(&weekread::week_dir()?.join("timer.json"), &doc)?;
    }
    let mut out = json!({
        "promptu": 1, "schedule": label, "service_name": "promptu-week.service", "timer_name": "promptu-week.timer",
        "service": service, "timer": timer, "recorded": enabled,
    });
    if let Some(w) = warning {
        out["warnings"] = json!([w]);
    }
    Ok(out)
}

/// The text view of `timer`'s output: each unit under its file name, then the commands that turn it on.
pub fn timer_text(v: &Value) -> String {
    let s = |k: &str| v[k].as_str().unwrap_or("");
    let mut out = format!("# ~/.config/systemd/user/{}\n{}\n# ~/.config/systemd/user/{}\n{}\n", s("service_name"), s("service"), s("timer_name"), s("timer"));
    out.push_str("# To turn it on: save the two units there, then\n#   systemctl --user daemon-reload && systemctl --user enable --now promptu-week.timer\n#   promptu week timer --enabled    (tells `week status` when; changes nothing else)\n");
    for w in v["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        out.push_str(&format!("# warning: {w}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timer_units_carry_the_schedule_persistence_and_the_binary() {
        let (service, timer) = units((chrono::Weekday::Wed, 8, 0), "/opt/my tools/promptu");
        assert!(timer.contains("\nOnCalendar=Wed *-*-* 08:00:00\n"), "{timer}");
        assert!(timer.contains("\nPersistent=true\n"), "{timer}");
        assert!(service.contains("\nExecStart=\"/opt/my tools/promptu\" week run --scheduled\n"), "{service}");
        assert!(service.contains("Type=oneshot"));
        let (_, timer) = units((chrono::Weekday::Sun, 21, 5), "/bin/p");
        assert!(timer.contains("OnCalendar=Sun *-*-* 21:05:00"));
    }
}
