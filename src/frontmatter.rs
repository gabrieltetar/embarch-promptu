//! A memory file's frontmatter, read the way Claude Code 2.1.287 reads it, so
//! the hook never calls a file broken that Claude Code itself accepts:
//!
//! - the block is `---`, then everything up to the next `---` (Claude Code's
//!   own `^---\s*\n([\s\S]*?)---\s*\n?`);
//! - it is YAML; when that fails, each top-level `key: value` whose value holds
//!   one of `{}[]*&#!|>%@`` ` or `": "` is double-quoted, leading tabs become
//!   two spaces each, and it is parsed once more. One real memory file in 75
//!   needs that second pass (an unquoted `: ` inside its description).

use serde_yaml::{Mapping, Value};

#[derive(Debug, Clone, Default)]
pub struct Front {
    /// A `---` block was found at the top of the file.
    pub present: bool,
    /// Its keys, when it parsed to a mapping (an empty block is an empty map).
    pub map: Option<Mapping>,
    /// Why it did not parse.
    pub error: Option<String>,
}

fn normalize(text: &str) -> String {
    text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n")
}

/// The raw block between the fences.
fn block(text: &str) -> Option<String> {
    let rest = text.strip_prefix("---")?;
    // `\s*\n`: the opening may be followed by whitespace, the last newline in
    // that run ends it.
    let ws_len = rest.len() - rest.trim_start().len();
    let nl = rest[..ws_len].rfind('\n')?;
    let start = 3 + nl + 1;
    let close = text[start..].find("---")? + start;
    Some(text[start..close].to_string())
}

fn quote_lossy(fm: &str) -> String {
    fm.split('\n')
        .map(|line| {
            let Some((key, value)) = split_key(line) else { return line.to_string() };
            let quoted = (value.starts_with('"') && value.ends_with('"') && value.len() > 1)
                || (value.starts_with('\'') && value.ends_with('\'') && value.len() > 1);
            if quoted {
                return line.to_string();
            }
            if value.starts_with('[') && value.ends_with(']') && matches!(serde_yaml::from_str::<Value>(value), Ok(Value::Sequence(_))) {
                return line.to_string();
            }
            let special = value.contains(": ") || value.chars().any(|c| "{}[]*&#!|>%@`".contains(c));
            if !special {
                return line.to_string();
            }
            format!("{key}: \"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        })
        .map(|line| {
            let tabs = line.len() - line.trim_start_matches('\t').len();
            format!("{}{}", "  ".repeat(tabs), &line[tabs..])
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `^([a-zA-Z_-]+):\s+(.+)$` on one line.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic() || c == '_' || c == '-') {
        return None;
    }
    let after = &line[colon + 1..];
    let value = after.trim_start();
    if value.len() == after.len() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

fn to_map(v: Value) -> Mapping {
    match v {
        Value::Mapping(m) => m,
        _ => Mapping::new(),
    }
}

pub fn parse(text: &str) -> Front {
    let text = normalize(text);
    let Some(inner) = block(&text) else { return Front::default() };
    let mut front = Front { present: true, ..Default::default() };
    if inner.trim().is_empty() {
        front.map = Some(Mapping::new());
        return front;
    }
    match serde_yaml::from_str::<Value>(&inner) {
        Ok(v) => front.map = Some(to_map(v)),
        Err(_) => match serde_yaml::from_str::<Value>(&quote_lossy(&inner)) {
            Ok(v) => front.map = Some(to_map(v)),
            Err(e) => front.error = Some(e.to_string().lines().next().unwrap_or("does not parse").to_string()),
        },
    }
    front
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `key` at the top level, and under `metadata:`.
pub fn both<'a>(map: &'a Mapping, key: &str) -> (Option<&'a Value>, Option<&'a Value>) {
    let top = map.get(key);
    let nested = map.get("metadata").and_then(Value::as_mapping).and_then(|m| m.get(key));
    (top, nested)
}

/// The value of `key`, top level first, then under `metadata:`, as text.
pub fn text(map: &Mapping, key: &str) -> Option<String> {
    let (top, nested) = both(map, key);
    top.or(nested).and_then(scalar)
}

/// `source:` as a list: one value, or a YAML list of them.
pub fn list(map: &Mapping, key: &str) -> Option<Vec<String>> {
    let (top, nested) = both(map, key);
    match top.or(nested)? {
        Value::Sequence(s) => Some(s.iter().filter_map(scalar).collect()),
        Value::Null => None,
        other => scalar(other).map(|s| vec![s]),
    }
}

/// Every key, top level and `metadata.<key>`, in file order.
pub fn keys(map: &Mapping) -> Vec<String> {
    let mut out = vec![];
    for (k, v) in map {
        let Some(k) = scalar(k) else { continue };
        if k == "metadata" {
            if let Some(m) = v.as_mapping() {
                out.extend(m.keys().filter_map(scalar).map(|n| format!("metadata.{n}")));
                continue;
            }
        }
        out.push(k);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_block_parses_with_its_metadata() {
        let f = parse("---\nname: x\ndescription: y\nkind: trap\nmetadata:\n  type: feedback\n---\n\nbody\n");
        assert!(f.present);
        let m = f.map.unwrap();
        assert_eq!(text(&m, "kind").as_deref(), Some("trap"));
        assert_eq!(text(&m, "type").as_deref(), Some("feedback"));
        assert_eq!(keys(&m), ["name", "description", "kind", "metadata.type"]);
    }

    #[test]
    fn an_unquoted_colon_space_parses_on_the_second_pass_as_claude_codes_does() {
        let f = parse("---\nname: x\ndescription: In a repo: the rule is this\nmetadata:\n  type: feedback\n---\nbody");
        assert!(f.error.is_none(), "{:?}", f.error);
        assert_eq!(text(f.map.as_ref().unwrap(), "description").as_deref(), Some("In a repo: the rule is this"));
    }

    #[test]
    fn a_broken_block_reports_why_and_a_missing_one_is_absent() {
        let f = parse("---\nname: [unclosed\n  - : :\n---\n");
        assert!(f.present && f.map.is_none() && f.error.is_some());
        let f = parse("no frontmatter here\n");
        assert!(!f.present);
        // No closing --- at all: Claude Code sees no frontmatter either.
        assert!(!parse("---\nname: x\n").present);
    }

    #[test]
    fn crlf_a_bom_and_a_scalar_block_are_handled() {
        let f = parse("\u{feff}---\r\nname: x\r\ndescription: y\r\n---\r\nbody");
        assert_eq!(text(f.map.as_ref().unwrap(), "name").as_deref(), Some("x"));
        let f = parse("---\njust a string\n---\n");
        assert!(f.map.unwrap().is_empty());
    }

    #[test]
    fn a_source_can_be_one_value_or_a_list() {
        let m = parse("---\nsource: a/b.md\n---\n").map.unwrap();
        assert_eq!(list(&m, "source"), Some(vec!["a/b.md".to_string()]));
        let m = parse("---\nsource:\n  - a/b.md\n  - https://x.example/y\n---\n").map.unwrap();
        assert_eq!(list(&m, "source").unwrap().len(), 2);
        let m = parse("---\nmetadata:\n  source: 1a2b3c4\n---\n").map.unwrap();
        assert_eq!(list(&m, "source"), Some(vec!["1a2b3c4".to_string()]));
    }
}
