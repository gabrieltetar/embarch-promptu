//! `promptu week status`, `promptu week show` and `promptu desk` (docs/contract.md
//! §9, §14.4): read-only views of the week folder. No model, no network, and
//! nothing is written, so the state folder need not exist.

use crate::week::Fail;
use crate::{config, state};
use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveTime, TimeZone};
use serde_json::{json, Value};
use std::path::PathBuf;

/// A run recorded as `running` is read as stale after this long: a drafter's
/// timeout is at most an hour (§14.2), so a record older than two is a crash
/// (docs/forks.md W12).
const RUNNING_STALE_HOURS: i64 = 2;
/// How long past a scheduled instant before the run counts as missed (W9 a).
const MISSED_GRACE_HOURS: i64 = 2;

fn fail(code: i32, msg: impl Into<String>) -> Fail {
    Fail { code, msg: msg.into() }
}

/// Now, local; `PROMPTU_NOW=<RFC 3339 instant>` pins it for tests, as
/// `PROMPTU_TODAY` pins the day.
fn now() -> DateTime<Local> {
    std::env::var("PROMPTU_NOW")
        .ok()
        .and_then(|s| DateTime::parse_from_rfc3339(s.trim()).ok())
        .map(|d| d.with_timezone(&Local))
        .unwrap_or_else(Local::now)
}

fn week_dir() -> Result<PathBuf, Fail> {
    Ok(state::dir().map_err(|e| fail(3, e))?.join("week"))
}

pub(crate) fn is_week_id(s: &str) -> bool {
    s.len() == 8 && s.get(4..6) == Some("-W") && s[..4].bytes().all(|b| b.is_ascii_digit()) && s[6..].bytes().all(|b| b.is_ascii_digit())
}

/// Week folder names, newest first. No folder, no weeks.
fn weeks() -> Result<Vec<String>, Fail> {
    let mut v: Vec<String> = std::fs::read_dir(week_dir()?)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_week_id(n))
        .collect();
    v.sort_unstable_by(|a, b| b.cmp(a));
    Ok(v)
}

fn read_json(path: &std::path::Path) -> Result<Option<Value>, Fail> {
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(None) };
    serde_json::from_str(&text).map(Some).map_err(|e| fail(3, format!("{}: {e}", path.display())))
}

/// The newest week folder whose sheet has `superseded_by: null` (§14.1).
pub(crate) fn open_sheet() -> Result<Option<(String, Value)>, Fail> {
    for w in weeks()? {
        if let Some(s) = read_json(&week_dir()?.join(&w).join("sheet.json"))? {
            if s["superseded_by"].is_null() {
                return Ok(Some((w, s)));
            }
        }
    }
    Ok(None)
}

/// The earlier sheet a pack for `week` carries from (§14.1, decision 46): the
/// sheet this week's own sheet supersedes, else the newest open sheet of an
/// earlier week. `None` with no such sheet.
pub(crate) fn carry_source(week: &str) -> Result<Option<(String, Value)>, Fail> {
    let dir = week_dir()?;
    if let Some(own) = read_json(&dir.join(week).join("sheet.json"))? {
        let Some(from) = own["supersedes"].as_str().filter(|w| is_week_id(w)) else { return Ok(None) };
        return Ok(read_json(&dir.join(from).join("sheet.json"))?.map(|s| (from.to_string(), s)));
    }
    for w in weeks()?.into_iter().filter(|w| w.as_str() < week) {
        if let Some(s) = read_json(&dir.join(&w).join("sheet.json"))? {
            if s["superseded_by"].is_null() {
                return Ok(Some((w, s)));
            }
        }
    }
    Ok(None)
}

/// A pack's events of an earlier week by id; empty when its pack is gone.
pub(crate) fn pack_events(week: &str) -> Vec<Value> {
    let Ok(dir) = week_dir() else { return vec![] };
    read_json(&dir.join(week).join("pack.json")).ok().flatten().and_then(|p| p["events"].as_array().cloned()).unwrap_or_default()
}

pub(crate) fn sheet_lines(sheet: &Value) -> Vec<&Value> {
    lines(sheet).collect()
}

fn lines(sheet: &Value) -> impl Iterator<Item = &Value> {
    sheet["groups"].as_array().into_iter().flatten().flat_map(|g| g["lines"].as_array().into_iter().flatten())
}

fn open_in(lines: &[Value]) -> usize {
    lines.iter().filter(|l| l["status"] == "open").count()
}

fn parse_time(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s.trim()).ok()
}

/// The schedule's instant on a local day.
fn instant(day: NaiveDate, h: u32, m: u32) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&day.and_time(NaiveTime::from_hms_opt(h, m, 0)?)).earliest()
}

/// The first scheduled instant after `now`.
fn next_after(now: DateTime<Local>, sched: (chrono::Weekday, u32, u32)) -> Option<DateTime<Local>> {
    (0..=8).filter_map(|i| instant(now.date_naive() + Duration::days(i), sched.1, sched.2).filter(|_| (now.date_naive() + Duration::days(i)).weekday() == sched.0)).find(|t| *t > now)
}

/// The latest scheduled instant at or before `now`.
fn latest_before(now: DateTime<Local>, sched: (chrono::Weekday, u32, u32)) -> Option<DateTime<Local>> {
    (0..=8).filter_map(|i| instant(now.date_naive() - Duration::days(i), sched.1, sched.2).filter(|_| (now.date_naive() - Duration::days(i)).weekday() == sched.0)).find(|t| *t <= now)
}

/// Every `week/<week>/run.json`, newest `started` first.
fn runs() -> Result<Vec<Value>, Fail> {
    let mut v = vec![];
    for w in weeks()? {
        if let Ok(Some(r)) = read_json(&week_dir()?.join(&w).join("run.json")) {
            v.push(r);
        }
    }
    v.sort_by_key(|r| std::cmp::Reverse(r["started"].as_str().and_then(parse_time)));
    Ok(v)
}

/// `promptu week status` (§14.4): what the chat asks. Never asks the scheduler.
pub fn status() -> Result<Value, Fail> {
    let table = config::table();
    let (sched, warning) = config::week_schedule(&table);
    let mut warnings: Vec<String> = warning.into_iter().collect();
    let now = now();

    let timer = read_json(&week_dir()?.join("timer.json")).unwrap_or_else(|e| {
        warnings.push(e.msg);
        None
    });
    let installed_at = timer.as_ref().and_then(|t| t["installed"].as_str()).map(str::to_string);
    let installed = installed_at.as_deref().and_then(parse_time);

    let runs = runs()?;
    let last = runs.first().map(|r| json!({ "week": r["week"], "started": r["started"], "finished": r["finished"], "status": r["status"], "error": r["error"] }));
    let running = runs.iter().any(|r| {
        r["status"] == "running" && r["finished"].is_null() && r["started"].as_str().and_then(parse_time).is_some_and(|s| now.signed_duration_since(s) < Duration::hours(RUNNING_STALE_HOURS))
    });

    let next = next_after(now, sched).map(|t| t.to_rfc3339());
    let mut missed_since = None;
    if let (Some(inst), Some(s)) = (installed, latest_before(now, sched)) {
        let started_since = runs.iter().filter_map(|r| r["started"].as_str().and_then(parse_time)).any(|t| t >= s);
        if s > inst && now >= s + Duration::hours(MISSED_GRACE_HOURS) && !started_since {
            missed_since = Some(s.to_rfc3339());
        }
    }

    let sheet = match open_sheet() {
        Ok(s) => s,
        Err(e) => {
            warnings.push(e.msg);
            None
        }
    };
    let open = sheet.as_ref().map_or(0, |(_, s)| lines(s).filter(|l| l["status"] == "open").count());
    Ok(json!({
        "promptu": 1,
        "schedule": format!("{} {:02}:{:02}", format!("{:?}", sched.0).to_lowercase(), sched.1, sched.2),
        "timer": { "installed": timer.is_some(), "installed_at": installed_at },
        "last": last,
        "running": running,
        "next": next,
        "missed": missed_since.is_some(),
        "missed_since": missed_since,
        "open": open,
        "sheet": sheet.as_ref().map(|(w, s)| json!({ "week": w, "generated": s["generated"] })),
        "warnings": warnings,
    }))
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

pub fn status_text(v: &Value) -> String {
    let mut out = format!("schedule: {}\n", s(&v["schedule"]));
    out.push_str(&match v["timer"]["installed"].as_bool() {
        Some(true) => format!("timer: installed {}\n", s(&v["timer"]["installed_at"])),
        _ => "timer: not installed\n".to_string(),
    });
    out.push_str(&match v["last"].as_object() {
        Some(l) => {
            let l = Value::Object(l.clone());
            let err = l["error"].as_str().map(|e| format!(": {e}")).unwrap_or_default();
            format!("last run: {} {} at {}{err}\n", s(&l["week"]), s(&l["status"]), s(&l["started"]))
        }
        None => "last run: none\n".to_string(),
    });
    if v["running"] == true {
        out.push_str("running now\n");
    }
    out.push_str(&format!("next run: {}\n", s(&v["next"])));
    if v["missed"] == true {
        out.push_str(&format!("missed: the run scheduled for {} did not start\n", s(&v["missed_since"])));
    }
    match v["sheet"].as_object() {
        Some(sh) => out.push_str(&format!("waiting: {} open in {}\n", v["open"], sh.get("week").and_then(Value::as_str).unwrap_or(""))),
        None => out.push_str("waiting: no sheet\n"),
    }
    for w in v["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        out.push_str(&format!("warning: {w}\n"));
    }
    out
}

/// `promptu week show` (§14.4): the sheet, as `sheet.json` has it (`json`) or as text.
pub fn show(week: Option<&str>, json: bool) -> Result<String, Fail> {
    let (id, sheet) = match week {
        Some(w) => {
            if !is_week_id(w) {
                return Err(fail(2, format!("`{w}` is not a week like 2026-W48")));
            }
            match read_json(&week_dir()?.join(w).join("sheet.json"))? {
                Some(sh) => (w.to_string(), sh),
                None => return Err(fail(2, format!("no sheet for {w}"))),
            }
        }
        None => open_sheet()?.ok_or_else(|| fail(2, "no sheet yet"))?,
    };
    if json {
        return serde_json::to_string_pretty(&sheet).map(|t| t + "\n").map_err(|e| fail(3, e.to_string()));
    }
    if let Ok(md) = std::fs::read_to_string(week_dir()?.join(&id).join("sheet.md")) {
        return Ok(md);
    }
    let mut out = format!("week {id}\n");
    for l in lines(&sheet) {
        let mark = if l["status"] == "cut" || l["verdict"] == "cut" { " " } else { "x" };
        out.push_str(&format!("- [{mark}] {} {}\n", s(&l["id"]), s(&l["text"])));
    }
    Ok(out)
}

/// `promptu desk` (§9): the Desk section's one fact, and none when nothing is open.
pub fn desk() -> Result<Value, Fail> {
    let Some((week, sheet)) = open_sheet()? else { return Ok(json!({ "promptu": 1, "open": 0 })) };
    let mut groups = vec![];
    let mut open = 0;
    for g in sheet["groups"].as_array().into_iter().flatten() {
        let n = open_in(g["lines"].as_array().map_or(&[], |a| a.as_slice()));
        open += n;
        if n > 0 {
            groups.push(json!({ "id": g["id"], "title": g["title"], "open": n }));
        }
    }
    if open == 0 {
        return Ok(json!({ "promptu": 1, "open": 0 }));
    }
    Ok(json!({ "promptu": 1, "open": open, "week": week, "generated": sheet["generated"], "groups": groups, "feel": sheet["feel"] }))
}

pub fn desk_text(v: &Value) -> String {
    match v["open"].as_u64() {
        Some(n) if n > 0 => format!("promptu: {n} open ({})\n", s(&v["week"])),
        _ => String::new(),
    }
}
