//! The footer (docs/contract.md §6): the last line of a rendered prompt, and
//! the only record a run leaves.
//!
//! ```text
//! — promptu bench-session@3 asked=board:other,pass:rec filled=flash:memory.de8cea skipped=timebox assume=heavy
//! ```

use regex::Regex;
use std::sync::OnceLock;

pub const LEAD: &str = "— promptu ";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Footer {
    /// `None` for `— promptu none`.
    pub pattern: Option<(String, u32)>,
    /// `(qid, code)`: `rec`, `all`, or `o1+o3`, `other` …
    pub asked: Vec<(String, String)>,
    /// `(qid, source, changed)`: `prompt`, `desk`, `last`, `context` or `memory.<id>`.
    pub filled: Vec<(String, String, bool)>,
    pub skipped: Vec<String>,
    pub assume: Vec<String>,
    pub pick_owner: bool,
    /// Fields after `pick` (§11: new fields go there): `mode=brief`.
    pub extra: Vec<(String, String)>,
}

impl Footer {
    pub fn none() -> Footer {
        Footer::default()
    }

    pub fn format(&self) -> String {
        let Some((p, v)) = &self.pattern else { return format!("{LEAD}none") };
        let mut s = format!("{LEAD}{p}@{v}");
        if !self.asked.is_empty() {
            s.push_str(" asked=");
            s.push_str(&self.asked.iter().map(|(q, c)| format!("{q}:{c}")).collect::<Vec<_>>().join(","));
        }
        if !self.filled.is_empty() {
            s.push_str(" filled=");
            s.push_str(&self.filled.iter().map(|(q, src, ch)| format!("{q}:{src}{}", if *ch { "!" } else { "" })).collect::<Vec<_>>().join(","));
        }
        if !self.skipped.is_empty() {
            s.push_str(&format!(" skipped={}", self.skipped.join(",")));
        }
        if !self.assume.is_empty() {
            s.push_str(&format!(" assume={}", self.assume.join(",")));
        }
        if self.pick_owner {
            s.push_str(" pick=owner");
        }
        for (k, v) in &self.extra {
            s.push_str(&format!(" {k}={v}"));
        }
        s
    }
}

fn grammar() -> &'static [Regex; 6] {
    static R: OnceLock<[Regex; 6]> = OnceLock::new();
    R.get_or_init(|| {
        let qid = "[a-z_]+";
        let choice = "(?:o[0-9]+|other)";
        [
            Regex::new(r"^[a-z0-9-]{1,64}@[0-9]+$").unwrap(),
            Regex::new(&format!(r"^{qid}:(?:rec|all|{choice}(?:\+{choice})*)$")).unwrap(),
            Regex::new(&format!(r"^{qid}:(?:prompt|desk|last|context|memory\.(?:[0-9a-f]{{6}}|claude-md))!?$")).unwrap(),
            Regex::new(&format!(r"^{qid}$")).unwrap(),
            Regex::new(r"^[a-z]+$").unwrap(),
            Regex::new(r"^\S+$").unwrap(),
        ]
    })
}

/// Parse a footer line; `Err` names the first part that breaks the grammar.
/// Unknown `key=value` fields after the known ones are kept in `extra`.
pub fn parse(line: &str) -> Result<Footer, String> {
    let rest = line.trim_end().strip_prefix(LEAD).ok_or("does not start with `— promptu `")?;
    if rest == "none" {
        return Ok(Footer::none());
    }
    let g = grammar();
    let mut it = rest.split(' ');
    let head = it.next().unwrap_or_default();
    if !g[0].is_match(head) {
        return Err(format!("`{head}` is not <pattern>@<version>"));
    }
    let (p, v) = head.split_once('@').unwrap();
    let mut f = Footer { pattern: Some((p.to_string(), v.parse().map_err(|_| "version")?)), ..Default::default() };
    let order = ["asked", "filled", "skipped", "assume", "pick"];
    let mut last = 0;
    for field in it {
        let (k, v) = field.split_once('=').ok_or_else(|| format!("`{field}` is not key=value"))?;
        let pos = order.iter().position(|o| *o == k);
        match pos {
            Some(p) if p < last && last <= order.len() => return Err(format!("`{k}` is out of order")),
            Some(p) => last = p + 1,
            None => last = order.len() + 1,
        }
        match k {
            "asked" => {
                for a in v.split(',') {
                    if !g[1].is_match(a) {
                        return Err(format!("asked `{a}`"));
                    }
                    let (q, c) = a.split_once(':').unwrap();
                    f.asked.push((q.into(), c.into()));
                }
            }
            "filled" => {
                for a in v.split(',') {
                    if !g[2].is_match(a) {
                        return Err(format!("filled `{a}`"));
                    }
                    let (q, s) = a.split_once(':').unwrap();
                    let ch = s.ends_with('!');
                    f.filled.push((q.into(), s.trim_end_matches('!').into(), ch));
                }
            }
            "skipped" | "assume" => {
                for q in v.split(',') {
                    if !g[3].is_match(q) {
                        return Err(format!("{k} `{q}`"));
                    }
                    if k == "skipped" { f.skipped.push(q.into()) } else { f.assume.push(q.into()) }
                }
            }
            "pick" => {
                if v != "owner" {
                    return Err(format!("pick `{v}`"));
                }
                f.pick_owner = true;
            }
            other => {
                if !g[4].is_match(other) || !g[5].is_match(v) {
                    return Err(format!("field `{field}`"));
                }
                f.extra.push((other.into(), v.into()));
            }
        }
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contracts_example_round_trips() {
        let line = "— promptu bench-session@3 asked=board:other,attached:o1+o3,pass:rec,gate:rec filled=flash:memory.de8cea,unblocks:desk! skipped=timebox assume=heavy";
        let f = parse(line).unwrap();
        assert_eq!(f.pattern, Some(("bench-session".into(), 3)));
        assert_eq!(f.asked[1], ("attached".into(), "o1+o3".into()));
        assert_eq!(f.filled[1], ("unblocks".into(), "desk".into(), true));
        assert_eq!(f.format(), line);
        assert_eq!(parse("— promptu none").unwrap().format(), "— promptu none");
    }

    #[test]
    fn new_fields_go_after_pick_and_bad_ones_fail() {
        let f = parse("— promptu handoff@1 asked=reader:rec pick=owner mode=brief").unwrap();
        assert!(f.pick_owner);
        assert_eq!(f.extra, vec![("mode".to_string(), "brief".to_string())]);
        assert_eq!(f.format(), "— promptu handoff@1 asked=reader:rec pick=owner mode=brief");
        assert!(parse("— promptu x@1 asked=a:maybe").is_err());
        assert!(parse("— promptu x@1 filled=a:memory.XYZ").is_err());
        assert!(parse("— promptu x@1 skipped=a asked=b:rec").is_err());
        assert!(parse("— promptu X@1").is_err());
        assert!(parse("promptu x@1").is_err());
        assert!(parse("— promptu x@1 filled=a:memory.claude-md,b:last").is_ok());
    }
}
