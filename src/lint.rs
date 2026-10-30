//! `promptu lint [<path>...] [--json] [--show]` (docs/contract.md §7.7): the
//! pattern files checked against the schema, AskUserQuestion's limits, their
//! own slots, and their examples rendered. No model.

use crate::footer;
use crate::mustache;
use crate::pattern::{parse_when, Pattern, Question, Set, CONTEXT, FLAGS, GAP_RANK, PARTS, SOURCES};
use crate::run;
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Finding {
    pub check: &'static str,
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct FileReport {
    pub path: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub questions: usize,
    pub examples: usize,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub promptu: u32,
    pub files: Vec<FileReport>,
    pub errors: usize,
    pub warnings: usize,
}

const HEADER_MAX: usize = 12;
const LABEL_WARN: usize = 30;
const LABEL_MAX: usize = 60;
const QUESTIONS_WARN: usize = 12;
const TYPES: [&str; 4] = ["choice", "multi", "text", "bool"];

fn err(check: &'static str, message: impl Into<String>) -> Finding {
    Finding { check, severity: "error", message: message.into() }
}

fn warn(check: &'static str, message: impl Into<String>) -> Finding {
    Finding { check, severity: "warning", message: message.into() }
}

fn question(q: &Question, earlier: &[Question], out: &mut Vec<Finding>) {
    let id = &q.id;
    let id_ok = Regex::new(r"^[a-z][a-z_]*$").unwrap();
    if !id_ok.is_match(id) {
        out.push(err("id", format!("`{id}`: an id is lowercase letters and `_`, starting with a letter")));
    }
    let hlen = q.header.chars().count();
    if hlen == 0 || hlen > HEADER_MAX {
        out.push(err("header", format!("`{id}`: header \"{}\" is {hlen} characters; AskUserQuestion takes 1–{HEADER_MAX}", q.header)));
    }
    if q.ask.trim().is_empty() {
        out.push(err("ask", format!("`{id}`: no `ask`")));
    }
    if !TYPES.contains(&q.qtype.as_str()) {
        out.push(err("type", format!("`{id}`: type `{}` is not choice, multi, text or bool", q.qtype)));
    }
    let n = q.options.len();
    match q.qtype.as_str() {
        "choice" | "multi" if !(2..=4).contains(&n) => out.push(err("options", format!("`{id}`: {n} options; AskUserQuestion takes 2–4"))),
        "text" if n > 3 => out.push(err("options", format!("`{id}`: a text question has at most 3 static options, not {n}"))),
        "bool" if n != 0 && n != 2 => out.push(err("options", format!("`{id}`: a bool question has 0 or 2 options"))),
        _ => {}
    }
    if q.qtype == "text" && !matches!(q.fallback.as_deref(), Some("leave-out") | Some("not-sure")) {
        out.push(err("fallback", format!("`{id}`: a text question needs `fallback: leave-out` or `not-sure`")));
    }
    if q.qtype != "text" && q.fallback.is_some() {
        out.push(err("fallback", format!("`{id}`: `fallback` is for text questions")));
    }
    if q.suggest > 3 || (q.suggest > 0 && q.qtype != "text") {
        out.push(err("suggest", format!("`{id}`: `suggest` is 0–3, on text questions only")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for o in &q.options {
        let l = o.label.chars().count();
        if l == 0 || l > LABEL_MAX {
            out.push(err("label", format!("`{id}`: label \"{}\" is {l} characters (1–{LABEL_MAX})", o.label)));
        } else if l > LABEL_WARN {
            out.push(warn("label", format!("`{id}`: label \"{}\" is {l} characters; a dialog label reads best at {LABEL_WARN} or fewer", o.label)));
        }
        if o.label.ends_with("(Recommended)") {
            out.push(err("label", format!("`{id}`: \"(Recommended)\" is added by Promptu, not written in a label")));
        }
        if !seen.insert(o.label.to_lowercase()) {
            out.push(err("label", format!("`{id}`: label \"{}\" appears twice", o.label)));
        }
        // An option's value is rendered with the context only: no answers, no partials.
        if let Some(v) = &o.value {
            for (kind, name) in mustache::tags(v) {
                let root = name.split('.').next().unwrap_or_default();
                if kind == ">" {
                    out.push(err("slot", format!("`{id}`: option \"{}\" includes a partial; an option value cannot", o.label)));
                } else if kind != "/" && !CONTEXT.contains(&root) && !FLAGS.contains(&root) {
                    out.push(err("slot", format!("`{id}`: option \"{}\" names `{{{{{name}}}}}`; an option value may name only context tokens and flags", o.label)));
                }
            }
            if let Err(e) = mustache::render(v, &serde_json::json!({}), &Default::default()) {
                out.push(err("slot", format!("`{id}`: option \"{}\" does not render: {e}", o.label)));
            }
        }
    }
    let labels = q.labels();
    match (q.qtype.as_str(), &q.recommended) {
        ("choice" | "bool", Some(crate::pattern::Rec::One(r))) if !labels.contains(&r.as_str()) && !(q.qtype == "bool" && n == 0) => {
            out.push(err("recommended", format!("`{id}`: recommended \"{r}\" is not one of its labels")))
        }
        ("choice", None) => out.push(err("recommended", format!("`{id}`: a choice question needs `recommended`"))),
        ("choice", Some(crate::pattern::Rec::Many(_))) => out.push(err("recommended", format!("`{id}`: a choice question recommends one label"))),
        ("multi", None) => out.push(err("recommended", format!("`{id}`: a multi question needs `recommended` (a list, possibly empty)"))),
        ("multi", Some(r)) => {
            for l in r.labels() {
                if !labels.contains(&l.as_str()) {
                    out.push(err("recommended", format!("`{id}`: recommended \"{l}\" is not one of its labels")));
                }
            }
        }
        ("text", Some(r)) => {
            for l in r.labels() {
                if !labels.contains(&l.as_str()) {
                    out.push(err("recommended", format!("`{id}`: recommended \"{l}\" is not one of its static labels")));
                }
            }
        }
        _ => {}
    }
    match &q.default {
        Some(crate::pattern::Rec::One(d)) if q.qtype != "text" && !q.options.iter().any(|o| &o.label == d || o.value() == d) => {
            out.push(err("default", format!("`{id}`: default \"{d}\" is neither a label nor a value")))
        }
        Some(crate::pattern::Rec::Many(ls)) => {
            if q.qtype != "multi" {
                out.push(err("default", format!("`{id}`: a list default is for multi questions")));
            }
            for l in ls {
                if !labels.contains(&l.as_str()) {
                    out.push(err("default", format!("`{id}`: default \"{l}\" is not one of its labels")));
                }
            }
        }
        _ => {}
    }
    if q.sources.is_empty() {
        out.push(err("sources", format!("`{id}`: no `sources`")));
    }
    for s in &q.sources {
        if !SOURCES.contains(&s.as_str()) {
            out.push(err("sources", format!("`{id}`: source `{s}` is not one of {}", SOURCES.join(", "))));
        }
    }
    if q.always_ask && q.sources.iter().any(|s| s == "memory") {
        out.push(err("volatile", format!("`{id}`: asked every run, so memory may never answer it")));
    }
    if q.always_ask && q.required {
        out.push(warn("volatile", format!("`{id}`: `required` means nothing on a question asked every run")));
    }
    if let Some(w) = &q.when {
        match parse_when(w) {
            None => out.push(err("when", format!("`{id}`: cannot read `when: {w}`"))),
            Some(w) => {
                let dep = earlier.iter().find(|e| e.id == w.name);
                if dep.is_none() && !FLAGS.contains(&w.name.as_str()) {
                    out.push(err("when", format!("`{id}`: `when` tests `{}`, which is neither an earlier question nor a flag", w.name)));
                }
                if let Some(d) = dep {
                    for l in &w.literals {
                        if !d.labels().contains(&l.as_str()) {
                            out.push(err("when", format!("`{id}`: `when` names \"{l}\", not a label of `{}`", d.id)));
                        }
                    }
                }
                if FLAGS.contains(&w.name.as_str()) && w.op.is_some() {
                    out.push(err("when", format!("`{id}`: a flag is tested bare, `{}` or `!{}`", w.name, w.name)));
                }
            }
        }
    }
    // EmbArch parts: known names, and the question still works without them.
    for r in q.requires.iter().chain(q.options.iter().filter_map(|o| o.requires.as_ref())) {
        if !PARTS.contains(&r.as_str()) {
            out.push(err("requires", format!("`{id}`: requires `{r}`, not one of {}", PARTS.join(", "))));
        }
    }
    if q.options.iter().any(|o| o.requires.is_some()) {
        let bare = q.visible_labels(&[]).len();
        let ok = match q.qtype.as_str() {
            "choice" | "multi" => (2..=4).contains(&bare),
            _ => true,
        };
        if !ok {
            out.push(err("requires", format!("`{id}`: without EmbArch it shows {bare} options; AskUserQuestion takes 2–4")));
        }
        if q.required && q.requires.is_none() && q.qtype != "text" && bare == 0 {
            out.push(err("requires", format!("`{id}`: required, but nothing shows without EmbArch")));
        }
    }
    if q.gap != "none" && !GAP_RANK.contains(&q.gap.as_str()) {
        out.push(err("gap", format!("`{id}`: gap `{}` is not G1–G16 or none", q.gap)));
    }
    if q.why.trim().is_empty() {
        out.push(err("why", format!("`{id}`: no `why`")));
    }
}

/// The checks that need the whole set: includes, slots, examples.
fn pattern(p: &Pattern, set: &Set, today: &str, parts: &[String], show: &mut Vec<String>, out: &mut Vec<Finding>) {
    let f = &p.front;
    let qs = set.questions(p);
    for inc in &f.include {
        match set.fragments.get(&inc.fragment) {
            None => out.push(err("include", format!("includes `{}`, which is not a fragment in the set", inc.fragment))),
            Some(frag) => {
                if let Some(r) = &inc.recommended {
                    for l in r.labels() {
                        if !frag.front.questions.iter().any(|q| q.labels().contains(&l.as_str())) {
                            out.push(err("include", format!("`{}`: recommended \"{l}\" is not a label of the fragment", inc.fragment)));
                        }
                    }
                }
            }
        }
    }
    for id in &f.ask_first {
        if !qs.iter().any(|q| &q.id == id) {
            out.push(err("ask_first", format!("`ask_first` names `{id}`, not a question of this pattern")));
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for (i, q) in qs.iter().enumerate() {
        if !ids.insert(q.id.clone()) {
            out.push(err("id", format!("`{}` is defined twice (fragments included)", q.id)));
        }
        if q.from.is_some() {
            // A fragment's question with this pattern's recommended and when.
            let mut sub = vec![];
            question(q, &qs[..i], &mut sub);
            out.extend(sub.into_iter().filter(|x| matches!(x.check, "recommended" | "when")));
        } else {
            question(q, &qs[..i], out);
        }
    }
    if qs.len() > QUESTIONS_WARN {
        out.push(warn("size", format!("{} questions: more than {QUESTIONS_WARN} slows the dialog (not a cap)", qs.len())));
    }
    for (kind, name) in mustache::tags(&p.body) {
        match kind.as_str() {
            ">" => {
                if !set.fragments.contains_key(&name) {
                    out.push(err("partial", format!("`{{{{> {name}}}}}` names no fragment")));
                }
            }
            "/" => {}
            _ => {
                let root = name.split('.').next().unwrap_or_default();
                if name != "." && !ids.contains(root) && !CONTEXT.contains(&root) && !FLAGS.contains(&root) {
                    out.push(err("slot", format!("`{{{{{name}}}}}` is not a question, a context token or a flag")));
                }
            }
        }
    }
    if f.examples.is_empty() {
        out.push(warn("example", "no examples: the pass reads them, and the tests render them"));
    }
    let set_config = serde_json::Map::from_iter([("voice_skill".to_string(), serde_json::json!("example-voice"))]);
    for (i, ex) in f.examples.iter().enumerate() {
        let n = i + 1;
        match run::example_run(set, p, i, today, parts) {
            Err(e) => out.push(err("example", format!("example {n}: {e}"))),
            Ok(mut r) => {
                r.rest = Some("default".into());
                match run::render(&r, set, &set_config) {
                    Err(e) => out.push(err("example", format!("example {n} does not render: {e}"))),
                    Ok(out_r) => {
                        if out_r.prompt.contains("{{") || out_r.prompt.contains("}}") {
                            out.push(err("example", format!("example {n} renders a stray mustache tag")));
                        }
                        if let Err(e) = footer::parse(&out_r.footer) {
                            out.push(err("footer", format!("example {n}'s footer breaks the grammar: {e}")));
                        }
                        if !out_r.prompt.contains(ex.rough.trim()) {
                            out.push(err("example", format!("example {n}: the rough prompt is not in the render verbatim")));
                        }
                        show.push(format!("######## {} example {n}\n{}\n", p.name(), out_r.prompt));
                    }
                }
            }
        }
        // The no-model pre-pick should not put another pattern above this one.
        let scores = crate::pass::regex_scores(set, &ex.rough);
        let mine = scores.get(p.name()).copied().unwrap_or(0);
        if let Some((other, s)) = scores.iter().filter(|(k, _)| k.as_str() != p.name()).max_by_key(|(_, s)| **s) {
            if *s > mine {
                out.push(warn("signals", format!("example {n}: the keyword pre-pick scores `{other}` {s} against this pattern's {mine}")));
            }
        }
    }
}

fn front_checks(p: &Pattern, path: &Path, out: &mut Vec<Finding>) {
    let f = &p.front;
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let stem = stem.strip_prefix("embedded:").unwrap_or(&stem).to_string();
    let name_ok = Regex::new(r"^[a-z0-9-]{1,64}$").unwrap();
    if !name_ok.is_match(&f.name) {
        out.push(err("name", format!("name `{}`: 1–64 of a–z, 0–9 and -", f.name)));
    }
    let want = if p.is_fragment() { format!("_{}", f.name) } else { f.name.clone() };
    if stem != want {
        out.push(err("name", format!("the file is `{stem}.md`; a {} named `{}` lives in `{want}.md`", if p.is_fragment() { "fragment" } else { "pattern" }, f.name)));
    }
    if f.description.trim().is_empty() || f.description.chars().count() > 1024 {
        out.push(err("description", format!("description is {} characters (1–1,024)", f.description.chars().count())));
    }
    if f.version == 0 {
        out.push(err("version", "`version` is a whole number from 1"));
    }
    if !matches!(f.kind.as_str(), "task" | "fragment") {
        out.push(err("kind", format!("kind `{}` is not task or fragment", f.kind)));
    }
    for rx in f.signals.phrases.iter().chain(f.signals.not.iter()) {
        if let Err(e) = RegexBuilder::new(rx).case_insensitive(true).build() {
            out.push(err("signals", format!("`{rx}` is not a regex: {}", e.to_string().lines().last().unwrap_or_default())));
        }
    }
    if let Some(d) = &f.signals.desk {
        if !matches!(d.as_str(), "default" | "possible" | "never") {
            out.push(err("signals", format!("signals.desk `{d}` is not default, possible or never")));
        }
    }
    if p.is_fragment() {
        if !f.include.is_empty() || !f.examples.is_empty() {
            out.push(err("kind", "a fragment neither includes nor has examples"));
        }
        for (i, q) in f.questions.iter().enumerate() {
            question(q, &f.questions[..i], out);
        }
        for (kind, name) in mustache::tags(&p.body) {
            if kind == ">" {
                out.push(err("partial", format!("a fragment includes no partial (`{name}`)")));
            } else if kind != "/" {
                let root = name.split('.').next().unwrap_or_default();
                if name != "." && !f.questions.iter().any(|q| q.id == root) && !CONTEXT.contains(&root) && !FLAGS.contains(&root) {
                    out.push(err("slot", format!("`{{{{{name}}}}}` is not one of the fragment's questions, a context token or a flag")));
                }
            }
        }
    }
}

/// Lint a set; `only` limits the report to those files.
pub fn lint(set: &Set, only: Option<&[PathBuf]>, today: &str, parts: &[String]) -> (Report, Vec<String>) {
    let mut files = vec![];
    let mut show = vec![];
    let wanted = |file: &str| only.is_none_or(|o| o.iter().any(|p| std::fs::canonicalize(p).ok() == std::fs::canonicalize(file).ok()));
    for (file, e) in &set.broken {
        if wanted(file) {
            files.push(FileReport { path: file.clone(), name: None, kind: None, questions: 0, examples: 0, findings: vec![err("schema", e.clone())] });
        }
    }
    for p in set.fragments.values().chain(set.patterns.values()) {
        if !wanted(&p.file) {
            continue;
        }
        let mut findings = vec![];
        front_checks(p, Path::new(&p.file), &mut findings);
        if !p.is_fragment() {
            pattern(p, set, today, parts, &mut show, &mut findings);
        }
        files.push(FileReport {
            path: p.file.clone(),
            name: Some(p.name().to_string()),
            kind: Some(p.front.kind.clone()),
            questions: if p.is_fragment() { p.front.questions.len() } else { set.questions(p).len() },
            examples: p.front.examples.len(),
            findings,
        });
    }
    let errors = files.iter().flat_map(|f| &f.findings).filter(|f| f.severity == "error").count();
    let warnings = files.iter().flat_map(|f| &f.findings).filter(|f| f.severity == "warning").count();
    (Report { promptu: 1, files, errors, warnings }, show)
}

pub fn text(r: &Report) -> String {
    let mut s = format!("{:22} {:8} {:>9} {:>8}  findings\n", "pattern", "kind", "questions", "examples");
    for f in &r.files {
        s.push_str(&format!("{:22} {:8} {:>9} {:>8}  {}\n", f.name.as_deref().unwrap_or(&f.path), f.kind.as_deref().unwrap_or("?"), f.questions, f.examples, f.findings.len()));
        for x in &f.findings {
            s.push_str(&format!("    {} {:7} {}\n", if x.severity == "error" { "error" } else { "warn " }, x.check, x.message));
        }
    }
    s.push_str(&format!("{} file(s): {} error(s), {} warning(s)", r.files.len(), r.errors, r.warnings));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_set_is_clean() {
        let set = Set::embedded();
        let all: Vec<String> = PARTS.iter().map(|p| p.to_string()).collect();
        let (r, show) = lint(&set, None, "2026-10-29", &all);
        assert_eq!(r.errors, 0, "{}", text(&r));
        assert_eq!(r.files.len(), 13);
        let examples: usize = set.patterns.values().map(|p| p.front.examples.len()).sum();
        assert_eq!(show.len(), examples);
    }

    #[test]
    fn tags_inside_option_values_are_checked() {
        let q: Question = serde_yaml::from_str(
            r#"{id: a, header: A, ask: A?, type: choice, recommended: One, sources: [prompt], gap: G1, why: w,
                options: [{label: One, value: "on {{branch}}"}, {label: Two, value: "after {{goal}}"}, {label: Three, value: "{{#branch}}open"}]}"#,
        )
        .unwrap();
        let mut out = vec![];
        question(&q, &[], &mut out);
        let msgs: Vec<&str> = out.iter().filter(|f| f.check == "slot").map(|f| f.message.as_str()).collect();
        assert_eq!(msgs.len(), 2, "{msgs:?}");
        assert!(msgs[0].contains("`{{goal}}`"));
        assert!(msgs[1].contains("does not render"));
    }

    #[test]
    fn broken_patterns_are_caught() {
        let dir = std::env::temp_dir().join(format!("promptu-lint-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("bad.md"),
            r#"---
name: bad
description: A broken pattern.
version: 1
kind: task
signals: {phrases: ["(unclosed"]}
include: [{fragment: nope}]
ask_first: [nowhere]
questions:
  - id: first
    header: A header far too long
    ask: Which?
    type: choice
    options: [{label: Only one}]
    recommended: Missing
    always_ask: true
    sources: [memory, web]
    gap: G99
  - id: second
    header: Second
    ask: Then?
    type: text
    requires: nope
    when: "first == 'Nope'"
    sources: [prompt]
    gap: G1
    why: x
---
## Goal
{{{ask}}} {{unknown}} {{> nope}}
"#,
        )
        .unwrap();
        std::fs::write(dir.join("wrong-name.md"), "---\nname: other\ndescription: d\nversion: 1\nkind: task\n---\n## Goal\n{{{ask}}}\n").unwrap();
        std::fs::write(dir.join("junk.md"), "no frontmatter").unwrap();
        let set = Set::from_dir(&dir).unwrap();
        let (r, _) = lint(&set, None, "2026-10-29", &[]);
        let checks: std::collections::BTreeSet<&str> = r.files.iter().flat_map(|f| &f.findings).filter(|f| f.severity == "error").map(|f| f.check).collect();
        for c in ["schema", "signals", "include", "header", "options", "recommended", "volatile", "sources", "gap", "why", "when", "fallback", "slot", "partial", "name", "requires", "ask_first"] {
            assert!(checks.contains(c), "missing {c}: {}", text(&r));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
