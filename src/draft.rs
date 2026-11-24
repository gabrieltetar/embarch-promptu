//! `promptu week draft` (docs/contract.md §14.4, §10): the pass's one model
//! call. A stripped `claude -p` reads the week's pack and the current patterns
//! and replies with the sheet's lines as JSON; Promptu guards each line, checks
//! each proposed pattern with `promptu lint`, and writes `sheet.json`,
//! `sheet.md`, `proposed/`, `examples/` and `run.json`.
//!
//! A drafter that fails (a bad exit, a timeout, a reply with no JSON) writes
//! `run.json` with `status: "failed"` and nothing else: every sheet stays as it
//! was. A line that is broken beyond use (no kind, no text, no pack event) is
//! dropped and counted; a line whose target does not exist, or whose proposed
//! pattern fails lint, stays on the sheet as `failed` with its error (forks W13).

use crate::checks::{self, Severity};
use crate::pass::{self, Call};
use crate::pattern::{self, Set};
use crate::week::Fail;
use crate::{config, lint, state, weekread};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const SYSTEM: &str = include_str!("draft_system.md");

const KINDS: [&str; 5] = ["pattern", "memory", "new-pattern", "external", "no-change"];
const PATTERN_TARGETS: [&str; 5] = ["question", "new_question", "template", "picker", "pattern"];
const NOT_ADDRESSED: &str = "not addressed";

pub struct Args {
    pub week: Option<String>,
    pub pack: Option<PathBuf>,
}

fn usage(msg: impl Into<String>) -> Fail {
    Fail { code: 2, msg: msg.into() }
}

fn internal(msg: impl Into<String>) -> Fail {
    Fail { code: 3, msg: msg.into() }
}

fn drafter_failed(msg: impl AsRef<str>) -> Fail {
    Fail { code: 1, msg: format!("the drafter failed: {}", msg.as_ref().split_whitespace().collect::<Vec<_>>().join(" ")) }
}

/// A file or folder name the drafter chose: letters, digits, `-`, `_`, and `.`
/// inside, never a path.
fn safe_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && !s.starts_with('.') && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !s.contains("..")
}

fn slug(s: &str) -> bool {
    !s.is_empty() && s.len() <= 60 && !s.starts_with(['-', '_']) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("").trim()
}

/// A pattern file's text: the file, or the copy the binary carries.
fn raw_text(p: &pattern::Pattern) -> Option<String> {
    match p.file.strip_prefix("embedded:") {
        Some(name) => pattern::EMBEDDED.iter().find(|(n, _)| *n == name).map(|(_, t)| t.to_string()),
        None => std::fs::read_to_string(&p.file).ok(),
    }
}

/// The user message: the week, the pack and every current pattern file.
pub fn message(week: &str, pack: &Value, set: &Set) -> String {
    let mut files = String::new();
    for p in set.fragments.values().chain(set.patterns.values()) {
        if let Some(t) = raw_text(p) {
            files.push_str(&format!("<pattern name=\"{}\">\n{}\n</pattern>\n", p.name(), t.trim_end()));
        }
    }
    format!("<week>\n{week}\n</week>\n\n<pack>\n{}\n</pack>\n\n<patterns>\n{files}</patterns>\n", serde_json::to_string(pack).unwrap_or_default())
}

// ---- the guard ----

/// One line as the drafter proposed it, before ids and groups.
struct Line {
    v: Map<String, Value>,
}

impl Line {
    fn fail(&mut self, error: impl Into<String>) {
        self.v.insert("status".into(), json!("failed"));
        self.v.insert("error".into(), json!(error.into()));
    }

    fn open(&self) -> bool {
        self.v["status"] == "open"
    }

    fn kind(&self) -> &str {
        self.v["kind"].as_str().unwrap_or("")
    }

    fn pattern(&self) -> Option<&str> {
        matches!(self.kind(), "pattern" | "new-pattern").then(|| self.v["target"]["pattern"].as_str()).flatten()
    }
}

/// A pattern-changing line's target against the current set (docs/contract.md §10).
fn check_pattern_target(kind: &str, t: &Value, set: &Set) -> Result<(), String> {
    let tkind = str_of(t, "kind");
    if !PATTERN_TARGETS.contains(&tkind) {
        return Err(format!("target kind `{tkind}` is not one of {}", PATTERN_TARGETS.join(", ")));
    }
    let name = str_of(t, "pattern");
    if tkind == "pattern" {
        if kind != "new-pattern" {
            return Err("a whole new pattern file is a `new-pattern` line".into());
        }
        if !slug(name) {
            return Err(format!("`{name}` is not a pattern name (lowercase letters, digits, hyphens)"));
        }
        if set.patterns.contains_key(name) || set.fragments.contains_key(name) {
            return Err(format!("a pattern named `{name}` already exists"));
        }
        return Ok(());
    }
    let p = set.get(name).ok_or_else(|| format!("no pattern `{name}`"))?;
    let qs = set.questions(p);
    let q = str_of(t, "question");
    match tkind {
        "question" if !qs.iter().any(|x| x.id == q) => Err(format!("pattern `{name}` has no question `{q}`")),
        "new_question" if q.is_empty() => Err("a new question needs an id".into()),
        "new_question" if qs.iter().any(|x| x.id == q) => Err(format!("pattern `{name}` already has a question `{q}`")),
        _ => Ok(()),
    }
}

/// A memory line's target and files (docs/contract.md §2, §3.4).
fn check_memory(v: &Map<String, Value>, today: chrono::NaiveDate) -> Result<(), String> {
    let t = &v["target"];
    let (project, file) = (str_of(t, "project"), str_of(t, "file"));
    if str_of(t, "kind") != "memory" || !safe_name(project) {
        return Err("a memory target names its `project` folder and `file`".into());
    }
    if !safe_name(file) || !file.ends_with(".md") || file == "MEMORY.md" {
        return Err(format!("`{file}` is not a topic file name"));
    }
    let text = v.get("proposed_text").and_then(Value::as_str).unwrap_or("");
    if text.trim().is_empty() {
        return Err("a memory line carries the whole topic file as `proposed_text`".into());
    }
    let errors: Vec<String> = checks::topic(checks::Before::Absent, text, today).into_iter().filter(|f| f.severity == Severity::Error).map(|f| format!("{}: {}", f.check, f.message)).collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let index = v.get("index_line").and_then(Value::as_str).unwrap_or("").trim();
    if !checks::is_index_line(index) {
        return Err("a memory line carries its `MEMORY.md` line as `index_line` (`- [Title](file.md) — hook`)".into());
    }
    if index.chars().count() > checks::INDEX_MAX {
        return Err(format!("`index_line` is {} characters; the limit is {}", index.chars().count(), checks::INDEX_MAX));
    }
    Ok(())
}

/// The guard on the reply's lines: a broken one is dropped and counted, a line
/// naming something that does not exist is kept as `failed`.
fn guard(reply: &Value, ids: &HashSet<String>, set: &Set, today: chrono::NaiveDate) -> (Vec<Line>, usize) {
    let mut out = vec![];
    let mut dropped = 0;
    for raw in reply["lines"].as_array().into_iter().flatten() {
        let kind = str_of(raw, "kind");
        let text = str_of(raw, "text");
        let mut events: Vec<String> = vec![];
        for e in raw["events"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if ids.contains(e) && !events.iter().any(|x| x == e) {
                events.push(e.to_string());
            }
        }
        if !KINDS.contains(&kind) || text.is_empty() || events.is_empty() {
            dropped += 1;
            continue;
        }
        let target = if matches!(kind, "external" | "no-change") { Value::Null } else { raw["target"].clone() };
        let no_change = kind == "no-change";
        let mut v = Map::new();
        v.insert("kind".into(), json!(kind));
        v.insert("verdict".into(), json!(if no_change { "cut" } else { "approve" }));
        v.insert("note".into(), json!(""));
        v.insert("target".into(), target);
        v.insert("change".into(), json!(str_of(raw, "change")));
        v.insert("text".into(), json!(text));
        v.insert("count".into(), json!(events.len()));
        v.insert("events".into(), json!(events));
        v.insert("carried_from".into(), Value::Null);
        v.insert("proposed".into(), Value::Null);
        v.insert("check".into(), Value::Null);
        v.insert("status".into(), json!(if no_change { "cut" } else { "open" }));
        v.insert("error".into(), Value::Null);
        v.insert("commit".into(), Value::Null);
        if kind == "memory" {
            v.insert("proposed_text".into(), json!(raw.get("proposed_text").and_then(Value::as_str).unwrap_or("")));
            v.insert("index_line".into(), json!(str_of(raw, "index_line")));
        }
        let mut line = Line { v };
        let verdict = match kind {
            "pattern" | "new-pattern" => check_pattern_target(kind, &line.v["target"], set),
            "memory" => check_memory(&line.v, today),
            _ => Ok(()),
        };
        if let Err(e) = verdict {
            line.fail(e);
        }
        out.push(line);
    }
    (out, dropped)
}

// ---- checking the proposed patterns ----

struct Checked {
    lint: &'static str,
    examples: &'static str,
    error: Option<String>,
}

fn examples_text(show: &[String], name: &str) -> String {
    let head = format!("######## {name} example ");
    let mine: Vec<&String> = show.iter().filter(|s| s.starts_with(&head)).collect();
    if mine.is_empty() {
        "(no examples)\n".to_string()
    } else {
        mine.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// `promptu lint` on the set with the proposed file in it, and the examples of
/// the old and the new file rendered into `examples/` (docs/contract.md §10 `check`).
fn check_proposed(name: &str, text: &str, set: &Set, week_dir: &Path, today: &str, parts: &[String]) -> Result<Checked, String> {
    let path = week_dir.join("proposed").join(format!("{name}.md"));
    state::write_atomic(&path, text)?;
    let fail = |error: String| Checked { lint: "failed", examples: "failed", error: Some(error) };
    let parsed = match pattern::parse(&path.to_string_lossy(), text) {
        Ok(p) => p,
        Err(e) => return Ok(fail(format!("the proposed file does not parse: {e}"))),
    };
    if parsed.name() != name {
        return Ok(fail(format!("the proposed file is named `{}`, not `{name}`", parsed.name())));
    }
    if parsed.is_fragment() {
        return Ok(fail("the proposed file is a fragment; fragments are never changed here".into()));
    }
    let mut proposed = set.clone();
    proposed.patterns.insert(name.to_string(), parsed);
    let (report, show) = lint::lint(&proposed, None, today, parts);
    let errors: Vec<&lint::Finding> = report.files.iter().filter(|f| f.name.as_deref() == Some(name)).flat_map(|f| &f.findings).filter(|f| f.severity == "error").collect();
    let ex = week_dir.join("examples");
    state::write_atomic(&ex.join(format!("{name}.new.md")), &examples_text(&show, name))?;
    if set.patterns.contains_key(name) {
        let (_, old) = lint::lint(set, None, today, parts);
        state::write_atomic(&ex.join(format!("{name}.old.md")), &examples_text(&old, name))?;
    }
    let example_error = errors.iter().any(|f| matches!(f.check, "example" | "footer"));
    Ok(Checked {
        lint: if errors.is_empty() { "ok" } else { "failed" },
        examples: if example_error { "failed" } else { "ok" },
        error: (!errors.is_empty()).then(|| errors.iter().map(|f| format!("{}: {}", f.check, f.message)).collect::<Vec<_>>().join("; ")),
    })
}

/// Give each pattern-changing line its proposed file and its checks.
fn check_patterns(lines: &mut [Line], files: &Value, set: &Set, week_dir: &Path, today: &str) -> Result<(), String> {
    let parts: Vec<String> = pattern::DETECTED.iter().map(|p| p.to_string()).collect();
    let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, l) in lines.iter().enumerate() {
        if let (true, Some(n)) = (l.open(), l.pattern()) {
            by_name.entry(n.to_string()).or_default().push(i);
        }
    }
    for (name, idx) in by_name {
        if !safe_name(&name) {
            for i in idx {
                lines[i].fail(format!("`{name}` is not a pattern name"));
            }
            continue;
        }
        let Some(text) = files.get(&name).and_then(Value::as_str).filter(|t| !t.trim().is_empty()) else {
            for i in idx {
                lines[i].fail(format!("the reply has no proposed file for `{name}`"));
            }
            continue;
        };
        let c = check_proposed(&name, text, set, week_dir, today, &parts)?;
        for i in idx {
            lines[i].v.insert("proposed".into(), json!(format!("proposed/{name}.md")));
            lines[i].v.insert("check".into(), json!({ "lint": c.lint, "examples": c.examples, "replay": [] }));
            if let Some(e) = &c.error {
                lines[i].fail(e.clone());
            }
        }
    }
    Ok(())
}

/// A memory line's whole topic file under `proposed/memory/<project>/`.
fn write_memory(lines: &mut [Line], week_dir: &Path) -> Result<(), String> {
    for l in lines.iter_mut().filter(|l| l.kind() == "memory") {
        let (project, file) = (str_of(&l.v["target"], "project").to_string(), str_of(&l.v["target"], "file").to_string());
        if !(safe_name(&project) && safe_name(&file)) {
            continue;
        }
        let rel = format!("proposed/memory/{project}/{file}");
        state::write_atomic(&week_dir.join(&rel), l.v["proposed_text"].as_str().unwrap_or(""))?;
        l.v.insert("proposed".into(), json!(rel));
    }
    Ok(())
}

// ---- the sheet ----

/// The group a line belongs to: one per changed pattern, then one per other kind.
fn group_key(l: &Line) -> String {
    match l.pattern() {
        Some(n) => format!("{}:{n}", l.kind()),
        None => l.kind().to_string(),
    }
}

fn group_title(key: &str, lines: &[&Line]) -> String {
    let n: u64 = lines.iter().map(|l| l.v["count"].as_u64().unwrap_or(0)).sum();
    match key.split_once(':') {
        Some((_, name)) => {
            let mut kinds: Vec<(String, u64)> = vec![];
            for l in lines {
                let k = l.v["target"]["kind"].as_str().unwrap_or("change").replace('_', " ");
                let c = l.v["count"].as_u64().unwrap_or(0);
                match kinds.iter_mut().find(|(x, _)| *x == k) {
                    Some(e) => e.1 += c,
                    None => kinds.push((k, c)),
                }
            }
            format!("{name} — {n}: {}", kinds.iter().map(|(k, c)| format!("{c} {k}")).collect::<Vec<_>>().join(", "))
        }
        None => format!("{} — {n}", key.replace('-', " ")),
    }
}

/// What each line's target and change were, for keeping the owner's marks over a second draft.
fn mark_key(l: &Value) -> String {
    format!("{}|{}|{}", l["kind"], l["target"], l["change"])
}

fn marks(old: &Value) -> HashMap<String, (String, String)> {
    old["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["lines"].as_array().into_iter().flatten())
        .map(|l| (mark_key(l), (l["verdict"].as_str().unwrap_or("approve").to_string(), l["note"].as_str().unwrap_or("").to_string())))
        .collect()
}

/// Decision 15's checks as the pack's run events measure them. A number the
/// pack cannot compute is left out, never zero; the friction numbers wait for
/// the session events (EMB-274).
fn numbers(events: &[Value], previous: Option<&Value>) -> Vec<Value> {
    let runs: Vec<&Value> = events.iter().filter(|e| e["kind"] == "run").collect();
    let count = |k: &str, e: &Value| e[k].as_u64().unwrap_or(0) as f64;
    let filled: f64 = runs.iter().map(|e| count("filled", e)).sum();
    let changed: f64 = runs.iter().map(|e| e["changed_fills"].as_array().map_or(0, Vec::len) as f64).sum();
    let patterned: Vec<&&Value> = runs.iter().filter(|e| !e["pattern"].is_null()).collect();
    let picked = patterned.iter().filter(|e| e["picked_by_owner"] == true).count() as f64;
    let all: Vec<(&str, Option<f64>, &str)> = vec![
        ("fills_kept", (filled > 0.0).then(|| 1.0 - changed / filled), ">= 0.90"),
        ("patterns_kept", (!patterned.is_empty()).then(|| 1.0 - picked / patterned.len() as f64), ">= 0.85"),
        ("questions_per_run", (!runs.is_empty()).then(|| runs.iter().map(|e| count("asked", e)).sum::<f64>() / runs.len() as f64), "falling"),
    ];
    let round = |x: f64| (x * 100.0).round() / 100.0;
    all.into_iter()
        .filter_map(|(check, value, target)| {
            let baseline = previous.and_then(|p| p["numbers"].as_array()).and_then(|a| a.iter().find(|n| n["check"] == check)).and_then(|n| n["value"].as_f64());
            value.map(|v| json!({ "check": check, "value": round(v), "baseline": baseline, "target": target }))
        })
        .collect()
}

/// `sheet.md`: a read-only view of the sheet, one `- [x]` line per proposal (decision 30).
pub fn sheet_md(sheet: &Value) -> String {
    let n = sheet["groups"].as_array().into_iter().flatten().flat_map(|g| g["lines"].as_array().into_iter().flatten()).count();
    let mut out = format!(
        "# Promptu sheet {}\n\nGenerated {}, {n} line(s). Read-only: mark lines with `promptu week mark`; editing this file changes nothing.\n",
        sheet["week"].as_str().unwrap_or(""),
        sheet["generated"].as_str().unwrap_or("")
    );
    for nr in sheet["numbers"].as_array().into_iter().flatten() {
        out.push_str(&format!("\n- {}: {} (baseline {}, target {})", nr["check"].as_str().unwrap_or(""), nr["value"], nr["baseline"], nr["target"].as_str().unwrap_or("")));
    }
    for g in sheet["groups"].as_array().into_iter().flatten() {
        out.push_str(&format!("\n## {} {}\n\n", g["id"].as_str().unwrap_or(""), g["title"].as_str().unwrap_or("")));
        for l in g["lines"].as_array().into_iter().flatten() {
            let ticked = l["verdict"] == "approve" && l["status"] != "cut";
            let events: Vec<&str> = l["events"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let mut s = format!("- [{}] {} {}", if ticked { "x" } else { " " }, l["id"].as_str().unwrap_or(""), l["text"].as_str().unwrap_or(""));
            if l["count"].as_u64().unwrap_or(1) > 1 {
                s.push_str(&format!(" (x{})", l["count"]));
            }
            s.push_str(&format!(" [{}]", events.join(", ")));
            if l["status"] != "open" {
                s.push_str(&format!(" — {}", l["status"].as_str().unwrap_or("")));
            }
            if let Some(e) = l["error"].as_str() {
                s.push_str(&format!(": {e}"));
            }
            out.push_str(&s);
            out.push('\n');
            if let Some(p) = l["proposed"].as_str() {
                out.push_str(&format!("  - proposed: {p}\n"));
            }
        }
    }
    out
}

/// The sheet from the guarded lines: grouped, numbered, with every event held.
fn build_sheet(week: &str, events: &[Value], mut lines: Vec<Line>, old: Option<&Value>, supersedes: Option<String>, previous: Option<&Value>) -> Value {
    let held: HashSet<&str> = lines.iter().flat_map(|l| l.v["events"].as_array().into_iter().flatten().filter_map(Value::as_str)).collect();
    let missing: Vec<String> = events.iter().filter_map(|e| e["id"].as_str()).filter(|id| !held.contains(id)).map(str::to_string).collect();
    if !missing.is_empty() {
        let mut v = Map::new();
        for (k, x) in [("kind", json!("no-change")), ("verdict", json!("cut")), ("note", json!("")), ("target", Value::Null), ("change", json!("")), ("text", json!(NOT_ADDRESSED)), ("count", json!(missing.len())), ("events", json!(missing)), ("carried_from", Value::Null), ("proposed", Value::Null), ("check", Value::Null), ("status", json!("cut")), ("error", Value::Null), ("commit", Value::Null)] {
            v.insert(k.into(), x);
        }
        lines.push(Line { v });
    }
    if let Some(old) = old {
        let kept = marks(old);
        for l in lines.iter_mut().filter(|l| l.open() && l.kind() != "no-change") {
            if let Some((verdict, note)) = kept.get(&mark_key(&Value::Object(l.v.clone()))) {
                l.v.insert("verdict".into(), json!(verdict));
                l.v.insert("note".into(), json!(note));
                if verdict == "cut" {
                    l.v.insert("status".into(), json!("cut"));
                }
            }
        }
    }
    let mut order: Vec<String> = vec![];
    for l in &lines {
        let k = group_key(l);
        if !order.contains(&k) {
            order.push(k);
        }
    }
    let (mut gn, mut ln) = (0, 0);
    let mut groups = vec![];
    for key in order {
        let members: Vec<&Line> = lines.iter().filter(|l| group_key(l) == key).collect();
        gn += 1;
        let out: Vec<Value> = members
            .iter()
            .map(|l| {
                ln += 1;
                let mut v = l.v.clone();
                let mut with_id = Map::new();
                with_id.insert("id".into(), json!(format!("l{ln}")));
                with_id.append(&mut v);
                Value::Object(with_id)
            })
            .collect();
        let pattern = members[0].pattern().map(str::to_string);
        groups.push(json!({
            "id": format!("g{gn}"), "kind": members[0].kind(), "pattern": pattern, "title": group_title(&key, &members),
            "cut": members.iter().all(|l| l.v["verdict"] == "cut"), "lines": out
        }));
    }
    json!({
        "promptu": 1, "week": week, "generated": chrono::Local::now().to_rfc3339(), "feel": old.map_or(Value::Null, |o| o["feel"].clone()),
        "superseded_by": null, "supersedes": supersedes, "numbers": numbers(events, previous), "groups": groups,
    })
}

// ---- the command ----

fn write_run(dir: &Path, doc: &Value) -> Result<(), Fail> {
    state::write_atomic(&dir.join("run.json"), &serde_json::to_string_pretty(doc).map_err(|e| internal(e.to_string()))?).map_err(internal)
}

fn call_for(table: &toml::Table) -> Call {
    Call {
        claude: std::env::var("PROMPTU_CLAUDE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "claude".into()),
        model: config::week_model(table),
        effort: config::week_effort(table).to_string(),
        timeout: config::week_timeout(table),
        cwd: state::pass_dir().unwrap_or_else(|_| std::env::temp_dir()),
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Write the sheet: the drafter's reply guarded, checked and written. Returns the output object (§14.4).
pub fn draft(args: &Args) -> Result<Value, Fail> {
    let table = config::table();
    let week = match (&args.week, &args.pack) {
        (Some(w), _) => w.clone(),
        (None, Some(p)) => read_json(p).and_then(|v| v["week"].as_str().map(str::to_string)).unwrap_or_else(|| crate::week::week_of(config::today())),
        (None, None) => crate::week::week_of(config::today()),
    };
    if !weekread::is_week_id(&week) || crate::week::week_monday(&week).is_none() {
        return Err(usage(format!("`{week}` is not a week like 2026-W48")));
    }
    let weeks = state::dir().map_err(internal)?.join("week");
    let dir = weeks.join(&week);

    // The pack: `--pack`, the week's own, or one built now.
    let pack_path = match &args.pack {
        Some(p) => {
            if !p.is_file() {
                return Err(usage(format!("{}: no such pack", p.display())));
            }
            p.clone()
        }
        None => {
            let own = dir.join("pack.json");
            if !own.is_file() {
                crate::week::pack(&crate::week::Args { week: Some(week.clone()), projects: None })?;
            }
            own
        }
    };
    let pack = read_json(&pack_path).ok_or_else(|| usage(format!("{}: not a pack", pack_path.display())))?;
    let events: Vec<Value> = pack["events"].as_array().cloned().unwrap_or_default();
    let ids: HashSet<String> = events.iter().filter_map(|e| e["id"].as_str().map(str::to_string)).collect();
    let set = Set::load().map_err(usage)?;
    let today = config::today();
    let call = call_for(&table);
    let started = chrono::Local::now().to_rfc3339();

    let mut run = json!({
        "promptu": 1, "week": week, "kind": "manual", "scheduled_for": null, "started": started, "finished": null,
        "status": "running", "error": null, "model": call.model, "events": events.len(), "lines": 0, "dropped": 0,
    });
    let failed = |run: &mut Value, dir: &Path, msg: String| -> Fail {
        run["status"] = json!("failed");
        run["finished"] = json!(chrono::Local::now().to_rfc3339());
        run["error"] = json!(msg.split_whitespace().collect::<Vec<_>>().join(" "));
        let _ = write_run(dir, run);
        drafter_failed(msg)
    };

    // No events: a sheet with no lines, and no model call.
    let (lines, dropped, cost) = if events.is_empty() {
        (vec![], 0, None)
    } else {
        let reply = match pass::call_model(&call, SYSTEM, &message(&week, &pack, &set)) {
            Ok(r) => r,
            Err(e) => return Err(failed(&mut run, &dir, e)),
        };
        let Some(v) = pass::first_json_object(&reply.result).filter(|v| v["lines"].is_array()) else {
            return Err(failed(&mut run, &dir, "the reply held no JSON object with `lines`".into()));
        };
        let (mut lines, dropped) = guard(&v, &ids, &set, today);
        check_patterns(&mut lines, &v["patterns"], &set, &dir, &today.to_string()).map_err(internal)?;
        write_memory(&mut lines, &dir).map_err(internal)?;
        (lines, dropped, reply.cost_usd)
    };

    // The earlier open sheet: a second draft of one week replaces it and keeps
    // the owner's marks; an earlier week's is superseded (decision 46).
    let open = weekread::open_sheet().map_err(|e| internal(e.msg))?;
    let (same, earlier) = match open {
        Some((w, s)) if w == week => (Some(s), None),
        Some((w, s)) if w < week => (None, Some((w, s))),
        _ => (None, None),
    };
    let previous = earlier.as_ref().map(|(_, s)| s);
    let supersedes = earlier.as_ref().map(|(w, _)| w.clone()).or_else(|| same.as_ref().and_then(|s| s["supersedes"].as_str().map(str::to_string)));
    let sheet = build_sheet(&week, &events, lines, same.as_ref(), supersedes.clone(), previous);
    let line_count = sheet["groups"].as_array().into_iter().flatten().map(|g| g["lines"].as_array().map_or(0, Vec::len)).sum::<usize>();

    state::write_atomic(&dir.join("sheet.md"), &sheet_md(&sheet)).map_err(internal)?;
    state::write_atomic(&dir.join("sheet.json"), &serde_json::to_string_pretty(&sheet).map_err(|e| internal(e.to_string()))?).map_err(internal)?;
    if let Some((w, mut old)) = earlier {
        old["superseded_by"] = json!(week);
        let text = serde_json::to_string_pretty(&old).map_err(|e| internal(e.to_string()))?;
        state::write_atomic(&weeks.join(&w).join("sheet.json"), &text).map_err(internal)?;
    }
    run["status"] = json!("ok");
    run["finished"] = json!(chrono::Local::now().to_rfc3339());
    run["lines"] = json!(line_count);
    run["dropped"] = json!(dropped);
    if let Some(c) = cost {
        run["cost_usd"] = json!(c);
    }
    write_run(&dir, &run)?;
    Ok(json!({
        "promptu": 1, "week": week, "status": "ok", "sheet": dir.join("sheet.json").to_string_lossy(),
        "lines": line_count, "dropped": dropped, "supersedes": supersedes,
    }))
}

/// The text view of `draft`'s output.
pub fn text(v: &Value) -> String {
    let mut s = format!("week {}: sheet written, {} line(s)", v["week"].as_str().unwrap_or(""), v["lines"]);
    if v["dropped"].as_u64().unwrap_or(0) > 0 {
        s.push_str(&format!(", {} dropped", v["dropped"]));
    }
    if let Some(w) = v["supersedes"].as_str() {
        s.push_str(&format!(", supersedes {w}"));
    }
    s.push_str(&format!("\n{}\n", v["sheet"].as_str().unwrap_or("")));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_ids(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_broken_line_is_dropped_and_an_unknown_target_fails() {
        let set = Set::embedded();
        let reply = json!({ "lines": [
            { "kind": "no-change", "events": ["e1", "e99"], "text": "one-off" },
            { "kind": "bogus", "events": ["e1"], "text": "x" },
            { "kind": "no-change", "events": ["e99"], "text": "x" },
            { "kind": "no-change", "events": ["e1"], "text": "" },
            { "kind": "pattern", "events": ["e2"], "text": "x", "change": "c", "target": { "kind": "question", "pattern": "nope", "question": "q" } },
            { "kind": "pattern", "events": ["e2"], "text": "x", "change": "c", "target": { "kind": "question", "pattern": "handoff", "question": "nope" } },
        ]});
        let (lines, dropped) = guard(&reply, &pack_ids(&["e1", "e2"]), &set, chrono::NaiveDate::from_ymd_opt(2026, 11, 25).unwrap());
        assert_eq!(dropped, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].v["events"], json!(["e1"]));
        assert_eq!((lines[0].v["verdict"].as_str(), lines[0].v["status"].as_str()), (Some("cut"), Some("cut")));
        assert_eq!(lines[1].v["error"], "no pattern `nope`");
        assert_eq!(lines[2].v["error"], "pattern `handoff` has no question `nope`");
        assert_eq!(lines[2].v["status"], "failed");
    }

    #[test]
    fn a_file_name_is_never_a_path() {
        for bad in ["", "../x.md", "a/b.md", ".hidden", "a b.md"] {
            assert!(!safe_name(bad), "{bad}");
        }
        assert!(safe_name("flash-flags.md") && slug("my-pattern") && !slug("My") && !slug("_frag"));
    }

    #[test]
    fn numbers_leave_out_what_the_pack_cannot_compute() {
        assert!(numbers(&[], None).is_empty());
        let ev = vec![
            json!({"kind": "run", "pattern": "a", "asked": 2, "filled": 4, "changed_fills": ["x"], "picked_by_owner": false}),
            json!({"kind": "run", "pattern": "a", "asked": 4, "filled": 0, "changed_fills": [], "picked_by_owner": true}),
        ];
        let n = numbers(&ev, None);
        let get = |c: &str| n.iter().find(|x| x["check"] == c).map(|x| x["value"].as_f64().unwrap());
        assert_eq!((get("fills_kept"), get("patterns_kept"), get("questions_per_run")), (Some(0.75), Some(0.5), Some(3.0)));
    }
}
