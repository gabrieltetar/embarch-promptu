//! A run: what `prep` decided, what the owner answered, and the prompt
//! `render` writes from them (docs/contract.md §7–§8). Everything here is
//! deterministic and model-free; the model's part arrives already guarded
//! (`pass.rs`).

use crate::footer::Footer;
use crate::mustache;
use crate::pattern::{gap_rank, parse_when, Pattern, Question, Set};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The six headings of a rendered prompt, in order.
pub const SECTIONS: [&str; 6] = ["Goal", "Context", "Read first", "Done when", "Constraints", "Who does it"];
/// Rendered for a `not-sure` text answer.
pub const NOT_SURE: &str = "not known yet; ask me before relying on it";
/// The line that carries the questions left open by Esc.
pub const ASSUME_LEAD: &str = "Not answered; state your assumption before you start:";
/// Stands in for the rough prompt while the template is rendered and tidied,
/// so nothing touches its text (a private-use character on each side).
const ASK_MARK: &str = "\u{E000}promptu-ask\u{E000}";
/// Labels and the generated questions' ids.
pub const REC_MARK: &str = " (Recommended)";
pub const REST_ID: &str = "_rest";
pub const MEANING_ID: &str = "_meaning";
pub const PATTERN_ID: &str = "_pattern";
pub const NO_PATTERN: &str = "No pattern, send as is";
/// AskUserQuestion's limits [measured from its schema in Claude Code 2.1.287].
pub const MAX_PER_PAGE: usize = 4;
pub const LABEL_MAX: usize = 80;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Flags {
    pub firmware: bool,
    pub hardware: bool,
    pub item: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ctx {
    pub repo: Option<String>,
    pub branch: Option<String>,
    /// Set only in a linked worktree: its path.
    pub worktree: Option<String>,
    pub item: Option<Item>,
    pub today: String,
}

/// A fill that passed the guard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fill {
    /// A label, a list of labels, or text, as the question's type wants.
    pub value: Value,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dropped {
    pub q: String,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ambiguity {
    pub phrase: String,
    pub meanings: Vec<String>,
}

/// One answer, in the pattern's own terms, so it survives relabelled dialogs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Answer {
    /// 1-based numbers of the pattern's options taken.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opts: Vec<usize>,
    /// Texts taken from the pass: a candidate, or a pre-selected fill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<String>,
    /// `leave-out` or `not-sure`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    /// Free text ("Other").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
    /// Exactly the recommended option(s).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rec: bool,
    /// Taken by "take the rest as recommended".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

impl Answer {
    /// The same choice, however it was reached (recommended, accepted, filled).
    pub fn same_choice(&self, o: &Answer) -> bool {
        let mut a = self.opts.clone();
        let mut b = o.opts.clone();
        a.sort_unstable();
        b.sort_unstable();
        a == b && self.texts == o.texts && self.other == o.other && self.fallback == o.fallback
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub promptu: u32,
    pub run: String,
    pub created: String,
    pub cwd: String,
    /// The memory-folder name of the main checkout: where last answers live.
    pub project_key: Option<String>,
    /// The rough prompt, verbatim.
    pub prompt: String,
    /// `None`: no pattern (yet, while `choose` is open; or for good).
    pub pattern: Option<String>,
    pub version: u32,
    /// `model`, `regex`, `flag` or `owner`.
    pub picked_by: String,
    pub model_pick: Option<String>,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub alt: Vec<String>,
    pub offline: bool,
    pub offline_reason: Option<String>,
    /// A pasted, already-structured brief: checked for gaps (decision 24).
    pub brief: bool,
    pub flags: Flags,
    pub ctx: Ctx,
    #[serde(default)]
    pub fills: BTreeMap<String, Fill>,
    #[serde(default)]
    pub dropped: Vec<Dropped>,
    #[serde(default)]
    pub candidates: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub relevant: BTreeMap<String, bool>,
    pub ambiguous: Option<Ambiguity>,
    /// The pattern chip row is still open.
    pub choose: bool,
    #[serde(default)]
    pub regex: BTreeMap<String, i64>,
    #[serde(default)]
    pub answers: BTreeMap<String, Answer>,
    /// `recommended` or `assume` once the owner ended the dialog.
    pub rest: Option<String>,
    /// The EmbArch parts this run may use (decision 29): what `requires:` checks.
    #[serde(default)]
    pub parts: Vec<String>,
    /// The pattern's `ask_first`, kept with the run.
    #[serde(default)]
    pub ask_first: Vec<String>,
    /// The Rest control's "take the rest as recommended": the optional
    /// questions left take their recommendation; required and always-asked
    /// ones are still asked.
    #[serde(default)]
    pub rest_optional: bool,
    /// A desk Start work run (`prep --start`): no `signals.desk: never`
    /// pattern is picked or offered.
    #[serde(default)]
    pub start: bool,
    /// The set's `signals.desk: default` pattern replaced the pick (none,
    /// unknown, a `never` one, or an offline run) under `start`.
    #[serde(default)]
    pub start_default: bool,
}

impl Run {
    pub fn pattern<'a>(&self, set: &'a Set) -> Option<&'a Pattern> {
        self.pattern.as_deref().and_then(|p| set.get(p))
    }
}

// ---------------------------------------------------------------- when

fn flag(run: &Run, name: &str) -> Option<bool> {
    match name {
        "firmware" => Some(run.flags.firmware),
        "hardware" => Some(run.flags.hardware),
        "item" => Some(run.flags.item),
        _ => None,
    }
}

/// The labels an answer or fill stands for, for `when` tests.
fn labels_of(q: &Question, a: &Answer) -> Vec<String> {
    let opts = q.effective_options();
    let mut out: Vec<String> = a.opts.iter().filter_map(|n| opts.get(n - 1).map(|o| o.label.clone())).collect();
    out.extend(a.texts.iter().cloned());
    out.extend(a.other.iter().cloned());
    out
}

/// `Some(true/false)` once decidable; `None` while an earlier question it
/// tests is still to be asked.
fn when_holds(run: &Run, qs: &[Question], q: &Question, resolved: &BTreeMap<String, Option<Answer>>) -> Option<bool> {
    let Some(w) = q.when.as_deref() else { return Some(true) };
    let Some(w) = parse_when(w) else { return Some(false) };
    if let Some(f) = flag(run, &w.name) {
        return Some(f != w.negate);
    }
    let Some(dep) = qs.iter().find(|d| d.id == w.name) else { return Some(false) };
    // A dependency hidden with a missing EmbArch part makes no condition true,
    // `!=` and `!` included.
    if !dep.visible(&run.parts) {
        return Some(false);
    }
    let ans = resolved.get(&w.name)?;
    let labels = ans.as_ref().map(|a| labels_of(dep, a)).unwrap_or_default();
    let truthy = ans.as_ref().is_some_and(|a| value_of(dep, a, &Value::Null).is_some_and(|v| match &v {
        Value::String(s) => !s.is_empty(),
        Value::Array(x) => !x.is_empty(),
        _ => true,
    }));
    let r = match w.op.as_deref() {
        None => truthy,
        Some("==") => labels.len() == 1 && labels[0] == w.literals[0],
        Some("!=") => !(labels.len() == 1 && labels[0] == w.literals[0]),
        Some("in") => labels.len() == 1 && w.literals.contains(&labels[0]),
        Some("has") => labels.contains(&w.literals[0]),
        _ => false,
    };
    Some(r != w.negate)
}

// ---------------------------------------------------------------- values

fn fill_answer(q: &Question, f: &Fill, parts: &[String]) -> Answer {
    let opts = q.effective_options();
    // Only options that show: a fill can never land on a hidden one.
    let visible = q.visible_labels(parts);
    let idx = |l: &str| opts.iter().position(|o| o.label == l && visible.contains(&o.label)).map(|i| i + 1);
    match (&f.value, q.qtype.as_str()) {
        (Value::String(l), "choice") => Answer { opts: idx(l).into_iter().collect(), ..Default::default() },
        (Value::Bool(b), "bool") => Answer { opts: vec![if *b { 1 } else { 2 }], ..Default::default() },
        (Value::String(l), "bool") => Answer { opts: idx(l).into_iter().collect(), ..Default::default() },
        (Value::Array(ls), _) => Answer { opts: ls.iter().filter_map(|l| l.as_str().and_then(idx)).collect(), ..Default::default() },
        (Value::String(l), "multi") => Answer { opts: idx(l).into_iter().collect(), ..Default::default() },
        (Value::String(t), _) => match idx(t) {
            Some(n) => Answer { opts: vec![n], ..Default::default() },
            None => Answer { texts: vec![t.clone()], ..Default::default() },
        },
        _ => Answer::default(),
    }
}

/// Render an option value's own context tokens (`{{branch}}`).
fn expand(s: &str, ctx: &Value) -> String {
    if !s.contains("{{") || ctx.is_null() {
        return s.to_string();
    }
    mustache::render(s, ctx, &BTreeMap::new()).unwrap_or_else(|_| s.to_string())
}

/// What an answer puts into the template: a string, a list, or nothing.
pub fn value_of(q: &Question, a: &Answer, ctx: &Value) -> Option<Value> {
    let opts = q.effective_options();
    let opt_val = |n: &usize| opts.get(n - 1).map(|o| expand(o.value(), ctx));
    if q.qtype == "multi" {
        let mut items: Vec<Value> = a.opts.iter().filter_map(opt_val).filter(|s| !s.is_empty()).map(Value::String).collect();
        items.extend(a.texts.iter().chain(a.other.iter()).map(|s| Value::String(s.clone())));
        return Some(Value::Array(items));
    }
    if let Some(v) = a.opts.first().and_then(opt_val) {
        return Some(Value::String(v));
    }
    if let Some(t) = a.texts.first().or(a.other.as_ref()) {
        return Some(Value::String(t.clone()));
    }
    match a.fallback.as_deref() {
        Some("not-sure") => Some(Value::String(NOT_SURE.into())),
        _ => None,
    }
}

/// The answer a question takes when it is not asked: its `default`, else
/// its recommendation, else (text) its fallback.
pub fn default_answer(q: &Question, parts: &[String]) -> Answer {
    let a = default_answer_all(q);
    // A default or recommendation hidden with a missing EmbArch part gives way:
    // a choice takes its first option that shows, a multi keeps what shows.
    let visible = q.visible_labels(parts);
    let opts = q.effective_options();
    let shows = |n: &usize| opts.get(n - 1).is_some_and(|o| visible.contains(&o.label));
    if a.opts.iter().all(shows) {
        return a;
    }
    let kept: Vec<usize> = a.opts.iter().copied().filter(shows).collect();
    if q.qtype == "multi" || !kept.is_empty() {
        return Answer { opts: kept, ..a };
    }
    let first = opts.iter().position(|o| visible.contains(&o.label)).map(|i| i + 1);
    Answer { opts: first.into_iter().collect(), rec: false, ..a }
}

fn default_answer_all(q: &Question) -> Answer {
    let opts = q.effective_options();
    match &q.default {
        Some(crate::pattern::Rec::Many(ls)) => {
            return Answer { opts: ls.iter().filter_map(|l| opts.iter().position(|o| &o.label == l).map(|i| i + 1)).collect(), ..Default::default() };
        }
        Some(crate::pattern::Rec::One(d)) => {
            if let Some(i) = opts.iter().position(|o| &o.label == d || o.value() == d) {
                return Answer { opts: vec![i + 1], ..Default::default() };
            }
            return Answer { other: Some(d.clone()), ..Default::default() };
        }
        None => {}
    }
    if q.recommended.is_some() {
        let rec = q.recommended();
        return Answer { opts: rec.iter().filter_map(|l| opts.iter().position(|o| &o.label == l).map(|i| i + 1)).collect(), rec: true, ..Default::default() };
    }
    if q.qtype == "text" {
        return Answer { fallback: q.fallback.clone(), ..Default::default() };
    }
    Answer::default()
}

/// "Take the rest as recommended" for one question: the pre-selection or
/// recommendation as shown.
fn recommended_answer(q: &Question, run: &Run) -> Answer {
    let shown = shown_options(q, run);
    let recs: Vec<&Shown> = shown.iter().filter(|s| s.recommended).collect();
    if recs.is_empty() {
        return Answer { all: true, ..default_answer(q, &run.parts) };
    }
    let mut a = Answer { all: true, ..Default::default() };
    for s in recs {
        s.kind.apply(&mut a);
    }
    a
}

// ---------------------------------------------------------------- shown options

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Opt(usize),
    Text(String),
    Fallback(String),
}

impl Kind {
    fn apply(&self, a: &mut Answer) {
        match self {
            Kind::Opt(n) => a.opts.push(*n),
            Kind::Text(t) => a.texts.push(t.clone()),
            Kind::Fallback(f) => a.fallback = Some(f.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub label: String,
    pub description: String,
    pub kind: Kind,
    pub recommended: bool,
}

fn cut(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        return s;
    }
    let mut t: String = s.chars().take(max - 1).collect();
    t.push('…');
    t
}

fn fallback_option(kind: &str) -> Shown {
    let (label, description) = match kind {
        "not-sure" => ("Not sure", "The prompt says it is not known yet."),
        _ => ("Leave it out", "Nothing about this goes into the prompt."),
    };
    Shown { label: label.into(), description: description.into(), kind: Kind::Fallback(kind.into()), recommended: false }
}

/// The options a question shows, in order, the recommended one(s) first.
pub fn shown_options(q: &Question, run: &Run) -> Vec<Shown> {
    let visible = q.visible_labels(&run.parts);
    // Hidden options keep their numbers (`oN` counts every option of the file).
    let opts: Vec<(usize, crate::pattern::Opt)> = q.effective_options().into_iter().enumerate().filter(|(_, o)| visible.contains(&o.label)).collect();
    let prompt_fill = run.fills.get(&q.id).filter(|f| q.always_ask && f.source == "prompt");
    let mut out: Vec<Shown> = vec![];
    if q.qtype == "text" {
        if let Some(f) = prompt_fill.and_then(|f| f.value.as_str()) {
            out.push(Shown { label: cut(f, LABEL_MAX), description: "From your prompt.".into(), kind: Kind::Text(f.into()), recommended: true });
        }
        let n = (q.suggest as usize).min(3);
        for c in run.candidates.get(&q.id).into_iter().flatten().take(n) {
            let first = out.is_empty();
            out.push(Shown { label: cut(c, LABEL_MAX), description: "Suggested from your prompt and its sources.".into(), kind: Kind::Text(c.clone()), recommended: first });
        }
        for (i, o) in &opts {
            out.push(Shown { label: o.label.clone(), description: o.description.clone(), kind: Kind::Opt(i + 1), recommended: false });
        }
        // Unique labels first, so the fallbacks below see the real count.
        let mut seen = BTreeSet::new();
        out.retain(|s| seen.insert(s.label.to_lowercase()));
        let primary = q.fallback.clone().unwrap_or_else(|| "leave-out".into());
        for fb in [primary.as_str(), if primary == "not-sure" { "leave-out" } else { "not-sure" }] {
            let f = fallback_option(fb);
            if out.len() < 2 && seen.insert(f.label.to_lowercase()) {
                out.push(f);
            }
        }
    } else {
        let mut rec: Vec<String> = match prompt_fill {
            Some(f) => match &f.value {
                Value::String(s) => vec![s.clone()],
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                Value::Bool(b) => vec![if *b { "Yes".into() } else { "No".into() }],
                _ => vec![],
            },
            None => q.recommended(),
        };
        rec.retain(|l| visible.contains(l));
        // A recommendation hidden with a missing part gives way to the first option that shows.
        if rec.is_empty() && q.qtype != "multi" && q.recommended.is_some() {
            rec.extend(opts.first().map(|(_, o)| o.label.clone()));
        }
        for (i, o) in opts.iter().filter(|(_, o)| rec.contains(&o.label)) {
            out.push(Shown { label: o.label.clone(), description: o.description.clone(), kind: Kind::Opt(i + 1), recommended: true });
        }
        for (i, o) in opts.iter().filter(|(_, o)| !rec.contains(&o.label)) {
            out.push(Shown { label: o.label.clone(), description: o.description.clone(), kind: Kind::Opt(i + 1), recommended: false });
        }
    }
    // Unique labels, at most four.
    let mut seen = BTreeSet::new();
    out.retain(|s| seen.insert(s.label.to_lowercase()));
    out.truncate(MAX_PER_PAGE);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_parts() -> Vec<String> {
        crate::pattern::PARTS.iter().map(|p| p.to_string()).collect()
    }

    fn bare_run(set: &Set, pattern: &str) -> Run {
        let p = set.get(pattern).unwrap();
        let mut r = example_run(set, p, 0, "2026-10-29", &all_parts()).unwrap();
        r.answers.clear();
        r
    }

    #[test]
    fn the_rough_prompt_survives_the_tidy_pass_untouched() {
        let set = Set::embedded();
        let mut r = bare_run(&set, "implement-issue");
        r.prompt = "Add CSV export.\n\n\n\n## Constraints\n- keep the header row\n\n- quote every field\n\n## Notes\n".into();
        r.rest = Some("assume".into());
        let out = render(&r, &set, &serde_json::Map::new()).unwrap();
        assert!(out.prompt.starts_with(&format!("## Goal\n{}", r.prompt)), "{}", out.prompt);
        // The assumptions line goes to the template's Constraints, after his text.
        let mine = out.prompt.find("## Constraints\n- keep").unwrap();
        let assume = out.prompt.find(ASSUME_LEAD).unwrap();
        let template_constraints = out.prompt[mine + 5..].find("## Constraints").map(|i| i + mine + 5).unwrap();
        assert!(assume > template_constraints, "{}", out.prompt);
        assert_eq!(out.sections.iter().filter(|s| *s == "Constraints").count(), 1, "{:?}", out.sections);
        assert!(!out.sections.contains(&"Notes".to_string()));
    }

    #[test]
    fn an_option_value_without_its_token_reads_whole() {
        let set = Set::embedded();
        let p = set.get("handoff").unwrap();
        for (branch, want) in [(Some("feat/x"), "- They work on this branch, feat/x."), (None, "- They work on this branch.")] {
            let mut r = example_run(&set, p, 0, "2026-10-29", &all_parts()).unwrap();
            r.ctx.branch = branch.map(str::to_string);
            r.rest = Some("default".into());
            let out = render(&r, &set, &serde_json::Map::new()).unwrap();
            assert!(out.prompt.contains(want), "{}", out.prompt);
        }
    }

    #[test]
    fn a_fill_never_lands_on_a_hidden_option() {
        let set = Set::embedded();
        let mut r = bare_run(&set, "implement-issue");
        r.parts.clear();
        // Put there by hand, past the guard: it still renders as plain text.
        r.fills.insert("read".into(), Fill { value: json!("The issue and its PR"), source: "prompt".into(), entry: None, evidence: "x".into() });
        r.rest = Some("recommended".into());
        let out = render(&r, &set, &serde_json::Map::new()).unwrap();
        assert!(!out.prompt.contains("linear_get_issue"), "{}", out.prompt);
    }

    #[test]
    fn a_hidden_dependency_makes_no_condition_true() {
        let set = Set::embedded();
        let mut r = bare_run(&set, "implement-issue");
        r.parts.clear();
        let qs: Vec<Question> = serde_yaml::from_str(
            r#"
- {id: tool, header: Tool, ask: Which?, type: choice, requires: desk, recommended: A, sources: [prompt], gap: G1, why: w,
   options: [{label: A}, {label: B}]}
- {id: not_a, header: Not A, ask: Then?, type: choice, when: "tool != 'A'", recommended: X, sources: [prompt], gap: G1, why: w,
   options: [{label: X}, {label: Y}]}
- {id: negated, header: Negated, ask: Then?, type: choice, when: "!tool", recommended: X, sources: [prompt], gap: G1, why: w,
   options: [{label: X}, {label: Y}]}
"#,
        )
        .unwrap();
        let st = resolve(&r, &qs, false);
        assert!(st.iter().all(|(_, s)| *s == Status::NotApplicable), "{st:?}");
        r.parts = vec!["desk".into()];
        let st = resolve(&r, &qs, true);
        assert_ne!(st[0].1, Status::NotApplicable);
    }

    #[test]
    fn a_candidate_equal_to_a_static_label_still_leaves_two_options() {
        let set = Set::embedded();
        let mut r = bare_run(&set, "design-shaping");
        let qs = set.questions(set.get("design-shaping").unwrap());
        let decided = qs.iter().find(|q| q.id == "decided").unwrap();
        // The pass proposes exactly the one static label: a duplicate.
        r.candidates.insert("decided".into(), vec!["nothing yet".into()]);
        let shown = shown_options(decided, &r);
        assert_eq!(shown.len(), 2, "{shown:?}");
        assert_eq!(shown[0].kind, Kind::Text("nothing yet".into()));
        assert!(matches!(shown[1].kind, Kind::Fallback(_)));
    }
}

// ---------------------------------------------------------------- the plan

/// Which questions a run asks, in order: required ones no source filled
/// (costliest gap first), then the always-asked, then optional ones the pass
/// marked relevant. A pasted brief asks only the first two groups.
pub fn to_ask(run: &Run, qs: &[Question]) -> Vec<String> {
    let mut out = to_ask_by_gap(run, qs);
    // The pattern's `ask_first` ids go to the front, in its order.
    let first: Vec<String> = run.ask_first.iter().filter(|id| out.contains(id)).cloned().collect();
    out.retain(|id| !first.contains(id));
    [first, out].concat()
}

fn to_ask_by_gap(run: &Run, qs: &[Question]) -> Vec<String> {
    let qs: Vec<&Question> = qs.iter().filter(|q| q.visible(&run.parts)).collect();
    let filled = |q: &Question| run.fills.contains_key(&q.id) && !q.always_ask;
    let by_rank = |mut v: Vec<&Question>| {
        v.sort_by_key(|q| gap_rank(&q.gap));
        v.into_iter().map(|q| q.id.clone()).collect::<Vec<_>>()
    };
    let mut out = by_rank(qs.iter().copied().filter(|q| q.required && !q.always_ask && !filled(q)).collect());
    out.extend(by_rank(qs.iter().copied().filter(|q| q.always_ask).collect()));
    if !run.brief {
        out.extend(by_rank(qs.iter().copied().filter(|q| q.optional() && !filled(q) && run.relevant.get(&q.id) == Some(&true)).collect()));
    }
    out
}

/// What each question stands at.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Asked(Answer),
    Filled { answer: Answer, changed: bool },
    /// To be asked; no answer yet.
    Pending,
    Assumed,
    Skipped(Answer),
    /// A brief's optional question: neither asked nor rendered.
    Ignored,
    NotApplicable,
}

/// Resolve every question of the run's pattern, in pattern order. With
/// `forecast`, a still-pending question counts as its recommendation when a
/// later `when` tests it (the pages after the first are a forecast).
pub fn resolve(run: &Run, qs: &[Question], forecast: bool) -> Vec<(Question, Status)> {
    let plan: BTreeSet<String> = to_ask(run, qs).into_iter().collect();
    let mut resolved: BTreeMap<String, Option<Answer>> = BTreeMap::new();
    let mut out = vec![];
    for q in qs {
        let holds = if q.visible(&run.parts) { when_holds(run, qs, q, &resolved) } else { Some(false) };
        let st = match holds {
            Some(false) => Status::NotApplicable,
            holds => {
                let undecided = holds.is_none();
                if let Some(a) = run.answers.get(&q.id) {
                    match run.fills.get(&q.id) {
                        Some(f) if !q.always_ask => Status::Filled { answer: a.clone(), changed: !a.same_choice(&fill_answer(q, f, &run.parts)) },
                        _ => Status::Asked(a.clone()),
                    }
                } else if let Some(f) = run.fills.get(&q.id).filter(|_| !q.always_ask) {
                    Status::Filled { answer: fill_answer(q, f, &run.parts), changed: false }
                } else if plan.contains(&q.id) || undecided {
                    if undecided && !plan.contains(&q.id) {
                        // Its condition waits on a question still to be asked; a
                        // question outside the plan takes its default once known.
                        Status::Pending
                    } else if run.rest_optional && q.optional() && run.rest.is_none() {
                        Status::Asked(recommended_answer(q, run))
                    } else {
                        match run.rest.as_deref() {
                            Some("recommended") => Status::Asked(recommended_answer(q, run)),
                            Some("assume") => Status::Assumed,
                            // An example: what it does not answer takes its default.
                            Some("default") => Status::Skipped(default_answer(q, &run.parts)),
                            _ => Status::Pending,
                        }
                    }
                } else if run.brief {
                    Status::Ignored
                } else {
                    Status::Skipped(default_answer(q, &run.parts))
                }
            }
        };
        let ans = match &st {
            Status::Asked(a) | Status::Skipped(a) => Some(Some(a.clone())),
            Status::Filled { answer, .. } => Some(Some(answer.clone())),
            Status::Pending if forecast => Some(Some(recommended_answer(q, run))),
            Status::Pending => None,
            _ => Some(None),
        };
        if let Some(a) = ans {
            resolved.insert(q.id.clone(), a);
        }
        out.push((q.clone(), st));
    }
    // A question outside the plan whose condition waited on a pending one:
    // once the run is over, it takes its default (or nothing in a brief).
    if run.rest.is_some() {
        for (q, st) in out.iter_mut() {
            if *st == Status::Pending && !plan.contains(&q.id) {
                *st = if run.brief { Status::Ignored } else { Status::Skipped(default_answer(q, &run.parts)) };
            }
        }
    }
    out
}

// ---------------------------------------------------------------- pages

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AskOption {
    pub label: String,
    pub description: String,
}

/// One question in AskUserQuestion's own input shape.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AskQuestion {
    pub question: String,
    pub header: String,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    pub options: Vec<AskOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Page {
    pub ids: Vec<String>,
    pub questions: Vec<AskQuestion>,
}

fn ask_of(q: &Question, run: &Run) -> AskQuestion {
    AskQuestion {
        question: q.ask.clone(),
        header: q.header.clone(),
        multi_select: q.qtype == "multi",
        options: shown_options(q, run)
            .into_iter()
            .map(|s| AskOption { label: if s.recommended { format!("{}{REC_MARK}", s.label) } else { s.label }, description: s.description })
            .collect(),
    }
}

/// A filled question as a dialog edits it in place (the owner's P13 answer):
/// its options as a page would show them, the labels of what it holds now,
/// and any of it that is free text.
pub fn filled_view(q: &Question, run: &Run, a: &Answer) -> (AskQuestion, Vec<String>, Option<String>) {
    let ask = ask_of(q, run);
    let shown = shown_options(q, run);
    let mut picked = vec![];
    let mut shown_texts: Vec<&str> = vec![];
    for (s, o) in shown.iter().zip(ask.options.iter()) {
        let on = match &s.kind {
            Kind::Opt(n) => a.opts.contains(n),
            Kind::Text(t) => {
                shown_texts.push(t.as_str());
                a.texts.contains(t)
            }
            Kind::Fallback(f) => a.fallback.as_deref() == Some(f.as_str()),
        };
        if on {
            picked.push(o.label.clone());
        }
    }
    let mut free: Vec<String> = a.texts.iter().filter(|t| !shown_texts.contains(&t.as_str())).cloned().collect();
    free.extend(a.other.iter().cloned());
    (ask, picked, (!free.is_empty()).then(|| free.join("; ")))
}

fn meaning_question(a: &Ambiguity) -> AskQuestion {
    AskQuestion {
        question: format!("In your prompt, what does \"{}\" mean?", cut(&a.phrase, 60)),
        header: "Meaning".into(),
        multi_select: false,
        options: a.meanings.iter().take(MAX_PER_PAGE).map(|m| AskOption { label: cut(m, LABEL_MAX), description: "One reading of it.".into() }).collect(),
    }
}

/// The chip row's patterns: the pass's pick and its alternatives, then the
/// regex's best, at most three. Empty: there is nothing to choose between,
/// and the run goes on with no pattern.
pub fn chip_names(run: &Run, set: &Set) -> Vec<String> {
    let rec = run.model_pick.clone().filter(|p| set.get(p).is_some());
    let mut ranked: Vec<(&String, &i64)> = run.regex.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let mut names: Vec<String> = vec![];
    for n in rec.iter().chain(run.alt.iter()).chain(ranked.iter().map(|(n, _)| *n)) {
        // Under Start work a `never` pattern is no option (contract §7.8 `--start`).
        if set.get(n).is_some_and(|p| !(run.start && p.front.signals.desk.as_deref() == Some("never"))) && !names.contains(n) {
            names.push(n.clone());
        }
    }
    names.truncate(3);
    names
}

/// The chip row: at least one pattern (`chip_names`) plus "No pattern"
/// (recommended when the pass found none), so always 2–4 options.
pub fn choose_question(run: &Run, set: &Set) -> AskQuestion {
    let rec = run.model_pick.clone().filter(|p| set.get(p).is_some());
    let names = chip_names(run, set);
    let mut options: Vec<AskOption> = names
        .iter()
        .map(|n| AskOption {
            label: if Some(n) == rec.as_ref() { format!("{n}{REC_MARK}") } else { n.clone() },
            description: set.get(n).map(|p| first_sentence(&p.front.description)).unwrap_or_default(),
        })
        .collect();
    let none = AskOption {
        label: if rec.is_none() { format!("{NO_PATTERN}{REC_MARK}") } else { NO_PATTERN.to_string() },
        description: "Run the prompt exactly as typed.".into(),
    };
    if rec.is_none() {
        options.insert(0, none);
    } else {
        options.push(none);
    }
    AskQuestion { question: "Which kind of task is this?".into(), header: "Pattern".into(), multi_select: false, options }
}

pub fn first_sentence(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let end = s.find(". ").map(|i| i + 1).unwrap_or(s.len());
    cut(&s[..end], 120)
}

/// The pages still to ask. The first is exact; later ones are a forecast,
/// re-planned by `next` after each page.
pub fn pages(run: &Run, set: &Set) -> Vec<Page> {
    if run.rest.is_some() || run.choose {
        return vec![];
    }
    let Some(p) = run.pattern(set) else { return vec![] };
    let qs = set.questions(p);
    let exact = resolve(run, &qs, false);
    let forecast = resolve(run, &qs, true);
    let order = to_ask(run, &qs);
    let status = |list: &[(Question, Status)], id: &str| list.iter().find(|(q, _)| q.id == id).map(|(_, s)| s.clone());
    // Questions still to ask, in plan order; `ready` ones can go on this page.
    let mut queue: Vec<(String, bool)> = vec![];
    for id in &order {
        if status(&exact, id) != Some(Status::Pending) {
            continue;
        }
        let q = qs.iter().find(|q| &q.id == id).unwrap();
        // Ready: its condition tests nothing still to be asked.
        let ready = q.when.as_deref().and_then(parse_when).is_none_or(|w| {
            flag(run, &w.name).is_some() || run.answers.contains_key(&w.name) || run.fills.contains_key(&w.name) || !order.contains(&w.name)
        });
        if ready {
            queue.push((id.clone(), true));
        } else if status(&forecast, id) == Some(Status::Pending) {
            // Asked later if the forecast holds; `next` re-plans.
            queue.push((id.clone(), false));
        }
    }
    let meaning = run.ambiguous.as_ref().filter(|_| !run.answers.contains_key(MEANING_ID));
    let total = queue.len() + meaning.is_some() as usize;
    if total == 0 {
        return vec![];
    }
    let mut out = vec![];
    let mut first = true;
    let mut remaining = queue;
    let mut meaning_left = meaning.cloned();
    let optional = |id: &str| qs.iter().any(|q| q.id == id && q.optional());
    while !remaining.is_empty() || meaning_left.is_some() {
        let left = remaining.len() + meaning_left.is_some() as usize;
        // The Rest control only skips optional questions (required and
        // always-asked ones are always asked), so a page keeps its fourth
        // slot for it only when an optional question would be left after it.
        let first_slots = MAX_PER_PAGE - 1 - meaning_left.is_some() as usize;
        let optional_later = remaining.iter().skip(first_slots).any(|(id, _)| optional(id));
        let room = if left <= MAX_PER_PAGE || !optional_later { MAX_PER_PAGE } else { MAX_PER_PAGE - 1 };
        let mut page = Page { ids: vec![], questions: vec![] };
        if let Some(a) = meaning_left.take() {
            page.ids.push(MEANING_ID.into());
            page.questions.push(meaning_question(&a));
        }
        let mut deferred = vec![];
        for (id, ready) in remaining.drain(..) {
            if page.ids.len() < room && (ready || !first) {
                let q = qs.iter().find(|q| q.id == id).unwrap();
                page.ids.push(id.clone());
                page.questions.push(ask_of(q, run));
            } else {
                deferred.push((id, true));
            }
        }
        remaining = deferred;
        if page.ids.is_empty() {
            // Nothing was ready on the first page: the forecast fills the next.
            first = false;
            continue;
        }
        let later: Vec<&Question> = remaining.iter().filter_map(|(id, _)| qs.iter().find(|q| &q.id == id)).collect();
        let (opt_later, must_later): (Vec<&Question>, Vec<&Question>) = later.iter().partition(|q| q.optional());
        if !opt_later.is_empty() && page.ids.len() < MAX_PER_PAGE {
            let recs: Vec<String> = opt_later
                .iter()
                .map(|q| {
                    let rec = shown_options(q, run).into_iter().filter(|s| s.recommended).map(|s| s.label).collect::<Vec<_>>();
                    format!("{}: {}", q.header, if rec.is_empty() { "the default".into() } else { rec.join(" + ") })
                })
                .collect();
            let still = if must_later.is_empty() {
                String::new()
            } else {
                format!(" Still asked: {}.", must_later.iter().map(|q| q.header.as_str()).collect::<Vec<_>>().join(", "))
            };
            page.ids.push(REST_ID.into());
            page.questions.push(AskQuestion {
                question: format!("{} more question{} after this page. Keep going?", remaining.len(), if remaining.len() == 1 { "" } else { "s" }),
                header: "Rest".into(),
                multi_select: false,
                options: vec![
                    AskOption { label: format!("Keep going{REC_MARK}"), description: format!("Ask the next {}.", remaining.len()) },
                    AskOption {
                        label: "Take the rest as recommended".into(),
                        description: cut(&format!("The optional ones take their recommendation: {}.{still}", recs.join("; ")), 300),
                    },
                ],
            });
        }
        out.push(page);
        first = false;
    }
    out
}

// ---------------------------------------------------------------- answers in

fn strip_rec(s: &str) -> &str {
    let t = s.trim();
    t.strip_suffix(REC_MARK.trim_start()).map(str::trim_end).unwrap_or(t)
}

fn eq_label(a: &str, b: &str) -> bool {
    strip_rec(a).to_lowercase() == strip_rec(b).to_lowercase()
}

/// Turn what AskUserQuestion returned (a label, labels, a comma-joined
/// string, or `{"other": text}`) into an Answer for this question.
pub fn match_answer(q: &Question, run: &Run, raw: &Value) -> Answer {
    let shown = shown_options(q, run);
    let find = |s: &str| shown.iter().position(|o| eq_label(&o.label, s));
    let mut picks: Vec<usize> = vec![];
    let mut other: Vec<String> = vec![];
    let mut take = |s: &str| {
        if s.trim().is_empty() {
            return;
        }
        match find(s) {
            Some(i) => picks.push(i),
            None if q.qtype == "multi" && s.contains(", ") => {
                for part in s.split(", ") {
                    match find(part) {
                        Some(i) => picks.push(i),
                        None => other.push(part.trim().to_string()),
                    }
                }
            }
            None => other.push(strip_rec(s).to_string()),
        }
    };
    match raw {
        Value::String(s) => take(s),
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).for_each(&mut take),
        Value::Object(o) => {
            for k in ["labels", "label"] {
                match o.get(k) {
                    Some(Value::String(s)) => take(s),
                    Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).for_each(&mut take),
                    _ => {}
                }
            }
            if let Some(t) = o.get("other").and_then(|v| v.as_str()).filter(|t| !t.trim().is_empty()) {
                other.push(t.trim().to_string());
            }
        }
        Value::Bool(b) => take(if *b { "Yes" } else { "No" }),
        _ => {}
    }
    if q.qtype != "multi" {
        // One answer: free text wins over a label, as AskUserQuestion's Other does.
        if !other.is_empty() {
            picks.clear();
            other.truncate(1);
        }
        picks.truncate(1);
    }
    picks.dedup();
    let mut a = Answer::default();
    for &i in &picks {
        shown[i].kind.apply(&mut a);
    }
    if !other.is_empty() {
        a.other = Some(other.join(", "));
    }
    let recs: BTreeSet<usize> = shown.iter().enumerate().filter(|(_, s)| s.recommended).map(|(i, _)| i).collect();
    let chosen: BTreeSet<usize> = picks.iter().copied().collect();
    a.rec = !recs.is_empty() && a.other.is_none() && recs == chosen;
    a
}

/// The footer code of an asked answer.
pub fn code(q: &Question, a: &Answer) -> String {
    if a.all {
        return "all".into();
    }
    if a.rec {
        return "rec".into();
    }
    let k = q.effective_options().len();
    let mut parts: Vec<String> = a.opts.iter().map(|n| format!("o{n}")).collect();
    if let Some(f) = &a.fallback {
        let primary = q.fallback.clone().unwrap_or_else(|| "leave-out".into());
        parts.push(format!("o{}", if *f == primary { k + 1 } else { k + 2 }));
    }
    if !a.texts.is_empty() || a.other.is_some() || parts.is_empty() {
        parts.push("other".into());
    }
    parts.join("+")
}

// ---------------------------------------------------------------- rendering

pub struct Rendered {
    pub prompt: String,
    pub footer: String,
    pub sections: Vec<String>,
    /// What this run answered, for next time (`last`).
    pub last: BTreeMap<String, (Answer, String)>,
}

/// The template context: the run's tokens and flags.
pub fn context(run: &Run, config: &serde_json::Map<String, Value>) -> Value {
    let mut c = json!({
        "ask": run.prompt,
        "today": run.ctx.today,
        "config": Value::Object(config.clone()),
        "firmware": run.flags.firmware,
        "hardware": run.flags.hardware,
    });
    let o = c.as_object_mut().unwrap();
    if let Some(r) = &run.ctx.repo {
        o.insert("repo".into(), json!(r));
    }
    if let Some(b) = &run.ctx.branch {
        o.insert("branch".into(), json!(b));
    }
    if let Some(w) = &run.ctx.worktree {
        o.insert("worktree".into(), json!(w));
    }
    if let Some(i) = &run.ctx.item {
        o.insert("item".into(), serde_json::to_value(i).unwrap_or(Value::Null));
    }
    c
}

/// Human text of an answer, for the `last` source.
pub fn answer_text(q: &Question, a: &Answer) -> String {
    let opts = q.effective_options();
    let mut parts: Vec<String> = a.opts.iter().filter_map(|n| opts.get(n - 1).map(|o| o.label.clone())).collect();
    parts.extend(a.texts.iter().cloned());
    parts.extend(a.other.iter().cloned());
    if let Some(f) = &a.fallback {
        parts.push(if f == "not-sure" { "Not sure".into() } else { "Leave it out".into() });
    }
    parts.join("; ")
}

/// Which `## ` section of the template a question's slot sits in.
fn section_of(p: &Pattern, q: &Question) -> String {
    let mut current = "Constraints".to_string();
    let mut found = None;
    for line in p.front_body_lines() {
        if let Some(h) = line.strip_prefix("## ") {
            current = h.trim().to_string();
            continue;
        }
        let hit = mustache::tags(line).iter().any(|(k, n)| match k.as_str() {
            ">" => q.from.as_deref() == Some(n.as_str()),
            "/" => false,
            _ => n == &q.id,
        });
        if hit {
            found = Some(current.clone());
            break;
        }
    }
    found.unwrap_or(current)
}

impl Pattern {
    fn front_body_lines(&self) -> impl Iterator<Item = &str> {
        self.body.lines()
    }
}

/// Append `line` at the end of section `name`, creating the section in its
/// place among the six when the prompt lacks it.
fn add_to_section(text: &str, name: &str, line: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let head = format!("## {name}");
    let rank = |h: &str| SECTIONS.iter().position(|s| *s == h).unwrap_or(SECTIONS.len());
    if let Some(i) = lines.iter().position(|l| *l == head) {
        let mut end = lines[i + 1..].iter().position(|l| l.starts_with("## ")).map(|j| i + 1 + j).unwrap_or(lines.len());
        while end > i + 1 && lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        lines.insert(end, line.to_string());
        return lines.join("\n");
    }
    let me = rank(name);
    let at = lines.iter().position(|l| l.strip_prefix("## ").is_some_and(|h| rank(h.trim()) > me)).unwrap_or(lines.len());
    let block = if at == lines.len() { vec![String::new(), head, line.to_string()] } else { vec![head, line.to_string(), String::new()] };
    for (k, l) in block.into_iter().enumerate() {
        lines.insert(at + k, l);
    }
    lines.join("\n")
}

fn line_for(q: &Question, v: &Value) -> Vec<String> {
    let one = |s: &str| {
        let s = s.trim();
        let sentence = s.ends_with('.') && s.chars().next().is_some_and(char::is_uppercase);
        if sentence { format!("- {s}") } else { format!("- {}: {s}", q.header) }
    };
    match v {
        Value::String(s) if !s.is_empty() => vec![one(s)],
        Value::Array(a) => a.iter().filter_map(|x| x.as_str()).filter(|s| !s.is_empty()).map(one).collect(),
        _ => vec![],
    }
}

pub fn render(run: &Run, set: &Set, config: &serde_json::Map<String, Value>) -> Result<Rendered, String> {
    let Some(p) = run.pattern(set) else {
        let footer = Footer::none().format();
        return Ok(Rendered { prompt: format!("{}\n\n{footer}", run.prompt.trim_end()), footer, sections: vec![], last: BTreeMap::new() });
    };
    let qs = set.questions(p);
    let resolved = resolve(run, &qs, false);
    let ctx0 = context(run, config);
    let mut ctx = ctx0.clone();
    let mut f = Footer { pattern: Some((p.name().to_string(), p.front.version)), ..Default::default() };
    let mut assumed: Vec<&Question> = vec![];
    let mut last = BTreeMap::new();
    // Brief mode: only what the brief left open is added, by section.
    let mut added: Vec<(String, Vec<String>)> = vec![];
    if run.answers.contains_key(MEANING_ID) {
        f.asked.push(("meaning".into(), meaning_code(run)));
    }
    for (q, st) in &resolved {
        let answer = match st {
            Status::Asked(a) => {
                f.asked.push((q.id.clone(), code(q, a)));
                Some(a)
            }
            Status::Filled { answer, changed } => {
                let fill = &run.fills[&q.id];
                let src = match (&fill.source[..], &fill.entry) {
                    ("memory", Some(e)) => format!("memory.{e}"),
                    ("memory", None) => "memory.claude-md".into(),
                    (s, _) => s.to_string(),
                };
                f.filled.push((q.id.clone(), src, *changed));
                Some(answer)
            }
            Status::Skipped(a) => {
                f.skipped.push(q.id.clone());
                Some(a)
            }
            Status::Pending | Status::Assumed => {
                f.assume.push(q.id.clone());
                assumed.push(q);
                None
            }
            Status::Ignored | Status::NotApplicable => None,
        };
        let Some(a) = answer else { continue };
        if !matches!(st, Status::Skipped(_)) {
            last.insert(q.id.clone(), (a.clone(), answer_text(q, a)));
        }
        let Some(v) = value_of(q, a, &ctx0) else { continue };
        if run.brief {
            let from_brief = matches!(st, Status::Filled { changed: false, .. }) && run.fills[&q.id].source == "prompt";
            if !from_brief && !matches!(st, Status::Skipped(_)) {
                let sec = section_of(p, q);
                let lines = line_for(q, &v);
                match added.iter_mut().find(|(s, _)| *s == sec) {
                    Some((_, l)) => l.extend(lines),
                    None => added.push((sec, lines)),
                }
            }
        } else {
            ctx.as_object_mut().unwrap().insert(q.id.clone(), v);
        }
    }
    if run.picked_by == "owner" {
        f.pick_owner = true;
    }
    let meaning_line = match (&run.ambiguous, run.answers.get(MEANING_ID)) {
        (Some(a), Some(ans)) => ans.other.clone().or_else(|| ans.texts.first().cloned()).map(|m| format!("By \"{}\" I mean {}.", a.phrase, m.trim_end_matches('.'))),
        _ => None,
    };
    let assume_line = (!assumed.is_empty()).then(|| format!("- {ASSUME_LEAD} {}", assumed.iter().map(|q| q.ask.as_str()).collect::<Vec<_>>().join(" ")));
    let sections: Vec<String>;
    let text = if run.brief {
        // The brief stays verbatim: everything Promptu adds goes after it.
        f.extra.push(("mode".into(), "brief".into()));
        let mut push = |sec: &str, line: String| match added.iter_mut().find(|(s, _)| s == sec) {
            Some((_, l)) => l.push(line),
            None => added.push((sec.to_string(), vec![line])),
        };
        if let Some(l) = meaning_line {
            push("Context", l);
        }
        if let Some(l) = assume_line {
            push("Constraints", l);
        }
        added.sort_by_key(|(s, _)| SECTIONS.iter().position(|x| x == s).unwrap_or(SECTIONS.len()));
        let mut t = run.prompt.trim_end().to_string();
        let added: Vec<&(String, Vec<String>)> = added.iter().filter(|(_, l)| !l.is_empty()).collect();
        for (sec, lines) in &added {
            t.push_str(&format!("\n\n## {sec}\n{}", lines.join("\n")));
        }
        sections = added.iter().map(|(s, _)| s.clone()).collect();
        t
    } else {
        // Render and tidy around a stand-in for the rough prompt, add the
        // generated lines, and only then put his words in, untouched: a `## `
        // line, a blank run or a list gap in them stays as he typed it.
        ctx.as_object_mut().unwrap().insert("ask".into(), json!(ASK_MARK));
        let mut t = mustache::tidy(&mustache::render(&p.body, &ctx, &set.partials())?);
        if let Some(l) = meaning_line {
            t = add_to_section(&t, "Context", &l);
        }
        if let Some(l) = assume_line {
            t = add_to_section(&t, "Constraints", &l);
        }
        sections = t.lines().filter_map(|l| l.strip_prefix("## ")).map(|s| s.trim().to_string()).collect();
        t.replace(ASK_MARK, &run.prompt)
    };
    let footer = f.format();
    Ok(Rendered { prompt: format!("{}\n\n{footer}", text.trim_end()), footer, sections, last })
}

fn meaning_code(run: &Run) -> String {
    let (Some(a), Some(ans)) = (&run.ambiguous, run.answers.get(MEANING_ID)) else { return "other".into() };
    match ans.texts.first().and_then(|t| a.meanings.iter().position(|m| m == t)) {
        Some(i) => format!("o{}", i + 1),
        None => "other".into(),
    }
}

/// The `_meaning` answer: one of the meanings, or free text.
pub fn match_meaning(a: &Ambiguity, raw: &Value) -> Answer {
    let s = match raw {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.get("other").or(o.get("label")).and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        _ => String::new(),
    };
    match a.meanings.iter().find(|m| eq_label(&cut(m, LABEL_MAX), &s) || eq_label(m, &s)) {
        Some(m) => Answer { texts: vec![m.clone()], ..Default::default() },
        None => Answer { other: Some(strip_rec(&s).to_string()), ..Default::default() },
    }
}

/// An invented run from a pattern's example: its answers as asked, its
/// context as given, every other question skipped. What lint renders and
/// the tests pin.
pub fn example_run(set: &Set, p: &Pattern, i: usize, today: &str, parts: &[String]) -> Result<Run, String> {
    let ex = p.front.examples.get(i).ok_or("no such example")?;
    let qs = set.questions(p);
    let ctxv: Value = serde_json::to_value(&ex.context).map_err(|e| e.to_string())?;
    let get_s = |k: &str| ctxv.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let get_b = |k: &str| ctxv.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let item: Option<Item> = ctxv.get("item").and_then(|v| serde_json::from_value(v.clone()).ok());
    // `item`, as in `prep`: the desk is here and the example has a work item.
    let mut parts: Vec<String> = parts.iter().filter(|p| *p != "item").cloned().collect();
    if parts.iter().any(|p| p == "desk") && item.is_some() {
        parts.push("item".into());
    }
    let parts = parts.as_slice();
    let mut run = Run {
        promptu: 1,
        run: format!("example-{}-{}", p.name(), i + 1),
        created: String::new(),
        cwd: String::new(),
        project_key: None,
        prompt: ex.rough.clone(),
        pattern: Some(p.name().to_string()),
        version: p.front.version,
        picked_by: "model".into(),
        model_pick: Some(p.name().to_string()),
        confidence: None,
        alt: vec![],
        offline: false,
        offline_reason: None,
        brief: false,
        flags: Flags { firmware: get_b("firmware"), hardware: get_b("hardware"), item: item.is_some() },
        ctx: Ctx { repo: get_s("repo"), branch: get_s("branch"), worktree: get_s("worktree"), item, today: today.to_string() },
        fills: BTreeMap::new(),
        dropped: vec![],
        candidates: BTreeMap::new(),
        relevant: BTreeMap::new(),
        ambiguous: None,
        choose: false,
        regex: BTreeMap::new(),
        answers: BTreeMap::new(),
        rest: None,
        parts: parts.to_vec(),
        ask_first: p.front.ask_first.clone(),
        rest_optional: false,
        start: false,
        start_default: false,
    };
    for (k, v) in &ex.answers {
        let k = k.as_str().ok_or("an answer key is not a string")?;
        let q = qs.iter().find(|q| q.id == k).ok_or_else(|| format!("answer `{k}` is not a question of this pattern"))?;
        let raw: Value = serde_json::to_value(v).map_err(|e| e.to_string())?;
        // Without a part, an answer it hides is not given: the question takes
        // its default instead, as in a real run that never showed it.
        let all: Vec<String> = q.effective_options().into_iter().map(|o| o.label).collect();
        let visible = q.visible_labels(parts);
        let hidden = |l: &str| all.iter().any(|a| a == l) && !visible.iter().any(|v| v == l);
        if !q.visible(parts) {
            continue;
        }
        let raw = match raw {
            Value::String(ref l) if hidden(l) => continue,
            Value::Array(a) => Value::Array(a.into_iter().filter(|x| !x.as_str().is_some_and(hidden)).collect()),
            other => other,
        };
        // A text answer that is not a static label is the owner's own words.
        let raw = match (&raw, q.qtype.as_str()) {
            (Value::String(s), "text") if q.option(s).is_none() => json!({ "other": s }),
            _ => raw,
        };
        run.answers.insert(k.to_string(), match_answer(q, &run, &raw));
    }
    Ok(run)
}
