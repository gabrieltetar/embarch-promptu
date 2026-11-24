//! `promptu week pack` (docs/contract.md §14.4, §14.5): the week's events, read
//! from `log/`, `drafts/` and the transcripts, with no model and no network.
//!
//! This build writes the `run` events: what each rendered run's footer says,
//! and how the owner changed the prompt before he sent it. The other event
//! kinds (`session`, `unmatched`, `outcome`, `carried`) are counted as 0 until
//! their own unit (EMB-274) builds them.

use crate::footer::{self, LEAD};
use crate::state::{self, LogEntry};
use crate::{claude, config, used};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Weekday};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A usage error (exit 2) or an internal one (exit 3).
pub struct Fail {
    pub code: i32,
    pub msg: String,
}

fn usage(msg: impl Into<String>) -> Fail {
    Fail { code: 2, msg: msg.into() }
}

/// `YYYY-Www` to its Monday.
pub(crate) fn week_monday(week: &str) -> Option<NaiveDate> {
    let (y, w) = week.split_once("-W")?;
    if y.len() != 4 || w.len() != 2 {
        return None;
    }
    NaiveDate::from_isoywd_opt(y.parse().ok()?, w.parse().ok()?, Weekday::Mon)
}

pub(crate) fn week_of(d: NaiveDate) -> String {
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

/// The window: the seven local days before the run's day (§14.4, forks W1 a).
/// The run's day is today for the current week, and the schedule's weekday of
/// the week for a named one.
fn window(week: Option<&str>, today: NaiveDate, weekday: Weekday) -> Result<(String, NaiveDate, NaiveDate), Fail> {
    let (id, run_day) = match week {
        None => (week_of(today), today),
        Some(w) => {
            let monday = week_monday(w).ok_or_else(|| usage(format!("`{w}` is not a week like 2026-W48")))?;
            (w.to_string(), monday + Duration::days(weekday.num_days_from_monday() as i64))
        }
    };
    Ok((id, run_day - Duration::days(7), run_day - Duration::days(1)))
}

// ---- the transcripts ----

struct Owner {
    at: Option<DateTime<FixedOffset>>,
    at_text: String,
    cwd: String,
    text: String,
    footers: Vec<String>,
}

/// A `Send as is / Edit first` question the terminal skill asked, and what came back.
struct DraftAsk {
    seq: usize,
    at_text: String,
    answer: Option<String>,
}

struct Transcript {
    session: String,
    owner: Vec<Owner>,
    /// `(seq, text)` of every tool result.
    results: Vec<(usize, String)>,
    asks: Vec<DraftAsk>,
}

fn parse_time(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s.trim()).ok()
}

fn is_owner_turn(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("user")
        && v.get("isMeta").and_then(Value::as_bool) != Some(true)
        && v.pointer("/origin/kind").and_then(Value::as_str).is_none_or(|k| k == "human")
}

fn read_transcript(path: &Path) -> Option<Transcript> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut t = Transcript { session: path.file_stem()?.to_string_lossy().into_owned(), owner: vec![], results: vec![], asks: vec![] };
    let mut ask_ids: Vec<(String, usize)> = vec![];
    for (seq, line) in text.lines().enumerate() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let at_text = v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();
        let blocks = v["message"]["content"].as_array().into_iter().flatten();
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use") && b.get("name").and_then(Value::as_str) == Some("AskUserQuestion")) {
                    let drafty = b["input"]["questions"].as_array().into_iter().flatten().flat_map(|q| q["options"].as_array().into_iter().flatten()).any(|o| o.get("label").and_then(Value::as_str).is_some_and(|l| l.starts_with("Send as is")));
                    if drafty {
                        ask_ids.push((b.get("id").and_then(Value::as_str).unwrap_or("").to_string(), t.asks.len()));
                        t.asks.push(DraftAsk { seq, at_text: at_text.clone(), answer: None });
                    }
                }
            }
            Some("user") => {
                for b in blocks.filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                    let body = used::texts(&b["content"]).join("\n");
                    if let Some(id) = b.get("tool_use_id").and_then(Value::as_str) {
                        if let Some((_, i)) = ask_ids.iter().find(|(a, _)| a == id) {
                            t.asks[*i].answer.get_or_insert_with(|| body.clone());
                        }
                    }
                    t.results.push((seq, body));
                }
                if is_owner_turn(&v) {
                    let body = used::texts(&v["message"]["content"]).join("\n");
                    let footers: Vec<String> = body.lines().map(str::trim_end).filter(|l| l.starts_with(LEAD)).map(str::to_string).collect();
                    if !footers.is_empty() {
                        t.owner.push(Owner { at: parse_time(&at_text), at_text, cwd: v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string(), text: body, footers });
                    }
                }
            }
            _ => {}
        }
    }
    Some(t)
}

// ---- comparing a sent prompt with `rendered` ----

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").trim().to_string()
}

/// The footer line a rendered prompt ends in.
fn footer_line(rendered: &str) -> Option<String> {
    rendered.lines().rev().map(str::trim_end).find(|l| l.starts_with(LEAD)).map(str::to_string)
}

/// Most lines either side compared by the diff; past it the whole text is one change.
const DIFF_MAX_LINES: usize = 1500;
const CONTEXT: usize = 1;

/// A line diff of `sent` against `rendered`: `-` for a line the owner removed,
/// `+` for one he added, one line of context, hunks headed `@@ -old +new @@`.
/// `None` when the two are equal.
fn diff(rendered: &str, sent: &str) -> Option<String> {
    let (a_s, b_s) = (norm(rendered), norm(sent));
    if a_s == b_s {
        return None;
    }
    let a: Vec<&str> = a_s.lines().map(str::trim_end).collect();
    let b: Vec<&str> = b_s.lines().map(str::trim_end).collect();
    // ops: ' ' keep, '-' remove, '+' add.
    let mut ops: Vec<(char, &str)> = vec![];
    if a.len() > DIFF_MAX_LINES || b.len() > DIFF_MAX_LINES {
        ops.extend(a.iter().map(|l| ('-', *l)));
        ops.extend(b.iter().map(|l| ('+', *l)));
    } else {
        let (n, m) = (a.len(), b.len());
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if a[i] == b[j] {
                ops.push((' ', a[i]));
                i += 1;
                j += 1;
            } else if lcs[i + 1][j] >= lcs[i][j + 1] {
                ops.push(('-', a[i]));
                i += 1;
            } else {
                ops.push(('+', b[j]));
                j += 1;
            }
        }
        ops.extend(a[i..].iter().map(|l| ('-', *l)));
        ops.extend(b[j..].iter().map(|l| ('+', *l)));
    }
    // Line numbers (1-based) at each op, then the hunks.
    let mut pos = vec![];
    let (mut o, mut n) = (1usize, 1usize);
    for (c, _) in &ops {
        pos.push((o, n));
        if *c != '+' {
            o += 1;
        }
        if *c != '-' {
            n += 1;
        }
    }
    let changed: Vec<usize> = (0..ops.len()).filter(|&k| ops[k].0 != ' ').collect();
    let mut out = String::new();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= end + 2 * CONTEXT + 1 {
            k += 1;
            end = changed[k];
        }
        let end = (end + CONTEXT).min(ops.len() - 1);
        out.push_str(&format!("@@ -{} +{} @@\n", pos[start].0, pos[start].1));
        for (c, l) in &ops[start..=end] {
            out.push_str(&format!("{c}{l}\n"));
        }
        k += 1;
    }
    Some(out)
}

fn changed_lines(d: &Option<String>) -> usize {
    d.as_deref().map(|d| d.lines().filter(|l| l.starts_with('+') || l.starts_with('-')).count()).unwrap_or(0)
}

// ---- finding a run's sent prompt ----

/// Whether a transcript record's `cwd` is in the run's project.
fn same_project(rec_cwd: &str, key: &str) -> bool {
    if rec_cwd.is_empty() {
        return false;
    }
    let p = Path::new(rec_cwd);
    let rk = if p.is_dir() { claude::project_folder(&claude::project_root(p).to_string_lossy()) } else { claude::project_folder(rec_cwd) };
    rk == key || rk.starts_with(&format!("{key}-"))
}

fn sent_record(run: &LogEntry, txs: &[Transcript], claimed: &mut HashSet<(usize, usize)>) -> Value {
    let rendered = &run.rendered;
    let log_time = parse_time(&run.date);
    let key = run.project_key.clone().unwrap_or_else(|| claude::project_folder(&claude::project_root(Path::new(&run.cwd)).to_string_lossy()));

    // 1. The chat: the first owner message after the log time, in this
    //    project, carrying the same footer line (decision 44).
    if let Some(line) = footer_line(rendered) {
        let mut cands: Vec<(usize, usize, Option<String>)> = vec![];
        for (ti, t) in txs.iter().enumerate() {
            let first = t.owner.iter().enumerate().find(|(oi, o)| {
                !claimed.contains(&(ti, *oi))
                    && o.footers.contains(&line)
                    && same_project(&o.cwd, &key)
                    && match (log_time, o.at) {
                        (Some(l), Some(m)) => m >= l,
                        _ => true,
                    }
            });
            if let Some((oi, o)) = first {
                cands.push((ti, oi, diff(rendered, &o.text)));
            }
        }
        if !cands.is_empty() {
            let tie = cands.len() > 1;
            cands.sort_by(|a, b| changed_lines(&a.2).cmp(&changed_lines(&b.2)).then(txs[a.0].owner[a.1].at.cmp(&txs[b.0].owner[b.1].at)));
            let (ti, oi, d) = cands.remove(0);
            claimed.insert((ti, oi));
            let o = &txs[ti].owner[oi];
            let mut s = json!({
                "source": "transcript", "session": txs[ti].session, "at": o.at_text, "text": o.text,
                "edit": if d.is_some() { "edited" } else { "unchanged" },
            });
            if let Some(d) = d {
                s["diff"] = Value::String(d);
            }
            if tie {
                s["tie"] = Value::Bool(true);
            }
            return s;
        }
    }

    // 2. The terminal: no owner message carries the prompt; the answer to its
    //    "Send as is / Edit first" question, after the render that printed this run.
    let draft = state::dir().ok().and_then(|d| std::fs::read_to_string(d.join("drafts").join(format!("{}.md", run.run))).ok());
    for t in txs {
        let Some(seen) = t.results.iter().find(|(_, r)| r.contains(&run.run)).map(|(s, _)| *s) else { continue };
        let Some(ask) = t.asks.iter().find(|a| a.seq > seen) else { continue };
        let answer = ask.answer.clone().unwrap_or_default();
        let choice = if answer.contains("Edit first") {
            "edit_first"
        } else if answer.contains("Send as is") {
            "send_as_is"
        } else {
            "cancelled"
        };
        let mut s = json!({ "source": "terminal", "session": t.session, "at": ask.at_text, "choice": choice });
        match choice {
            "send_as_is" => {
                s["text"] = Value::String(rendered.clone());
                s["edit"] = Value::String("unchanged".into());
            }
            "edit_first" => match &draft {
                Some(text) => {
                    let d = diff(rendered, text);
                    s["text"] = Value::String(text.clone());
                    s["edit"] = Value::String(if d.is_some() { "edited" } else { "unchanged" }.into());
                    if let Some(d) = d {
                        s["diff"] = Value::String(d);
                    }
                }
                None => {
                    s["text"] = Value::Null;
                    s["edit"] = Value::String("unknown".into());
                }
            },
            _ => {
                s["text"] = Value::Null;
                s["edit"] = Value::String("unknown".into());
            }
        }
        return s;
    }

    // 3. A draft the owner changed, and nothing else that says it was sent.
    if let Some(text) = draft {
        if let Some(d) = diff(rendered, &text) {
            return json!({ "source": "draft", "text": text, "edit": "edited", "diff": d });
        }
    }
    Value::Null
}

// ---- the run event ----

fn run_event(id: usize, run: &LogEntry, sent: Value) -> Value {
    let f = run.rendered.lines().rev().map(str::trim_end).find(|l| l.starts_with(LEAD)).and_then(|l| footer::parse(l).ok().map(|f| (l.to_string(), f)));
    let (line, f) = match f {
        Some((l, f)) => (Some(l), f),
        None => (None, footer::Footer::default()),
    };
    let other_text: serde_json::Map<String, Value> = state::load_run(&run.run)
        .ok()
        .flatten()
        .map(|r| r.answers.iter().filter_map(|(q, a)| a.other.as_ref().map(|t| (q.clone(), Value::String(t.clone())))).collect())
        .unwrap_or_default();
    let mut sent = sent;
    let edit = sent.as_object_mut().and_then(|o| o.remove("edit")).unwrap_or_else(|| Value::String("unknown".into()));
    let diff = sent.as_object_mut().and_then(|o| o.remove("diff"));
    let mut e = json!({
        "id": format!("e{id}"), "kind": "run", "run": run.run, "date": run.date.get(..10).unwrap_or(""),
        "project": run.project_key, "pattern": run.pattern, "version": run.version, "mode": run.mode, "rewritten": run.rewritten,
        "footer": line,
        "asked": f.asked.len(), "filled": f.filled.len(), "skipped": f.skipped.len(), "assumed": f.assume.len(),
        "skipped_questions": f.skipped,
        "assumed_questions": f.assume,
        "not_recommended": f.asked.iter().filter(|(_, c)| c != "rec" && c != "all").map(|(q, c)| json!({"question": q, "answer": c})).collect::<Vec<_>>(),
        "other_text": other_text,
        "changed_fills": f.filled.iter().filter(|(_, _, c)| *c).map(|(q, _, _)| q.clone()).collect::<Vec<_>>(),
        "picked_by_owner": f.pick_owner,
    });
    e["sent"] = sent;
    e["edit"] = edit;
    if let Some(d) = diff {
        e["diff"] = d;
    }
    e
}

// ---- the command ----

pub struct Args {
    pub week: Option<String>,
    pub projects: Option<PathBuf>,
}

/// Build and write the pack. Returns the output object (§14.4).
pub fn pack(args: &Args) -> Result<Value, Fail> {
    let table = config::table();
    let ((weekday, _, _), warning) = config::week_schedule(&table);
    let today = config::today();
    let (week, from, to) = window(args.week.as_deref(), today, weekday)?;

    let projects = match &args.projects {
        Some(p) => {
            std::fs::read_dir(p).map_err(|e| usage(format!("{}: {e}", p.display())))?;
            Some(p.clone())
        }
        None => claude::config_dir().map(|c| c.join("projects")),
    };
    let oldest = today - Duration::days(used::WINDOW_DAYS);
    let txs: Vec<Transcript> = projects.map(|p| used::transcripts_in(&p, oldest)).unwrap_or_default().iter().filter_map(|f| read_transcript(f)).collect();

    let mut claimed = HashSet::new();
    let mut events: Vec<Value> = vec![];
    for run in state::all_logs() {
        let Some(date) = run.date.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) else { continue };
        if date < from || date > to {
            continue;
        }
        let sent = sent_record(&run, &txs, &mut claimed);
        events.push(run_event(events.len() + 1, &run, sent));
    }
    let runs = events.len();

    let dir = state::dir().map_err(|e| Fail { code: 3, msg: e })?.join("week");
    let pack_path = dir.join(&week).join("pack.json");
    let doc = json!({
        "promptu": 1, "week": week, "window": { "from": from.to_string(), "to": to.to_string() },
        "built": chrono::Local::now().to_rfc3339(), "events": events,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| Fail { code: 3, msg: e.to_string() })?;
    state::write_atomic(&pack_path, &text).map_err(|msg| Fail { code: 3, msg })?;
    let through = state::packed_through().map_or(to, |t| t.max(to));
    let packed = json!({ "promptu": 1, "through": through.to_string() }).to_string();
    state::write_atomic(&dir.join("packed.json"), &packed).map_err(|msg| Fail { code: 3, msg })?;

    let mut out = json!({
        "promptu": 1, "week": week, "pack": pack_path.to_string_lossy(),
        "window": { "from": from.to_string(), "to": to.to_string() },
        "runs": runs, "sessions": 0,
        "events": { "run": runs, "session": 0, "unmatched": 0, "outcome": 0, "carried": 0 },
    });
    if let Some(w) = warning {
        out["warnings"] = json!([w]);
    }
    Ok(out)
}

/// The text view of `pack`'s output.
pub fn text(v: &Value) -> String {
    let n = |k: &str| v["events"][k].as_u64().unwrap_or(0);
    let mut s = format!(
        "week {}: {} to {}\n{} run(s), {} session(s), {} unmatched, {} outcome(s), {} carried\npack: {}\n",
        v["week"].as_str().unwrap_or(""),
        v["window"]["from"].as_str().unwrap_or(""),
        v["window"]["to"].as_str().unwrap_or(""),
        n("run"),
        n("session"),
        n("unmatched"),
        n("outcome"),
        n("carried"),
        v["pack"].as_str().unwrap_or("")
    );
    for w in v["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        s.push_str(&format!("warning: {w}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_week_has_its_monday_and_the_window_is_the_seven_days_before_the_run_day() {
        assert_eq!(week_monday("2026-W48"), NaiveDate::from_ymd_opt(2026, 11, 23));
        assert_eq!(week_monday("2026-48"), None);
        assert_eq!(week_monday("2026-W60"), None);
        let today = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        let (w, from, to) = window(None, today, Weekday::Wed).ok().unwrap();
        assert_eq!((w.as_str(), from.to_string(), to.to_string()), ("2026-W48", "2026-11-18".into(), "2026-11-24".into()));
        let (w, from, to) = window(Some("2026-W47"), today, Weekday::Wed).ok().unwrap();
        assert_eq!((w.as_str(), from.to_string(), to.to_string()), ("2026-W47", "2026-11-11".into(), "2026-11-17".into()));
        assert!(window(Some("nope"), today, Weekday::Wed).is_err());
    }

    #[test]
    fn a_diff_is_none_when_equal_after_trim_and_crlf_and_shows_changed_lines_otherwise() {
        assert_eq!(diff("a\nb\n", "  a\r\nb  \r\n"), None);
        let d = diff("## Goal\nfix it\n\n— promptu x@1", "## Goal\nfix it now\n\n— promptu x@1").unwrap();
        assert!(d.contains("-fix it\n+fix it now\n"), "{d}");
        assert!(d.starts_with("@@ -1 +1 @@"), "{d}");
    }
}
