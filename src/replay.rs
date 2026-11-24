//! The replay of a proposed pattern (decision 47, docs/contract.md §10 `check`,
//! forks W23, W24): the pass run on a few real past rough prompts of the
//! pattern, once with the old pattern file and once with the new, the two
//! results written side by side in the week folder.
//!
//! Only a change to a question, a signal or a source is replayed: a template-only
//! change renders without a model, so it has nothing a model could answer
//! differently. The prompts are the log's `original` and are read from the log
//! only; the files written stay under the week folder in the state folder.

use crate::pass::{self, Call};
use crate::pattern::{self, Set};
use crate::state::{self, LogEntry};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

/// How many past prompts one pattern is replayed on (decision 47: 3 to 5; forks W23).
pub const MAX_PROMPTS: usize = 5;

/// The week's replay budget in dollars (`week_replay_cap_usd`, forks W24).
pub struct Budget {
    pub cap_usd: f64,
    pub spent_usd: f64,
}

/// What one side of a replay came to.
struct Side {
    picked: String,
    confidence: String,
    asked: Vec<String>,
    filled: Vec<String>,
}

/// Whether a changed file differs in its frontmatter, where the questions, the
/// signals and the sources live; a body-only change is a template change.
pub fn front_changed(old: &str, new: &str) -> bool {
    let front = |t: &str| pattern::split(t).map(|(f, _)| f.trim().to_string());
    front(old) != front(new)
}

/// The past prompts to replay: the newest distinct `original`s of runs of the
/// pattern that came in as a rough prompt (not a pasted brief).
pub fn past_prompts<'a>(logs: &'a [LogEntry], name: &str) -> Vec<&'a LogEntry> {
    let mut seen = HashSet::new();
    let mut out: Vec<&LogEntry> = vec![];
    for l in logs.iter().rev() {
        if l.pattern.as_deref() == Some(name) && l.mode == "prompt" && !l.original.trim().is_empty() && seen.insert(l.original.trim().to_string()) {
            out.push(l);
            if out.len() == MAX_PROMPTS {
                break;
            }
        }
    }
    out
}

fn one_side(call: &Call, set: &Set, prompt: &str, parts: &[String]) -> Result<(Side, f64), String> {
    let src = pass::Sources { prompt: prompt.to_string(), ..Default::default() };
    let scores = pass::regex_scores(set, prompt);
    let reply = pass::call_model(call, &pass::system_prompt(set, parts), &pass::user_message(&src, &scores))?;
    let cost = reply.cost_usd.unwrap_or(0.0);
    let v = pass::first_json_object(&reply.result).ok_or("the reply held no JSON object")?;
    let g = pass::guard(&v, set, &src, parts);
    let (asked, picked) = match g.pattern.as_deref().and_then(|n| set.get(n)) {
        Some(p) => (set.questions(p).iter().filter(|q| q.visible(parts) && !g.fills.contains_key(&q.id)).map(|q| q.id.clone()).collect(), p.name().to_string()),
        None => (vec![], "none".to_string()),
    };
    let filled = g.fills.iter().map(|(id, f)| format!("{id} = {}", f.value)).collect();
    Ok((Side { picked, confidence: g.confidence.map_or_else(|| "none".to_string(), |c| c.to_string()), asked, filled }, cost))
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn cells(r: &Result<(Side, f64), String>, f: impl Fn(&Side) -> String + Copy) -> String {
    match r {
        Ok((s, _)) => cell(&f(s)),
        Err(e) => cell(&format!("failed: {e}")),
    }
}

fn list(v: &[String]) -> String {
    if v.is_empty() { "(none)".to_string() } else { v.join(", ") }
}

fn side_by_side(name: &str, run: &str, prompt: &str, old: &Result<(Side, f64), String>, new: &Result<(Side, f64), String>) -> String {
    let quoted: String = prompt.trim().lines().map(|l| format!("> {l}\n")).collect();
    let mut out = format!("# Replay of `{name}` on run {run}\n\nThe rough prompt, from the log (it stays in the state folder):\n\n{quoted}\n| | old pattern | new pattern |\n|---|---|---|\n");
    for label in ["picked", "confidence", "questions asked", "filled"] {
        let f = |s: &Side| match label {
            "picked" => s.picked.clone(),
            "confidence" => s.confidence.clone(),
            "questions asked" => format!("{}: {}", s.asked.len(), list(&s.asked)),
            _ => list(&s.filled),
        };
        out.push_str(&format!("| {label} | {} | {} |\n", cells(old, f), cells(new, f)));
    }
    out
}

/// Replay `name` on past prompts, old set against new. Returns `check`'s
/// `replay` array and a note: why nothing was replayed, or that the cap stopped it.
pub fn replay(name: &str, sets: (&Set, &Set), logs: &[LogEntry], call: &Call, parts: &[String], week_dir: &Path, budget: &mut Budget) -> Result<(Vec<Value>, Option<String>), String> {
    let (old, new) = sets;
    let prompts = past_prompts(logs, name);
    if prompts.is_empty() {
        return Ok((vec![], Some(format!("no past prompt of `{name}` in the log to replay"))));
    }
    let mut out = vec![];
    let mut note = None;
    for l in &prompts {
        if budget.spent_usd >= budget.cap_usd {
            note = Some(format!("stopped by the replay cap of ${:.2} a week after {} of {} prompts", budget.cap_usd, out.len(), prompts.len()));
            break;
        }
        let a = one_side(call, old, &l.original, parts);
        let b = one_side(call, new, &l.original, parts);
        budget.spent_usd += [&a, &b].iter().filter_map(|r| r.as_ref().ok()).map(|(_, c)| c).sum::<f64>();
        let rel = format!("replay/{name}/{}.md", l.run);
        state::write_atomic(&week_dir.join(&rel), &side_by_side(name, &l.run, &l.original, &a, &b))?;
        let n = |r: &Result<(Side, f64), String>| r.as_ref().ok().map(|(s, _)| s.asked.len());
        out.push(json!({ "run": l.run, "questions_before": n(&a), "questions_after": n(&b), "file": rel }));
    }
    Ok((out, note))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(run: &str, pattern: &str, mode: &str, original: &str) -> LogEntry {
        LogEntry { run: run.into(), pattern: Some(pattern.into()), mode: mode.into(), original: original.into(), ..Default::default() }
    }

    #[test]
    fn a_body_only_change_is_not_a_front_change() {
        let a = "---\nname: x\nversion: 1\n---\nbody\n";
        assert!(!front_changed(a, "---\nname: x\nversion: 1\n---\nother body\n"));
        assert!(front_changed(a, "---\nname: x\nversion: 2\n---\nbody\n"));
    }

    #[test]
    fn the_newest_distinct_prompts_of_the_pattern_up_to_five() {
        let mut logs = vec![log("r00", "a", "prompt", "same"), log("r01", "b", "prompt", "other pattern"), log("r02", "a", "brief", "a brief")];
        for i in 3..10 {
            logs.push(log(&format!("r0{i}"), "a", "prompt", &format!("prompt {i}")));
        }
        logs.push(log("r10", "a", "prompt", "same"));
        logs.push(log("r11", "a", "prompt", "  "));
        let got: Vec<&str> = past_prompts(&logs, "a").iter().map(|l| l.run.as_str()).collect();
        assert_eq!(got, ["r10", "r09", "r08", "r07", "r06"]);
    }
}
