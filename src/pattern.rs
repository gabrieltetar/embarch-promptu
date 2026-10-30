//! Pattern files (docs/contract.md §7.1): one Markdown file per pattern, its
//! frontmatter the quiz, its body a logic-less Mustache template; `_<id>.md`
//! is a fragment other patterns include. The public set is embedded in the
//! binary (build.rs); `PROMPTU_PATTERNS` or `--patterns` reads a folder.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));

/// The question sources, in the contract's spelling.
pub const SOURCES: [&str; 5] = ["prompt", "desk", "memory", "last", "context"];
/// The flags a `when` may test, besides earlier questions.
pub const FLAGS: [&str; 3] = ["firmware", "hardware", "item"];
/// What a question or an option may need (`requires:`): an EmbArch part, or
/// `item`, the desk with a work item passed to this run. Without it, the
/// question or option is hidden and the generic rest remains.
pub const PARTS: [&str; 3] = ["desk", "embarch-api", "item"];
/// The EmbArch parts a session has or not: what `--have` and `[embarch]` name.
/// `item` is not one: it follows from the desk and the work item.
pub const DETECTED: [&str; 2] = ["desk", "embarch-api"];

/// Context tokens a template may name besides question ids and flags.
pub const CONTEXT: [&str; 7] = ["ask", "item", "repo", "branch", "worktree", "today", "config"];

/// The friction gaps, costliest first (SYNTHESIS §2.4, stream D's order):
/// the order unanswered questions are asked in. `none` last.
pub const GAP_RANK: [&str; 16] = ["G10", "G5", "G1", "G8", "G13", "G9", "G14", "G15", "G6", "G4", "G2", "G11", "G12", "G7", "G16", "G3"];

pub fn gap_rank(gap: &str) -> usize {
    GAP_RANK.iter().position(|g| *g == gap).unwrap_or(GAP_RANK.len())
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Rec {
    One(String),
    Many(Vec<String>),
}

impl Rec {
    pub fn labels(&self) -> Vec<String> {
        match self {
            Rec::One(s) => vec![s.clone()],
            Rec::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Opt {
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// The instruction rendered into the prompt; the label when absent.
    pub value: Option<String>,
    /// A part of EmbArch this option needs (`PARTS`); hidden without it.
    pub requires: Option<String>,
}

impl Opt {
    pub fn value(&self) -> &str {
        self.value.as_deref().unwrap_or(&self.label)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Question {
    pub id: String,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub ask: String,
    #[serde(rename = "type", default)]
    pub qtype: String,
    #[serde(default)]
    pub options: Vec<Opt>,
    #[serde(default)]
    pub suggest: u8,
    pub fallback: Option<String>,
    pub recommended: Option<Rec>,
    /// When neither asked nor filled: a label or a value; for `multi`, a list
    /// of labels, where `[]` means nothing at all.
    pub default: Option<Rec>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub always_ask: bool,
    #[serde(default)]
    pub sources: Vec<String>,
    pub when: Option<String>,
    #[serde(default)]
    pub gap: String,
    #[serde(default)]
    pub why: String,
    /// A part of EmbArch this question needs (`PARTS`); hidden without it.
    pub requires: Option<String>,
    /// The fragment this question came from, set when it is included.
    #[serde(skip)]
    pub from: Option<String>,
}

impl Question {
    pub fn labels(&self) -> Vec<&str> {
        self.options.iter().map(|o| o.label.as_str()).collect()
    }

    pub fn option(&self, label: &str) -> Option<(usize, &Opt)> {
        self.options.iter().enumerate().find(|(_, o)| o.label == label)
    }

    pub fn recommended(&self) -> Vec<String> {
        self.recommended.as_ref().map(Rec::labels).unwrap_or_default()
    }

    /// Optional: neither required nor asked every run.
    pub fn optional(&self) -> bool {
        !self.required && !self.always_ask
    }

    /// Whether this question shows with these EmbArch parts present.
    pub fn visible(&self, parts: &[String]) -> bool {
        self.requires.as_ref().is_none_or(|r| parts.contains(r))
    }

    /// The labels of the options that show with these parts.
    pub fn visible_labels(&self, parts: &[String]) -> Vec<String> {
        self.effective_options().into_iter().filter(|o| o.requires.as_ref().is_none_or(|r| parts.contains(r))).map(|o| o.label).collect()
    }

    /// `bool` behaves as a two-option choice.
    pub fn effective_options(&self) -> Vec<Opt> {
        if self.qtype == "bool" && self.options.is_empty() {
            return vec![
                Opt { label: "Yes".into(), description: String::new(), value: Some("true".into()), requires: None },
                Opt { label: "No".into(), description: String::new(), value: Some(String::new()), requires: None },
            ];
        }
        self.options.clone()
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct Signals {
    #[serde(default)]
    pub phrases: Vec<String>,
    #[serde(default)]
    pub not: Vec<String>,
    pub desk: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Include {
    pub fragment: String,
    pub recommended: Option<Rec>,
    pub when: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Example {
    pub rough: String,
    #[serde(default)]
    pub context: serde_yaml::Mapping,
    #[serde(default)]
    pub answers: serde_yaml::Mapping,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Front {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub signals: Signals,
    #[serde(default)]
    pub include: Vec<Include>,
    /// Question ids asked before all others, in this order, when they are
    /// asked at all (decision 31: bench state first in a bench session).
    #[serde(default)]
    pub ask_first: Vec<String>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub examples: Vec<Example>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub front: Front,
    pub body: String,
    /// The file it came from (`embedded:<name>` for the built-in set).
    pub file: String,
}

impl Pattern {
    pub fn name(&self) -> &str {
        &self.front.name
    }

    pub fn is_fragment(&self) -> bool {
        self.front.kind == "fragment"
    }
}

/// Split a file into frontmatter and body: `---\n…\n---\n<body>`.
pub fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n").map(|i| (i, i + 5)).or_else(|| rest.strip_suffix("\n---").map(|r| (r.len(), rest.len())))?;
    Some((&rest[..end.0], &rest[end.1..]))
}

pub fn parse(file: &str, text: &str) -> Result<Pattern, String> {
    let (fm, body) = split(text).ok_or("no frontmatter: the file must open with a `---` block")?;
    let front: Front = serde_yaml::from_str(fm).map_err(|e| format!("frontmatter: {e}"))?;
    Ok(Pattern { front, body: body.to_string(), file: file.to_string() })
}

/// A loaded set: task patterns and fragments, each by name, plus the files
/// that did not parse (lint reports them; everything else skips them).
#[derive(Debug, Default)]
pub struct Set {
    pub patterns: BTreeMap<String, Pattern>,
    pub fragments: BTreeMap<String, Pattern>,
    pub broken: Vec<(String, String)>,
}

impl Set {
    fn add(&mut self, file: &str, text: &str) {
        match parse(file, text) {
            Ok(p) if p.is_fragment() => {
                self.fragments.insert(p.name().to_string(), p);
            }
            Ok(p) => {
                self.patterns.insert(p.name().to_string(), p);
            }
            Err(e) => self.broken.push((file.to_string(), e)),
        }
    }

    pub fn embedded() -> Set {
        let mut s = Set::default();
        for (name, text) in EMBEDDED {
            s.add(&format!("embedded:{name}"), text);
        }
        s
    }

    pub fn from_dir(dir: &Path) -> std::io::Result<Set> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        let mut s = Set::default();
        for f in files {
            let text = std::fs::read_to_string(&f)?;
            s.add(&f.display().to_string(), &text);
        }
        Ok(s)
    }

    /// The set a run uses: `PROMPTU_PATTERNS` when set, else the embedded one.
    pub fn load() -> Result<Set, String> {
        match std::env::var_os("PROMPTU_PATTERNS").filter(|v| !v.is_empty()) {
            Some(d) => Set::from_dir(Path::new(&d)).map_err(|e| format!("PROMPTU_PATTERNS={}: {e}", Path::new(&d).display())),
            None => Ok(Set::embedded()),
        }
    }

    pub fn get(&self, name: &str) -> Option<&Pattern> {
        self.patterns.get(name)
    }

    /// The pattern a desk Start work takes by default: the first, in name
    /// order, whose `signals.desk` is `default`.
    pub fn desk_default(&self) -> Option<&str> {
        self.patterns.values().find(|p| p.front.signals.desk.as_deref() == Some("default")).map(|p| p.name())
    }

    /// Whether a pattern may be picked under desk Start work.
    pub fn startable(&self, name: &str) -> bool {
        self.get(name).is_some_and(|p| p.front.signals.desk.as_deref() != Some("never"))
    }

    /// The partials a template may name: each fragment's body.
    pub fn partials(&self) -> BTreeMap<String, String> {
        self.fragments.iter().map(|(k, v)| (k.clone(), v.body.clone())).collect()
    }

    /// A pattern's questions: its own, then each included fragment's, with
    /// the include's `recommended` and `when` applied.
    pub fn questions(&self, p: &Pattern) -> Vec<Question> {
        let mut out = p.front.questions.clone();
        for inc in &p.front.include {
            let Some(f) = self.fragments.get(&inc.fragment) else { continue };
            for q in &f.front.questions {
                let mut q = q.clone();
                q.from = Some(inc.fragment.clone());
                if let Some(r) = &inc.recommended {
                    q.recommended = Some(r.clone());
                }
                if let Some(w) = &inc.when {
                    q.when = Some(w.clone());
                }
                out.push(q);
            }
        }
        out
    }
}

/// A `when` condition (contract §7.1): `flag`, `!flag`, `qid`, `qid == 'L'`,
/// `qid != 'L'`, `qid in ['A','B']`, `qid has 'L'`.
#[derive(Debug, Clone, PartialEq)]
pub struct When {
    pub negate: bool,
    pub name: String,
    pub op: Option<String>,
    pub literals: Vec<String>,
}

pub fn parse_when(s: &str) -> Option<When> {
    let s = s.trim();
    let (negate, s) = match s.strip_prefix('!') {
        Some(r) => (true, r.trim_start()),
        None => (false, s),
    };
    let name_end = s.find(|c: char| !(c.is_ascii_lowercase() || c == '_')).unwrap_or(s.len());
    let name = &s[..name_end];
    if name.is_empty() {
        return None;
    }
    let rest = s[name_end..].trim();
    if rest.is_empty() {
        return Some(When { negate, name: name.into(), op: None, literals: vec![] });
    }
    if negate {
        return None;
    }
    let (op, arg) = ["==", "!=", "in", "has"].iter().find_map(|op| rest.strip_prefix(op).map(|a| (*op, a.trim())))?;
    let mut literals = vec![];
    let mut it = arg.split('\'');
    it.next();
    while let (Some(lit), Some(_)) = (it.next(), it.next()) {
        literals.push(lit.to_string());
    }
    let shape_ok = match op {
        "in" => arg.starts_with('[') && arg.ends_with(']') && !literals.is_empty(),
        _ => literals.len() == 1 && arg.starts_with('\'') && arg.ends_with('\''),
    };
    shape_ok.then(|| When { negate, name: name.into(), op: Some(op.into()), literals })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_set_loads_11_patterns_and_2_fragments() {
        let s = Set::embedded();
        assert!(s.broken.is_empty(), "{:?}", s.broken);
        assert_eq!(s.patterns.len(), 11, "{:?}", s.patterns.keys().collect::<Vec<_>>());
        assert_eq!(s.fragments.len(), 2);
        let imp = s.get("implement-issue").unwrap();
        let qs = s.questions(imp);
        let rules = qs.iter().find(|q| q.id == "rules").unwrap();
        assert_eq!(rules.from.as_deref(), Some("house-rules"));
        assert_eq!(rules.recommended(), vec!["Use another checkout", "Only I open PRs"]);
    }

    #[test]
    fn when_conditions_parse() {
        assert_eq!(parse_when("firmware").unwrap().name, "firmware");
        assert!(parse_when("!hardware").unwrap().negate);
        let w = parse_when("deliverable has 'Draft a reply'").unwrap();
        assert_eq!((w.op.as_deref(), w.literals.clone()), (Some("has"), vec!["Draft a reply".to_string()]));
        let w = parse_when("mode in ['A', 'B c']").unwrap();
        assert_eq!(w.literals, vec!["A", "B c"]);
        assert!(parse_when("mode == Research").is_none());
        assert!(parse_when("!mode == 'x'").is_none());
        assert!(parse_when("").is_none());
    }

    #[test]
    fn frontmatter_splits_at_the_second_rule() {
        assert_eq!(split("---\na: 1\n---\nbody\n"), Some(("a: 1", "body\n")));
        assert_eq!(split("no front"), None);
    }
}
