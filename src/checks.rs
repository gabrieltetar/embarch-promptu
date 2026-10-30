//! The memory checks, T1–T7 on a topic file and I1–I5 on `MEMORY.md`
//! (docs/contract.md §3.4). Pure functions of the text before and after a
//! write: the hook feeds them a write, `promptu memory check` a file as it is.
//!
//! **Only what a write touches is checked.** A topic file's labels (T1–T5)
//! are checked whenever the file is written; an index line only when the
//! write adds or changes it, so a long legacy line never blocks an unrelated
//! write.

use crate::frontmatter;
use chrono::{Days, NaiveDate};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub check: &'static str,
    pub severity: Severity,
    /// 1-based, in the file as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

fn err(check: &'static str, line: Option<usize>, message: String) -> Finding {
    Finding { check, severity: Severity::Error, line, message }
}

fn warn(check: &'static str, line: Option<usize>, message: String) -> Finding {
    Finding { check, severity: Severity::Warning, line, message }
}

/// What the file held before the write.
#[derive(Debug, Clone, Copy)]
pub enum Before<'a> {
    /// No write: `memory check` on a file as it stands. Every line counts as
    /// touched, and the two checks that compare (T7, I3) are skipped.
    Unknown,
    /// The write creates the file.
    Absent,
    Text(&'a str),
}

pub const KINDS: [&str; 5] = ["preference", "rule", "trap", "reference", "status"];
const KIND_LIST: &str = "preference · rule · trap · reference · status";
pub const INDEX_MAX: usize = 150;
pub const REVIEW_MAX_DAYS: u64 = 90;

/// The Claude Code `type` each kind goes with.
pub fn type_for(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "preference" => "user",
        "rule" | "trap" => "feedback",
        "reference" => "reference",
        "status" => "project",
        _ => return None,
    })
}

fn lines(text: &str) -> Vec<&str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect()
}

/// Which lines of `after` the write added or changed: a line of `after` is
/// untouched when an equal line of `before` is left to match it.
pub fn touched(before: Before, after: &str) -> Vec<bool> {
    let new = lines(after);
    let Before::Text(old) = before else { return vec![true; new.len()] };
    let mut left: HashMap<&str, usize> = HashMap::new();
    for l in lines(old) {
        *left.entry(l).or_default() += 1;
    }
    new.iter()
        .map(|l| match left.get_mut(l) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

/// Does `s` start with a `YYYY-MM-DD` date? Sliced with `get`, never `[..]`:
/// the text is the user's, and byte 10 can fall inside a character.
fn is_date_at(s: &str) -> bool {
    s.get(..10).is_some_and(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
}

/// Does this line open an appended update — `Update:`, `**Update …`,
/// `Update 2026-10-01:` or `Updated 2026-10-01`?
pub fn opens_update(line: &str) -> bool {
    let mut s = line.trim_start().trim_start_matches('#').trim_start();
    for m in ["- ", "* ", "+ ", "> "] {
        if let Some(r) = s.strip_prefix(m) {
            s = r.trim_start();
            break;
        }
    }
    let marked = s.starts_with("**") || s.starts_with("__");
    let s = s.trim_start_matches(['*', '_']);
    if let Some(r) = s.strip_prefix("Updated") {
        let r = r.trim_start();
        let r = r.strip_prefix("on ").unwrap_or(r);
        return marked || is_date_at(r);
    }
    let Some(r) = s.strip_prefix("Update") else { return false };
    if marked {
        return r.is_empty() || !r.starts_with(|c: char| c.is_alphanumeric());
    }
    let r = r.trim_start();
    if r.starts_with(':') {
        return true;
    }
    let r = r.strip_prefix('(').unwrap_or(r);
    is_date_at(r) && r.get(10..).is_some_and(|rest| rest.trim_start_matches(')').trim_start().starts_with(':'))
}

/// A `source:` value's format: a URL, a 7–40-hex commit, an absolute path
/// (`/…`, `~/…` or a drive), or a repo-relative path — one token, or one with
/// a `/` if it holds a space, so a sentence is refused.
pub fn valid_source(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains('\n') {
        return false;
    }
    if let Some((scheme, rest)) = s.split_once("://") {
        return !scheme.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            && !rest.is_empty()
            && !rest.contains(char::is_whitespace);
    }
    if (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if s.starts_with('-') {
        return false;
    }
    !s.contains(char::is_whitespace) || s.contains('/')
}

pub fn is_url(s: &str) -> bool {
    s.contains("://")
}

pub fn is_commit(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// T1–T7 on a topic file as `after` would leave it.
pub fn topic(before: Before, after: &str, today: NaiveDate) -> Vec<Finding> {
    let mut out = vec![];
    let front = frontmatter::parse(after);
    let Some(map) = front.map.as_ref() else {
        out.push(err(
            "T1",
            Some(1),
            match &front.error {
                Some(e) => format!("the frontmatter does not parse as YAML ({e}); fix it so `name`, `description` and `kind` can be read"),
                None => "start the file with a `---` frontmatter block holding `name`, `description` and `kind`".into(),
            },
        ));
        updates(before, after, &mut out);
        return out;
    };
    let missing: Vec<&str> = ["name", "description"]
        .into_iter()
        .filter(|k| frontmatter::text(map, k).is_none_or(|v| v.trim().is_empty()))
        .collect();
    if !missing.is_empty() {
        out.push(err("T1", Some(1), format!("the frontmatter needs `{}`", missing.join("` and `"))));
    }

    let kind = kind_of(map, &mut out);
    if let Some(kind) = kind.as_deref() {
        let want = type_for(kind).unwrap_or_default();
        let (top, nested) = frontmatter::both(map, "type");
        for (field, v) in [("type", top), ("metadata.type", nested)] {
            if let Some(t) = v.and_then(serde_yaml::Value::as_str) {
                if t != want {
                    out.push(err("T3", None, format!("kind {kind} goes with {field} {want}, not {t}")));
                }
            }
        }
        if kind == "status" {
            review_by(map, today, &mut out);
        }
    }

    let sources = frontmatter::list(map, "source").unwrap_or_default();
    let sources: Vec<String> = sources.into_iter().filter(|s| !s.trim().is_empty()).collect();
    match (kind.as_deref(), sources.is_empty()) {
        (Some("reference"), true) => out.push(err(
            "T5",
            None,
            "a reference memory needs `source:` — a repo path, an absolute path, a URL or a commit".into(),
        )),
        (Some("trap"), true) => out.push(warn(
            "T5",
            None,
            "a trap memory takes `source:` when there is one — a repo path, an absolute path, a URL or a commit".into(),
        )),
        _ => {}
    }
    for s in sources.iter().filter(|s| !valid_source(s)) {
        out.push(err("T5", None, format!("source `{s}` is not a repo path, an absolute path, a URL or a commit")));
    }

    updates(before, after, &mut out);

    if let Before::Text(old) = before {
        if let Some(old_map) = frontmatter::parse(old).map {
            let now = fields(map);
            let dropped: Vec<String> = fields(&old_map).into_iter().filter(|k| !now.contains(k) && !STAMPED.contains(&k.as_str())).collect();
            if !dropped.is_empty() {
                out.push(err(
                    "T7",
                    None,
                    format!("keep the frontmatter fields you did not write: `{}` was in the file and this write drops it", dropped.join("`, `")),
                ));
            }
        }
    }
    out
}

/// What Claude Code 2.1.287 stamps into `metadata:` on every memory write,
/// whatever the write held [measured 2026-10-29]: dropping one loses nothing.
const STAMPED: [&str; 3] = ["node_type", "originSessionId", "modified"];

/// Frontmatter field names, the level ignored: the harness moves every
/// top-level key but `name` and `description` under `metadata:` on a write
/// [measured 2026-10-29], so `kind` and `metadata.kind` are one field.
fn fields(map: &serde_yaml::Mapping) -> Vec<String> {
    frontmatter::keys(map).into_iter().map(|k| k.strip_prefix("metadata.").map(str::to_string).unwrap_or(k)).collect()
}

/// T2: the kind, when there is exactly one valid one.
fn kind_of(map: &serde_yaml::Mapping, out: &mut Vec<Finding>) -> Option<String> {
    let (top, nested) = frontmatter::both(map, "kind");
    let as_text = |v: Option<&serde_yaml::Value>| {
        v.map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| serde_yaml::to_string(v).unwrap_or_default().trim().to_string()))
    };
    let (top, nested) = (as_text(top), as_text(nested));
    let kind = match (&top, &nested) {
        (None, None) => {
            out.push(err("T2", None, format!("add `kind:` at the top level of the frontmatter: {KIND_LIST}")));
            return None;
        }
        (Some(a), Some(b)) if a != b => {
            out.push(err("T2", None, format!("`kind: {a}` and `metadata.kind: {b}` disagree; keep one, at the top level")));
            return None;
        }
        (Some(k), _) | (None, Some(k)) => k.clone(),
    };
    if !KINDS.contains(&kind.as_str()) {
        out.push(err("T2", None, format!("kind `{kind}` is not one of {KIND_LIST}")));
        return None;
    }
    Some(kind)
}

/// T4: a status memory's review date.
fn review_by(map: &serde_yaml::Mapping, today: NaiveDate, out: &mut Vec<Finding>) {
    let latest = today.checked_add_days(Days::new(REVIEW_MAX_DAYS)).unwrap_or(today);
    let Some(raw) = frontmatter::text(map, "review-by") else {
        out.push(err("T4", None, format!("a status memory needs `review-by: YYYY-MM-DD`, after today and by {latest}")));
        return;
    };
    let Ok(date) = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d") else {
        out.push(err("T4", None, format!("review-by `{raw}` is not a date; write YYYY-MM-DD, after today and by {latest}")));
        return;
    };
    if date <= today {
        out.push(err("T4", None, format!("review-by {date} has passed; check the status is still true and set a date after today, by {latest}")));
    } else if date > latest {
        out.push(err("T4", None, format!("review-by {date} is more than {REVIEW_MAX_DAYS} days ahead; use {latest} or earlier")));
    }
}

/// T6: no added line opens an update paragraph.
fn updates(before: Before, after: &str, out: &mut Vec<Finding>) {
    let mask = touched(before, after);
    for (i, l) in lines(after).iter().enumerate() {
        if mask[i] && opens_update(l) {
            out.push(err("T6", Some(i + 1), "rewrite the memory in place so it says what is true now; do not append an update paragraph".into()));
        }
    }
}

/// An index line, `- [Title](file.md) — the fact`.
pub fn is_index_line(line: &str) -> bool {
    line.trim_start().starts_with("- [")
}

/// The link target of an index line: everything between `](` and the next
/// `)`, trimmed, a leading `./` dropped (contract §2.2). A file name with a
/// space in it is one target.
pub fn link_target(line: &str) -> Option<String> {
    let start = line.find("](")? + 2;
    let end = line[start..].find(')')? + start;
    let t = line[start..end].trim();
    let t = t.strip_prefix("./").unwrap_or(t);
    (!t.is_empty()).then(|| t.to_string())
}

/// The `## ` header in force for each line (outside code fences), and the
/// headers in order with their line index.
pub fn sections(text: &str) -> (Vec<Option<String>>, Vec<(usize, String)>) {
    let mut fence = false;
    let mut current = None;
    let mut per_line = vec![];
    let mut headers = vec![];
    for (i, l) in lines(text).iter().enumerate() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        } else if !fence {
            if let Some(h) = l.strip_prefix("## ") {
                let h = h.trim().to_string();
                headers.push((i, h.clone()));
                current = Some(h);
            }
        }
        per_line.push(current.clone());
    }
    (per_line, headers)
}

/// I1–I5 on `MEMORY.md` as `after` would leave it. `dir` is the memory
/// folder, for I5; without it I5 is skipped.
pub fn index(before: Before, after: &str, dir: Option<&Path>) -> Vec<Finding> {
    let mut out = vec![];
    let all = lines(after);
    let mask = touched(before, after);
    let (in_section, headers) = sections(after);
    // I3 compares with the headers the index already had. Headers are free
    // while it has none — a new index, or one written before domains
    // existed — and checked from then on (decision 21): otherwise block mode
    // could never create or repair a header-less index, since I2 wants every
    // touched line under a header.
    let old_headers: Vec<String> = match before {
        Before::Text(old) => sections(old).1.into_iter().map(|(_, h)| h).collect(),
        Before::Unknown | Before::Absent => vec![],
    };
    if !old_headers.is_empty() {
        for (i, h) in &headers {
            if !old_headers.contains(h) {
                out.push(err(
                    "I3",
                    Some(i + 1),
                    format!("new domain `{h}`: domains are added by the owner, not by a tool write; use one of: {}", old_headers.join(" · ")),
                ));
            }
        }
    }

    let mut targets: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, l) in all.iter().enumerate() {
        if let Some(t) = is_index_line(l).then(|| link_target(l)).flatten() {
            targets.entry(t).or_default().push(i);
        }
    }

    let domains: Vec<&str> = headers.iter().map(|(_, h)| h.as_str()).collect();
    for (i, l) in all.iter().enumerate() {
        if !mask[i] || !is_index_line(l) {
            continue;
        }
        let n = l.chars().count();
        if n > INDEX_MAX {
            out.push(err("I1", Some(i + 1), format!("this index line is {n} characters; the limit is {INDEX_MAX} — keep the fact, cut the words")));
        }
        if in_section[i].is_none() {
            let hint = if domains.is_empty() { String::new() } else { format!(": {}", domains.join(" · ")) };
            out.push(err("I2", Some(i + 1), format!("put this index line under the `## <domain>` header it belongs to{hint}")));
        }
        let Some(t) = link_target(l) else { continue };
        if let Some(others) = targets.get(&t) {
            let elsewhere: Vec<String> = others.iter().filter(|&&j| j != i).map(|j| (j + 1).to_string()).collect();
            if !elsewhere.is_empty() {
                out.push(err("I4", Some(i + 1), format!("`{t}` is already linked on line {}; one index line per memory", elsewhere.join(", "))));
            }
        }
        if let Some(dir) = dir {
            if !is_url(&t) && !t.starts_with('#') && !dir.join(&t).exists() {
                out.push(warn("I5", Some(i + 1), format!("`{t}` is not in the memory folder (yet)")));
            }
        }
    }
    out
}

/// The topic files in `dir` that no index line links: what O1 reports after
/// an index write.
pub fn orphans(dir: &Path, index_text: &str) -> Vec<String> {
    let linked: Vec<String> = lines(index_text).iter().filter(|l| is_index_line(l)).filter_map(|l| link_target(l)).collect();
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".md") && n != "MEMORY.md" && !linked.contains(n))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    const TODAY: &str = "2026-10-29";

    fn ids(f: &[Finding]) -> Vec<&'static str> {
        f.iter().map(|x| x.check).collect()
    }

    fn topic_file(fields: &str) -> String {
        format!("---\nname: cache-trap\ndescription: The build cache keeps stale generated headers\n{fields}---\n\nThe body.\n")
    }

    #[test]
    fn a_well_labelled_file_passes() {
        for fields in [
            "kind: preference\nmetadata:\n  type: user\n",
            "kind: rule\nmetadata:\n  type: feedback\n",
            "kind: trap\nsource: tools/lint.sh\nmetadata:\n  type: feedback\n",
            "kind: reference\nsource: https://example.com/datasheet.pdf\nmetadata:\n  type: reference\n",
            "kind: status\nreview-by: 2026-12-01\nmetadata:\n  type: project\n",
        ] {
            let f = topic(Before::Absent, &topic_file(fields), day(TODAY));
            assert!(f.is_empty(), "{fields}: {f:?}");
        }
    }

    #[test]
    fn the_shape_the_harness_leaves_passes_like_the_top_level_shape() {
        // Claude Code moves every key but name and description under
        // metadata: on a Write and stamps three of its own [measured
        // 2026-10-29]. Every check reads a field at either level.
        let stamped = "  node_type: memory\n  originSessionId: 00000000-0000-4000-8000-000000000001\n  modified: 2026-10-29T02:00:00.000Z\n";
        for (top, moved) in [
            ("kind: preference\nmetadata:\n  type: user\n", "metadata:\n  kind: preference\n  type: user\n"),
            ("kind: rule\nmetadata:\n  type: feedback\n", "metadata:\n  kind: rule\n  type: feedback\n"),
            ("kind: trap\nsource: tools/lint.sh\nmetadata:\n  type: feedback\n", "metadata:\n  kind: trap\n  source: tools/lint.sh\n  type: feedback\n"),
            ("kind: reference\nsource: /opt/vendor/manual.pdf\nmetadata:\n  type: reference\n", "metadata:\n  kind: reference\n  source: /opt/vendor/manual.pdf\n  type: reference\n"),
            ("kind: status\nreview-by: 2026-12-01\nmetadata:\n  type: project\n", "metadata:\n  kind: status\n  review-by: 2026-12-01\n  type: project\n"),
        ] {
            let top = topic_file(top);
            let moved = topic_file(&format!("{moved}{stamped}"));
            for (before, after) in [(Before::Absent, &top), (Before::Absent, &moved), (Before::Text(&top), &moved), (Before::Text(&moved), &top)] {
                let f = topic(before, after, day(TODAY));
                assert!(f.is_empty(), "{after}: {f:?}");
            }
        }
        // And a moved field is still checked where it sits.
        let bad = topic_file("metadata:\n  kind: status\n  review-by: 2026-01-01\n  type: feedback\n");
        assert_eq!(ids(&topic(Before::Absent, &bad, day(TODAY))), ["T3", "T4"]);
        let bad = topic_file("metadata:\n  kind: reference\n  source: the owner told me\n  type: reference\n");
        assert_eq!(ids(&topic(Before::Absent, &bad, day(TODAY))), ["T5"]);
    }

    #[test]
    fn t1_needs_a_parsing_block_with_name_and_description() {
        assert_eq!(ids(&topic(Before::Absent, "just text\n", day(TODAY))), ["T1"]);
        let f = topic(Before::Absent, "---\nname: x\nkind: rule\n---\nbody", day(TODAY));
        assert_eq!(ids(&f), ["T1"]);
        assert!(f[0].message.contains("description"));
        let f = topic(Before::Absent, "---\n- a\nb: c\n---\n", day(TODAY));
        assert_eq!(ids(&f), ["T1"]);
        assert!(f[0].message.contains("does not parse"));
    }

    #[test]
    fn t2_wants_one_of_the_five_kinds() {
        assert_eq!(ids(&topic(Before::Absent, &topic_file(""), day(TODAY))), ["T2"]);
        assert_eq!(ids(&topic(Before::Absent, &topic_file("kind: gotcha\n"), day(TODAY))), ["T2"]);
        assert_eq!(ids(&topic(Before::Absent, &topic_file("kind: rule\nmetadata:\n  kind: trap\n"), day(TODAY))), ["T2"]);
        // Under metadata alone it still counts.
        assert!(topic(Before::Absent, &topic_file("metadata:\n  kind: rule\n  type: feedback\n"), day(TODAY)).is_empty());
    }

    #[test]
    fn t3_holds_kind_and_type_together() {
        let f = topic(Before::Absent, &topic_file("kind: trap\nsource: a/b.c\nmetadata:\n  type: project\n"), day(TODAY));
        assert_eq!(ids(&f), ["T3"]);
        assert!(f[0].message.contains("kind trap goes with metadata.type feedback"));
        assert_eq!(ids(&topic(Before::Absent, &topic_file("kind: rule\ntype: user\n"), day(TODAY))), ["T3"]);
    }

    #[test]
    fn t4_a_status_needs_a_review_date_inside_ninety_days() {
        let t = |rb: &str| ids(&topic(Before::Absent, &topic_file(&format!("kind: status\n{rb}")), day(TODAY)));
        assert_eq!(t(""), ["T4"]);
        assert_eq!(t("review-by: soon\n"), ["T4"]);
        assert_eq!(t("review-by: 2026-10-29\n"), ["T4"]);
        assert_eq!(t("review-by: 2027-01-28\n"), ["T4"]);
        assert!(t("review-by: 2027-01-27\n").is_empty());
        assert!(t("review-by: 2026-10-30\n").is_empty());
        // A body line is not the field.
        let body = topic_file("kind: status\n") + "review-by: 2026-11-15\n";
        assert_eq!(ids(&topic(Before::Absent, &body, day(TODAY))), ["T4"]);
    }

    #[test]
    fn t5_a_reference_needs_a_source_a_trap_is_only_warned() {
        let f = topic(Before::Absent, &topic_file("kind: reference\n"), day(TODAY));
        assert_eq!((f[0].check, f[0].severity), ("T5", Severity::Error));
        let f = topic(Before::Absent, &topic_file("kind: trap\n"), day(TODAY));
        assert_eq!((f[0].check, f[0].severity), ("T5", Severity::Warning));
        let f = topic(Before::Absent, &topic_file("kind: rule\nsource: the owner said so\n"), day(TODAY));
        assert_eq!((f[0].check, f[0].severity), ("T5", Severity::Error));
    }

    #[test]
    fn source_formats() {
        for ok in [
            "src/main.rs",
            "README.md",
            "Makefile",
            "/home/someone/tools/build.sh",
            "~/scratch/notes",
            "/mnt/c/Program Files/Vendor/tool.exe",
            "https://example.com/a?b=c",
            "1a2b3c4",
            "0123456789abcdef0123456789abcdef01234567",
            "../sibling-repo/docs/spec.md",
        ] {
            assert!(valid_source(ok), "{ok}");
        }
        for bad in ["", "the owner told me", "-rf", "see the wiki page", "http:// x", "a\nb"] {
            assert!(!valid_source(bad), "{bad}");
        }
    }

    #[test]
    fn t6_spots_appended_updates_only_on_added_lines() {
        for l in ["Update: it moved", "**Update 2026-10-01:** it moved", "- **Update**: x", "Updated 2026-10-01: x", "_Updated on 2026-10-01_", "Update (2026-10-01): x", "### Update 2026-10-02: x"] {
            assert!(opens_update(l), "{l}");
        }
        for l in ["Updates to the schema go through review.", "Update the index after a rename.", "**Updating** is cheap", "The update path is slow"] {
            assert!(!opens_update(l), "{l}");
        }
        let old = topic_file("kind: rule\n") + "Update: an old one, untouched.\n";
        let new = old.clone() + "**Update 2026-10-29:** a new one.\n";
        let f = topic(Before::Text(&old), &new, day(TODAY));
        assert_eq!(ids(&f), ["T6"]);
        assert_eq!(f[0].line, Some(9));
    }

    #[test]
    fn non_ascii_text_never_panics() {
        // Byte 10 inside a two-byte character, after a date-like start.
        for l in ["Updated 2026-10-0é: x", "Update 2026-10-0é: x", "Update (2026-1ü-01): x", "Updated é", "Update é:", "**Update** — ünïcödé", "2026-10-0é"] {
            let _ = opens_update(l);
        }
        assert!(!is_date_at("2026-10-0é"));
        assert!(!opens_update("Update 2026-10-01 — é: x"));
        assert!(opens_update("Updated 2026-10-01 — é"));
        let body = topic_file("kind: status\nreview-by: 2026-1é-01\n") + "Updated 2026-10-0é then more\nÜpdate: not ours\nUpdated 2026-10-01 — é\n";
        let f = topic(Before::Absent, &body, day(TODAY));
        assert_eq!(ids(&f), ["T4", "T6"]);
        // 20 characters of line, then 130 two-byte ones: 150 characters, 280 bytes.
        let idx = format!("## résumé\n- [Café](café.md) — {}\n", "é".repeat(130));
        assert!(index(Before::Unknown, &idx, None).is_empty());
        let idx = format!("## résumé\n- [Café](café.md) — {}\n", "é".repeat(131));
        assert_eq!(ids(&index(Before::Unknown, &idx, None)), ["I1"]);
    }

    #[test]
    fn t7_refuses_a_dropped_field() {
        let old = topic_file("kind: trap\nsource: a/b.sh\nmetadata:\n  type: feedback\n  originSessionId: abc\n");
        let new = topic_file("kind: trap\nsource: a/b.sh\nmetadata:\n  type: feedback\n");
        // The harness stamps these three again on every write: no loss.
        assert!(topic(Before::Text(&old), &new, day(TODAY)).is_empty());
        let new = topic_file("kind: trap\nmetadata:\n  type: feedback\n");
        let f = topic(Before::Text(&old), &new, day(TODAY));
        assert_eq!(ids(&f), ["T5", "T7"]);
        assert!(f.iter().any(|x| x.check == "T7" && x.message.contains("`source`")), "{f:?}");
        let new = topic_file("kind: trap\nsource: a/b.sh\n");
        assert!(topic(Before::Text(&old), &new, day(TODAY)).iter().any(|x| x.check == "T7" && x.message.contains("`type`")));
        // A rewrite that keeps them all passes.
        assert!(topic(Before::Text(&old), &old.replace("The body.", "The new body."), day(TODAY)).is_empty());
        // The harness moved kind under metadata; writing it back at the top
        // level is not a drop.
        let moved = topic_file("metadata:\n  kind: trap\n  source: a/b.sh\n  type: feedback\n");
        let back = topic_file("kind: trap\nsource: a/b.sh\nmetadata:\n  type: feedback\n");
        assert!(topic(Before::Text(&moved), &back, day(TODAY)).is_empty());
    }

    const INDEX: &str = "## testing\n- [Soak](soak.md) — the soak test runs nightly on the lab machine\n\n## tooling\n- [Lint](lint.md) — run the lint script before every commit\n";

    #[test]
    fn a_clean_index_write_passes_and_legacy_lines_are_left_alone() {
        let long = format!("- [Old](old.md) — {}", "x".repeat(200));
        let old = format!("{INDEX}{long}\n");
        let new = format!("{INDEX}{long}\n- [New](new.md) — a short new fact\n");
        let f = index(Before::Text(&old), &new, None);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn i1_i2_i4_on_touched_lines() {
        let new = format!("- [Top](top.md) — above every header\n{INDEX}- [Soak again](soak.md) — {}\n", "y".repeat(160));
        let f = index(Before::Text(INDEX), &new, None);
        assert_eq!(ids(&f), ["I2", "I1", "I4"]);
        assert_eq!(f[0].line, Some(1));
        assert!(f[2].message.contains("line 3"));
    }

    #[test]
    fn i3_refuses_a_new_domain_and_names_the_known_ones() {
        let new = format!("{INDEX}\n## release\n- [Freeze](freeze.md) — the release branch freezes a week before a tag\n");
        let f = index(Before::Text(INDEX), &new, None);
        assert_eq!(ids(&f), ["I3"]);
        assert!(f[0].message.contains("use one of: testing · tooling"));
        // A standalone check has no before to compare.
        assert!(index(Before::Unknown, INDEX, None).is_empty());
    }

    #[test]
    fn headers_are_free_until_the_index_has_some() {
        // Creating the index: its headers are the first domains.
        assert!(index(Before::Absent, INDEX, None).is_empty());
        // An index written before domains existed: the write that adds them
        // and moves the lines under them goes through...
        let legacy = "- [Soak](soak.md) — x\n- [Lint](lint.md) — y\n";
        assert!(index(Before::Text(legacy), INDEX, None).is_empty());
        // ...while touching a line and leaving it under no header is still I2.
        let touched_only = "- [Soak](soak.md) — x, changed\n- [Lint](lint.md) — y\n";
        assert_eq!(ids(&index(Before::Text(legacy), touched_only, None)), ["I2"]);
        // Once there are headers, a new one is refused.
        let more = format!("{INDEX}## later\n");
        assert_eq!(ids(&index(Before::Text(INDEX), &more, None)), ["I3"]);
    }

    #[test]
    fn i5_warns_on_a_link_to_nothing() {
        let dir = std::env::temp_dir().join(format!("promptu-i5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("soak.md"), "x").unwrap();
        let f = index(Before::Unknown, INDEX, Some(&dir));
        assert_eq!(ids(&f), ["I5"]);
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(orphans(&dir, "## a\n- [Other](other.md) — x\n"), ["soak.md"]);
        assert!(orphans(&dir, INDEX).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_link_target_runs_to_the_next_parenthesis() {
        assert_eq!(link_target("- [Notes](my notes.md) — x").as_deref(), Some("my notes.md"));
        assert_eq!(link_target("- [A](./a.md) — x").as_deref(), Some("a.md"));
        assert_eq!(link_target("- [A]( a.md ) — x").as_deref(), Some("a.md"));
        assert_eq!(link_target("- [A]() — x"), None);
        assert_eq!(link_target("- [A] no link"), None);
        // Two files whose names differ after a space are two targets.
        let idx = "## a\n- [One](field notes.md) — x\n- [Two](field log.md) — y\n";
        assert!(index(Before::Unknown, idx, None).is_empty());
    }

    #[test]
    fn a_header_inside_a_code_fence_is_not_a_domain() {
        let (per, heads) = sections("## a\n```\n## not\n```\n- [x](x.md)\n");
        assert_eq!(heads.len(), 1);
        assert_eq!(per[4].as_deref(), Some("a"));
    }

    #[test]
    fn touched_marks_added_and_changed_lines() {
        assert_eq!(touched(Before::Text("a\nb\nc"), "a\nB\nc\nd"), [false, true, false, true]);
        assert_eq!(touched(Before::Absent, "a\nb"), [true, true]);
    }
}
