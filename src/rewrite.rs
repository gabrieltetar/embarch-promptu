//! The rewrite (decision 42, docs/contract.md §8.4): after the last question,
//! one more model call turns the rough prompt, the answers and the structured
//! plan into the prompt that lands in the draft. The original stays in the log
//! (`state::LogEntry`). A pasted brief, a run with no pattern, an offline run
//! and `rewrite = "off"` are never rewritten, and a failed call leaves the
//! structured render: the rewrite never blocks the prompt.

use crate::footer::LEAD;
use crate::pass::{self, Call};
use crate::state::LogEntry;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const SYSTEM: &str = include_str!("rewrite_system.md");

/// The three tagged blocks the rewrite reads.
pub fn message(rough: &str, answers: &BTreeMap<String, String>, plan: &str) -> String {
    let lines: Vec<String> = answers.iter().filter(|(_, t)| !t.trim().is_empty()).map(|(q, t)| format!("{q}: {t}")).collect();
    let answers = if lines.is_empty() { "(none)".to_string() } else { lines.join("\n") };
    format!("<rough>\n{rough}\n</rough>\n\n<answers>\n{answers}\n</answers>\n\n<plan>\n{plan}\n</plan>\n")
}

/// The model's reply as a prompt: a whole-reply code fence and any footer line
/// the model added come off; `None` when nothing is left.
pub fn clean(reply: &str) -> Option<String> {
    let mut t = reply.trim();
    if t.starts_with("```") && t.ends_with("```") && t.len() > 6 {
        let inner = t.trim_start_matches('`');
        let inner = inner.split_once('\n').map(|(_, r)| r).unwrap_or(inner);
        t = inner.trim_end_matches('`').trim();
    }
    let kept: Vec<&str> = t.lines().filter(|l| !l.starts_with(LEAD)).collect();
    let out = kept.join("\n").trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// The rewrite call: the prompt text and the call's wall time in ms.
pub fn call(call: &Call, message: &str) -> Result<(String, u64), String> {
    let reply = pass::call_model(call, SYSTEM, message)?;
    let text = clean(&reply.result).ok_or("the rewrite came back empty")?;
    Ok((text, reply.ms as u64))
}

/// The value at percentile `p` (1 to 100), nearest rank.
pub fn percentile(sorted: &[u64], p: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (p * sorted.len()).div_ceil(100).clamp(1, sorted.len());
    Some(sorted[rank - 1])
}

fn spread(mut v: Vec<u64>) -> Value {
    v.sort_unstable();
    json!({ "n": v.len(), "p50": percentile(&v, 50), "p95": percentile(&v, 95), "max": v.last() })
}

/// `promptu stats`: the wait after the last question and the rewrite call's own
/// time over the logged runs that were rewritten, overall and per model.
pub fn stats(entries: &[LogEntry], model: Option<&str>) -> Value {
    let rewritten: Vec<&LogEntry> = entries.iter().filter(|e| e.rewritten && model.is_none_or(|m| e.rewrite_model.as_deref() == Some(m))).collect();
    let mut by_model: BTreeMap<String, Vec<&LogEntry>> = BTreeMap::new();
    for e in &rewritten {
        by_model.entry(e.rewrite_model.clone().unwrap_or_else(|| "unknown".into())).or_default().push(e);
    }
    let of = |es: &[&LogEntry]| json!({ "wait_ms": spread(es.iter().map(|e| e.wait_ms).collect()), "rewrite_ms": spread(es.iter().filter_map(|e| e.rewrite_ms).collect()) });
    let models: serde_json::Map<String, Value> = by_model.iter().map(|(m, es)| (m.clone(), of(es))).collect();
    let mut out = of(&rewritten);
    out["promptu"] = json!(1);
    out["runs"] = json!(entries.len());
    out["rewritten"] = json!(rewritten.len());
    out["models"] = Value::Object(models);
    out
}

pub fn stats_text(v: &Value) -> String {
    let cell = |s: &Value, k: &str| s[k].as_u64().map(|n| format!("{n} ms")).unwrap_or_else(|| "-".into());
    let row = |label: &str, v: &Value| {
        format!("{label}: n={} wait p50 {} p95 {}; rewrite p50 {} p95 {}", v["wait_ms"]["n"], cell(&v["wait_ms"], "p50"), cell(&v["wait_ms"], "p95"), cell(&v["rewrite_ms"], "p50"), cell(&v["rewrite_ms"], "p95"))
    };
    let mut s = format!("{} run(s) logged, {} rewritten\n{}\n", v["runs"], v["rewritten"], row("all", v));
    for (m, mv) in v["models"].as_object().into_iter().flatten() {
        s.push_str(&format!("{}\n", row(m, mv)));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fence_and_a_footer_come_off_the_reply() {
        assert_eq!(clean("```\n## Goal\nFix it.\n```").as_deref(), Some("## Goal\nFix it."));
        assert_eq!(clean("## Goal\nFix it.\n\n— promptu x@1").as_deref(), Some("## Goal\nFix it."));
        assert_eq!(clean("  \n"), None);
        assert_eq!(clean("— promptu x@1"), None);
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let v: Vec<u64> = (1..=10).collect();
        assert_eq!(percentile(&v, 50), Some(5));
        assert_eq!(percentile(&v, 95), Some(10));
        assert_eq!(percentile(&[7], 95), Some(7));
        assert_eq!(percentile(&[], 50), None);
    }
}
