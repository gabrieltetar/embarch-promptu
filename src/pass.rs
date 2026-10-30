//! The pass (docs/contract.md §7.3–§7.4): one stripped `claude -p` that
//! picks the pattern and fills what the sources already answer, and the
//! no-model guard that throws away any fill it cannot verify.
//!
//! What it sends: a fixed system prompt (`pass_system.md` plus the catalogue
//! generated from the pattern set: stable, so Claude Code's prompt cache
//! holds it) and one user message with the rough prompt and its sources,
//! each in its own tag.

use crate::pattern::{Pattern, Question, Set};
use crate::run::{Ambiguity, Dropped, Fill};
use regex::RegexBuilder;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const SYSTEM: &str = include_str!("pass_system.md");

fn q_line(q: &Question, parts: &[String]) -> String {
    let mut tags = vec![q.qtype.clone()];
    if q.required {
        tags.push("required".into());
    }
    if q.always_ask {
        tags.push("always_ask".into());
    }
    if q.suggest > 0 {
        tags.push(format!("suggest {}", q.suggest));
    }
    if let Some(w) = &q.when {
        tags.push(format!("when {w}"));
    }
    tags.push(format!("sources {}", q.sources.join(",")));
    let mut s = format!("- `{}` ({}): {}", q.id, tags.join("; "), q.ask);
    // Only the options that show with the run's EmbArch parts: the model never
    // sees a hidden label, so it cannot offer one back.
    let visible = q.visible_labels(parts);
    let opts: Vec<_> = q.effective_options().into_iter().filter(|o| visible.contains(&o.label)).collect();
    if !opts.is_empty() {
        let o: Vec<String> = opts.iter().map(|o| if o.description.is_empty() { o.label.clone() } else { format!("{} ({})", o.label, o.description) }).collect();
        s.push_str(&format!(" Options: {}", o.join(" | ")));
    }
    s
}

/// The catalogue: every task pattern, its description, phrases, example
/// prompts and questions (fragments' included), as they show with these
/// EmbArch parts: hidden questions and options are left out.
pub fn catalogue(set: &Set, parts: &[String]) -> String {
    let mut out = vec![];
    for (name, p) in &set.patterns {
        out.push(format!("## {name}"));
        out.push(p.front.description.split_whitespace().collect::<Vec<_>>().join(" "));
        if !p.front.signals.phrases.is_empty() {
            out.push(format!("Typical phrases: {}", p.front.signals.phrases.join(" · ")));
        }
        if !p.front.examples.is_empty() {
            out.push(format!("Example prompts: {}", p.front.examples.iter().map(|e| format!("\"{}\"", e.rough)).collect::<Vec<_>>().join(" · ")));
        }
        out.push("Questions:".into());
        for q in set.questions(p).iter().filter(|q| q.visible(parts)) {
            out.push(q_line(q, parts));
        }
        out.push(String::new());
    }
    format!("{}\n", out.join("\n").trim_end())
}

pub fn system_prompt(set: &Set, parts: &[String]) -> String {
    format!("{}\n\n{}", SYSTEM.trim_end(), catalogue(set, parts))
}

/// The no-model pre-pick: each `signals.phrases` hit +1, each `signals.not`
/// hit −1, case-insensitive. Patterns that score above 0, highest first.
pub fn regex_scores(set: &Set, text: &str) -> BTreeMap<String, i64> {
    let hit = |rx: &str| RegexBuilder::new(rx).case_insensitive(true).build().map(|r| r.is_match(text)).unwrap_or(false);
    let mut out = BTreeMap::new();
    for (name, p) in &set.patterns {
        let s = p.front.signals.phrases.iter().filter(|r| hit(r)).count() as i64 - p.front.signals.not.iter().filter(|r| hit(r)).count() as i64;
        if s > 0 {
            out.insert(name.clone(), s);
        }
    }
    out
}

pub fn regex_top(scores: &BTreeMap<String, i64>) -> Option<String> {
    let mut v: Vec<(&String, &i64)> = scores.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    v.first().map(|(n, _)| (*n).clone())
}

/// The sources as text, the way the model sees them and the guard checks them.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub prompt: String,
    pub desk: String,
    /// The memory block: index lines, each opening with `[<id>] `.
    pub memory: String,
    /// Memory lines by id (a `CLAUDE.md` line is `claude-md`).
    pub memory_by_id: BTreeMap<String, Vec<String>>,
    pub last: String,
    pub context: String,
}

fn block(tag: &str, body: &str) -> String {
    let b = body.trim();
    format!("<{tag}>\n{}\n</{tag}>", if b.is_empty() { "(none)" } else { b })
}

pub fn user_message(src: &Sources, scores: &BTreeMap<String, i64>) -> String {
    let mut ranked: Vec<(&String, &i64)> = scores.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let hint = if ranked.is_empty() { "(no hits)".to_string() } else { ranked.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ") };
    [
        block("prompt", &src.prompt),
        block("context", &src.context),
        block("desk", &src.desk),
        block("memory", &src.memory),
        block("last", &src.last),
        format!("<regex_hint>\n{hint}\n</regex_hint>"),
    ]
    .join("\n")
        + "\n"
}

/// How to call the model.
#[derive(Debug, Clone)]
pub struct Call {
    pub claude: String,
    pub model: String,
    pub effort: String,
    pub timeout: Duration,
    pub cwd: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub result: String,
    pub cost_usd: Option<f64>,
    pub ms: u128,
}

/// The decided flags (SYNTHESIS §3), the effort from config, the prompt on
/// stdin. `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` drops a side call
/// Claude Code makes on every `-p` [measured 2026-10-29: $0.0033 a run,
/// caching unaffected]; `EMBARCH_DESK_INTERNAL` keeps the desk's hooks off it.
pub fn argv(call: &Call, system: &str) -> Vec<String> {
    [
        "-p", "--model", &call.model, "--effort", &call.effort, "--no-session-persistence", "--output-format", "json",
        "--strict-mcp-config", "--setting-sources", "", "--tools", "", "--system-prompt", system,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// How long stdout may stay open after the child exits (a grandchild can
/// hold it): what arrived by then is the reply.
const DRAIN_GRACE: Duration = Duration::from_secs(1);

pub fn call_model(call: &Call, system: &str, message: &str) -> Result<Reply, String> {
    let t0 = Instant::now();
    // stderr is not read, so it goes nowhere: a full pipe would stall the child.
    let mut child = Command::new(&call.claude)
        .args(argv(call, system))
        .current_dir(&call.cwd)
        .env("EMBARCH_DESK_INTERNAL", "1")
        .env("PROMPTU_INTERNAL", "1")
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not run `{}`: {e}", call.claude))?;
    // Neither thread is ever joined: a child that never reads stdin or a
    // grandchild that keeps stdout open must not hold this process.
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let msg = message.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(msg.as_bytes());
    });
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut out: Vec<u8> = vec![];
    loop {
        while let Ok(chunk) = rx.try_recv() {
            out.extend(chunk);
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if t0.elapsed() > call.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the pass took longer than {} s", call.timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
    // The child is gone: take the rest until end of file, or until the grace runs out.
    let deadline = Instant::now() + DRAIN_GRACE;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(chunk) => out.extend(chunk),
            Err(_) => break,
        }
    }
    let out = String::from_utf8_lossy(&out);
    let v: Value = serde_json::from_str(out.trim()).map_err(|_| format!("`{}` printed no JSON", call.claude))?;
    if v["is_error"].as_bool() == Some(true) || v["terminal_reason"].as_str().is_some_and(|r| r != "completed") {
        return Err(format!("the pass failed: {}", v["result"].as_str().or(v["terminal_reason"].as_str()).unwrap_or("unknown error")));
    }
    let result = v["result"].as_str().ok_or("the pass returned no result")?.to_string();
    Ok(Reply { result, cost_usd: v["total_cost_usd"].as_f64(), ms: t0.elapsed().as_millis() })
}

/// The first JSON object in a reply, fenced or not.
pub fn first_json_object(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, c) in text[start..].char_indices() {
        if in_str {
            match (esc, c) {
                (true, _) => esc = false,
                (false, '\\') => esc = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..start + i + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// Case-, quote-, dash- and whitespace-folded, for the evidence check.
pub fn fold(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            c => c,
        })
        .collect();
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `quote` occurs in `text` (both folded) on word boundaries: a quote that
/// starts or ends with a letter or digit may not start or end inside a word.
pub fn quoted_in(text: &str, quote: &str) -> bool {
    if quote.is_empty() {
        return false;
    }
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    let (first, last) = (quote.chars().next(), quote.chars().last());
    text.match_indices(quote).any(|(i, _)| {
        let before = text[..i].chars().last();
        let after = text[i + quote.len()..].chars().next();
        !(word(first) && word(before)) && !(word(last) && word(after))
    })
}

/// What the guard keeps of the model's reply.
#[derive(Debug, Clone, Default)]
pub struct Guarded {
    pub pattern: Option<String>,
    pub confidence: Option<f64>,
    pub alt: Vec<String>,
    pub hardware: bool,
    pub firmware: bool,
    pub brief: bool,
    pub fills: BTreeMap<String, Fill>,
    pub dropped: Vec<Dropped>,
    pub candidates: BTreeMap<String, Vec<String>>,
    pub relevant: BTreeMap<String, bool>,
    pub ambiguous: Option<Ambiguity>,
}

/// A fill's value as this question takes it: one of its labels (`choice`,
/// `bool`), its labels (`multi`), non-empty text (`text`), counting only the
/// options that show with these EmbArch parts; `None` otherwise.
pub fn fit_value(q: &Question, v: &Value, parts: &[String]) -> Option<Value> {
    // Only the options that show with these EmbArch parts.
    let labels = q.visible_labels(parts);
    match (q.qtype.as_str(), v) {
        ("choice", Value::String(l)) if labels.contains(l) => Some(Value::String(l.clone())),
        ("bool", Value::Bool(b)) => Some(Value::String(if *b { "Yes".into() } else { "No".into() })),
        ("bool", Value::String(l)) if labels.contains(l) => Some(Value::String(l.clone())),
        ("multi", Value::Array(a)) if !a.is_empty() && a.iter().all(|x| x.as_str().is_some_and(|l| labels.iter().any(|y| y == l))) => Some(Value::Array(a.clone())),
        ("multi", Value::String(l)) if labels.contains(l) => Some(Value::Array(vec![Value::String(l.clone())])),
        // Text equal to a hidden option's label would map onto that option.
        ("text", Value::String(t)) if !t.trim().is_empty() && !q.effective_options().iter().any(|o| !labels.contains(&o.label) && o.label.eq_ignore_ascii_case(t.trim())) => {
            Some(Value::String(t.trim().to_string()))
        }
        _ => None,
    }
}

/// Check one fill; `Err` says why it is dropped.
pub fn check_fill(q: &Question, f: &Value, src: &Sources, parts: &[String]) -> Result<Fill, String> {
    let source = f["source"].as_str().unwrap_or_default().to_string();
    if let Some(r) = q.requires.as_ref().filter(|r| !parts.contains(r)) {
        return Err(format!("needs {r}, which this session does not have"));
    }
    if !q.sources.contains(&source) {
        return Err(format!("source `{source}` is not one this question takes"));
    }
    if q.always_ask && source != "prompt" {
        return Err("asked every run: only the prompt may pre-select it".into());
    }
    let evidence = f["evidence"].as_str().unwrap_or_default().trim().to_string();
    let ev = fold(evidence.trim_matches(|c: char| c == '"' || c == '\'' || c == '.' || c.is_whitespace()));
    if ev.split(' ').count() < 3 && ev.chars().count() < 12 {
        return Err("the evidence is too short to check".into());
    }
    let mut entry = f["entry"].as_str().filter(|e| !e.is_empty()).map(str::to_string);
    let haystack = match source.as_str() {
        "prompt" => src.prompt.clone(),
        "desk" => src.desk.clone(),
        "last" => src.last.clone(),
        "context" => src.context.clone(),
        "memory" => {
            match &entry {
                Some(e) => src.memory_by_id.get(e).map(|l| l.join("\n")).ok_or_else(|| format!("no memory entry `{e}`"))?,
                None => {
                    // No id given: the one entry whose line holds the quote.
                    let hits: Vec<&String> = src.memory_by_id.iter().filter(|(_, ls)| ls.iter().any(|l| quoted_in(&fold(l), &ev))).map(|(k, _)| k).collect();
                    match hits.as_slice() {
                        [one] => {
                            entry = Some((*one).clone());
                            src.memory_by_id[*one].join("\n")
                        }
                        _ => return Err("a memory fill names no entry".into()),
                    }
                }
            }
        }
        _ => return Err(format!("unknown source `{source}`")),
    };
    if !quoted_in(&fold(&haystack), &ev) {
        return Err("the evidence is not in the source".into());
    }
    let value = fit_value(q, &f["value"], parts).ok_or("the value is not one this question takes")?;
    if source == "memory" && !entry.as_deref().is_some_and(|e| e == "claude-md" || (e.len() == 6 && e.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))) {
        return Err("the memory entry id is malformed".into());
    }
    Ok(Fill { value, source, entry: if f["source"] == "memory" { entry } else { None }, evidence })
}

/// Guard the model's reply against the pattern set and the sources.
pub fn guard(v: &Value, set: &Set, src: &Sources, parts: &[String]) -> Guarded {
    let mut g = Guarded::default();
    let pick = v["pattern"].as_str().unwrap_or("none");
    let p: Option<&Pattern> = set.get(pick);
    g.pattern = p.map(|p| p.name().to_string());
    g.confidence = v["confidence"].as_f64().map(|c| c.clamp(0.0, 1.0));
    g.alt = v["alt"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter(|n| set.get(n).is_some() && *n != pick).take(2).map(str::to_string).collect();
    g.hardware = v["flags"]["hardware"].as_bool().unwrap_or(false);
    g.firmware = v["flags"]["firmware"].as_bool().unwrap_or(false);
    g.brief = v["flags"]["brief"].as_bool().unwrap_or(false);
    let Some(p) = p else { return g };
    let qs = set.questions(p);
    let q_of = |id: &str| qs.iter().find(|q| q.id == id);
    for (id, f) in v["filled"].as_object().into_iter().flatten() {
        let source = f["source"].as_str().unwrap_or_default().to_string();
        let Some(q) = q_of(id) else {
            g.dropped.push(Dropped { q: id.clone(), source, reason: "not a question of this pattern".into() });
            continue;
        };
        match check_fill(q, f, src, parts) {
            Ok(fill) => {
                g.fills.insert(id.clone(), fill);
            }
            Err(reason) => {
                // A text answer from a source an always-asked question may not
                // pre-select still helps as a suggestion.
                if q.always_ask && q.qtype == "text" && q.sources.contains(&source) {
                    if let Some(t) = f["value"].as_str().filter(|t| !t.trim().is_empty()) {
                        g.candidates.entry(id.clone()).or_default().push(t.trim().to_string());
                    }
                }
                g.dropped.push(Dropped { q: id.clone(), source, reason });
            }
        }
    }
    for (id, cs) in v["candidates"].as_object().into_iter().flatten() {
        let Some(q) = q_of(id).filter(|q| q.qtype == "text") else { continue };
        let list = g.candidates.entry(id.clone()).or_default();
        for c in cs.as_array().into_iter().flatten().filter_map(|c| c.as_str()) {
            let c = c.trim().to_string();
            if !c.is_empty() && !list.contains(&c) {
                list.push(c);
            }
        }
        list.truncate((q.suggest as usize).min(3));
    }
    g.candidates.retain(|_, v| !v.is_empty());
    for (id, r) in v["relevant"].as_object().into_iter().flatten() {
        if let (Some(q), Some(b)) = (q_of(id), r.as_bool()) {
            if q.optional() && q.visible(parts) {
                g.relevant.insert(id.clone(), b);
            }
        }
    }
    let prompt_folded = fold(&src.prompt);
    g.ambiguous = v["ambiguous"].as_array().into_iter().flatten().find_map(|a| {
        let phrase = a["phrase"].as_str()?.trim().to_string();
        let meanings: Vec<String> = a["meanings"].as_array()?.iter().filter_map(|m| m.as_str().map(|s| s.trim().to_string())).filter(|m| !m.is_empty()).take(3).collect();
        (meanings.len() >= 2 && !phrase.is_empty() && prompt_folded.contains(&fold(&phrase))).then_some(Ambiguity { phrase, meanings })
    });
    g
}

/// A pasted, already-structured brief, seen without the model: long enough,
/// and with at least two structure markers (headings, label lines, the
/// usual brief sections, a numbered list).
pub fn looks_like_brief(text: &str) -> bool {
    let words = text.split_whitespace().count();
    if words < 150 {
        return false;
    }
    let mut markers = 0;
    let mut numbered = 0;
    for l in text.lines().map(str::trim) {
        let heading = l.starts_with('#') && l.trim_start_matches('#').starts_with(' ');
        let bold = l.starts_with("**") && (l.ends_with("**") || l.ends_with("**:"));
        let label = l.ends_with(':') && l.len() <= 48 && l.chars().next().is_some_and(char::is_uppercase);
        if heading || bold || label {
            markers += 1;
        }
        if l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit())) {
            numbered += 1;
        }
    }
    let lower = text.to_lowercase();
    markers += ["read first", "definition of done", "done when", "hard rules", "order of work", "already decided"].iter().filter(|k| lower.contains(*k)).count();
    markers += (numbered >= 3) as usize;
    markers >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn src() -> Sources {
        let mut s = Sources { prompt: "Review the current branch, it adds the CAN watchdog. Commit locally.".into(), ..Default::default() };
        s.memory = "[56b97e] - [Example](example.md) — only I open PRs in client repos".into();
        s.memory_by_id.insert("56b97e".into(), vec![s.memory.clone()]);
        s
    }

    #[test]
    fn the_guard_keeps_a_quoted_fill_and_drops_the_rest() {
        let set = Set::embedded();
        let v = json!({"pattern": "review-loop", "confidence": 0.9, "alt": ["nope", "handoff"],
            "filled": {
                "target": {"value": "This branch", "source": "prompt", "evidence": "Review the current branch"},
                "rules": {"value": ["Only I open PRs"], "source": "memory", "evidence": "only I open PRs in client repos"},
                "lens": {"value": ["Correctness"], "source": "memory", "evidence": "only I open PRs in client repos"},
                "commits": {"value": "A local commit per round", "source": "prompt", "evidence": "commit every round locally"},
                "stop": {"value": "Whenever", "source": "prompt", "evidence": "Review the current branch"},
                "budget": {"value": "Normal", "source": "memory", "evidence": "only I open PRs"},
                "nosuch": {"value": "x", "source": "prompt", "evidence": "Review the current"}},
            "relevant": {"budget": true, "stop": true},
            "ambiguous": [{"phrase": "the CAN watchdog", "meanings": ["a", "b"]}]});
        let g = guard(&v, &set, &src(), &[]);
        assert_eq!(g.pattern.as_deref(), Some("review-loop"));
        assert_eq!(g.alt, vec!["handoff"]);
        assert_eq!(g.fills.keys().collect::<Vec<_>>(), vec!["rules", "target"]);
        assert_eq!(g.fills["rules"].entry.as_deref(), Some("56b97e"));
        let why: BTreeMap<_, _> = g.dropped.iter().map(|d| (d.q.as_str(), d.reason.as_str())).collect();
        assert_eq!(why["commits"], "the evidence is not in the source");
        assert_eq!(why["stop"], "the value is not one this question takes");
        assert!(why["budget"].starts_with("source `memory`"));
        assert!(why["lens"].starts_with("source `memory`"));
        assert_eq!(why["nosuch"], "not a question of this pattern");
        // `stop` is required, so it cannot be marked relevant.
        assert_eq!(g.relevant.keys().collect::<Vec<_>>(), vec!["budget"]);
        assert_eq!(g.ambiguous.unwrap().phrase, "the CAN watchdog");
    }

    #[test]
    fn an_always_asked_fill_from_memory_becomes_a_suggestion() {
        let set = Set::embedded();
        let mut s = src();
        s.last = "board: Motor board rev B".into();
        let v = json!({"pattern": "bench-session", "filled": {"board": {"value": "Motor board rev B", "source": "last", "evidence": "board: Motor board rev B"}}});
        let g = guard(&v, &set, &s, &[]);
        assert!(g.fills.is_empty());
        assert_eq!(g.candidates["board"], vec!["Motor board rev B"]);
    }

    #[test]
    fn evidence_is_three_words_or_twelve_characters_on_word_boundaries() {
        let set = Set::embedded();
        let p = set.get("review-loop").unwrap();
        let qs = set.questions(p);
        let target = qs.iter().find(|q| q.id == "target").unwrap();
        let s = Sources { prompt: "Respin the current branch; review it.".into(), ..Default::default() };
        let fill = |ev: &str| check_fill(target, &json!({"value": "This branch", "source": "prompt", "evidence": ev}), &s, &[]);
        assert_eq!(fill("the branch").unwrap_err(), "the evidence is too short to check");
        assert!(fill("current branch; review").is_ok(), "three words");
        assert!(fill("current branch").is_ok(), "fourteen characters");
        assert_eq!(fill("spin the current").unwrap_err(), "the evidence is not in the source", "starts inside a word");
        assert_eq!(fill("the current bran").unwrap_err(), "the evidence is not in the source", "ends inside a word");
        assert!(quoted_in("a (pr 12) b", "(pr 12)"));
        assert!(quoted_in("x cat, y", "cat,"));
        assert!(!quoted_in("concatenate", "cat"));
    }

    #[test]
    fn folding_and_json_extraction() {
        assert_eq!(fold("It’s  the\nPR — ok"), "it's the pr - ok");
        assert_eq!(first_json_object("```json\n{\"a\": \"}\"}\n```").unwrap()["a"], "}");
        assert!(first_json_object("no json").is_none());
    }

    #[test]
    fn a_brief_is_seen_by_its_structure() {
        let body = "word ".repeat(160);
        assert!(looks_like_brief(&format!("Context:\n{body}\n## Read first\n- a\n## Definition of done\nx")));
        assert!(!looks_like_brief(&body));
        assert!(!looks_like_brief("## Goal\nshort"));
    }

    #[test]
    fn the_catalogue_lists_every_pattern_with_its_fragment_questions() {
        let set = Set::embedded();
        let c = catalogue(&set, &crate::pattern::PARTS.map(String::from));
        assert_eq!(c.matches("\n## ").count() + c.starts_with("## ") as usize, 11);
        assert!(c.contains("- `rules` (multi; sources prompt,context,memory,last)"));
        assert!(system_prompt(&set, &[]).starts_with("You are Promptu's pass."));
        // Without the desk, its question options are not in the catalogue.
        assert!(c.contains("The issue and its PR"));
        assert!(!catalogue(&set, &[]).contains("The issue and its PR"));
        assert!(!catalogue(&set, &[]).contains("EmbArch ("));
    }

    #[test]
    fn a_text_fill_equal_to_a_hidden_label_is_refused() {
        let set = Set::embedded();
        let qs = set.questions(set.get("implement-issue").unwrap());
        let read = qs.iter().find(|q| q.id == "read").unwrap();
        assert!(fit_value(read, &json!("The issue and its PR"), &[]).is_none());
        assert!(fit_value(read, &json!("the issue and its PR"), &[]).is_none(), "any case");
        assert!(fit_value(read, &json!("docs/export.md"), &[]).is_some());
        let desk: Vec<String> = vec!["desk".into(), "item".into()];
        assert!(fit_value(read, &json!("The issue and its PR"), &desk).is_some());
    }
}
