//! Logic-less Mustache, the subset patterns use (docs/contract.md §7.2):
//! `{{x}}`, `{{{x}}}` (the same: the output is Markdown, nothing is
//! escaped), sections `{{#x}}…{{/x}}`, inverted sections `{{^x}}…{{/x}}`,
//! list loops with `{{.}}`, dotted names and partials `{{> name}}`. A line
//! holding only a section tag leaves no line behind. Then a tidy pass drops
//! empty `## ` sections, collapses blank runs and closes gaps inside lists.

use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn tok() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\{\{\{\s*([\w.]+)\s*\}\}\}|\{\{\s*([#^/>]?)\s*([\w.\-]+)\s*\}\}").unwrap())
}

fn standalone() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*\{\{\s*[#^/]\s*[\w.\-]+\s*\}\}\s*$").unwrap())
}

/// Every tag a template names: `(kind, name)`, kind `""`, `#`, `^`, `/`, `>`
/// or `{` for a triple mustache.
pub fn tags(tpl: &str) -> Vec<(String, String)> {
    tok()
        .captures_iter(tpl)
        .map(|c| match c.get(1) {
            Some(n) => ("{".to_string(), n.as_str().to_string()),
            None => (c[2].to_string(), c[3].to_string()),
        })
        .collect()
}

fn lookup<'a>(stack: &'a [&'a Value], name: &str) -> Option<&'a Value> {
    if name == "." {
        return stack.last().copied();
    }
    for ctx in stack.iter().rev() {
        let mut cur = *ctx;
        let mut ok = true;
        for part in name.split('.') {
            match cur.get(part) {
                Some(v) => cur = v,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Some(cur);
        }
    }
    None
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    }
}

fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => if *b { "true".into() } else { String::new() },
        Some(other) => other.to_string(),
    }
}

/// Drop the line break of every line that holds only a section tag.
fn strip_standalone(tpl: &str) -> String {
    let mut out = String::with_capacity(tpl.len());
    let lines: Vec<&str> = tpl.split('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        if standalone().is_match(line) {
            out.push_str(line.trim());
        } else {
            out.push_str(line);
            if i + 1 < lines.len() {
                out.push('\n');
            }
        }
    }
    out
}

pub fn render(tpl: &str, ctx: &Value, partials: &BTreeMap<String, String>) -> Result<String, String> {
    render_stack(tpl, &[ctx], partials, 0)
}

fn render_stack(tpl: &str, stack: &[&Value], partials: &BTreeMap<String, String>, depth: usize) -> Result<String, String> {
    if depth > 16 {
        return Err("partials nest more than 16 deep".into());
    }
    let tpl = strip_standalone(tpl);
    inner(&tpl, stack, partials, depth)
}

fn inner(tpl: &str, stack: &[&Value], partials: &BTreeMap<String, String>, depth: usize) -> Result<String, String> {
    let mut out = String::new();
    let mut i = 0;
    while let Some(m) = tok().captures_at(tpl, i) {
        let whole = m.get(0).unwrap();
        out.push_str(&tpl[i..whole.start()]);
        if let Some(n) = m.get(1) {
            out.push_str(&text(lookup(stack, n.as_str())));
            i = whole.end();
            continue;
        }
        let kind = &m[2];
        let name = &m[3];
        match kind {
            "" => {
                out.push_str(&text(lookup(stack, name)));
                i = whole.end();
            }
            ">" => {
                let p = partials.get(name).ok_or_else(|| format!("unknown partial `{name}`"))?;
                out.push_str(&render_stack(p, stack, partials, depth + 1)?);
                i = whole.end();
            }
            "#" | "^" => {
                // Find the matching close, counting nested opens of the same name.
                let mut level = 1;
                let mut j = whole.end();
                let close = loop {
                    let n = tok().captures_at(tpl, j).ok_or_else(|| format!("unclosed section `{name}`"))?;
                    let nw = n.get(0).unwrap();
                    if n.get(3).map(|x| x.as_str()) == Some(name) {
                        match n.get(2).map(|x| x.as_str()) {
                            Some("#") | Some("^") => level += 1,
                            Some("/") => level -= 1,
                            _ => {}
                        }
                    }
                    j = nw.end();
                    if level == 0 {
                        break nw;
                    }
                };
                let body = &tpl[whole.end()..close.start()];
                let v = lookup(stack, name);
                if kind == "^" {
                    if !truthy(v) {
                        out.push_str(&inner(body, stack, partials, depth)?);
                    }
                } else if truthy(v) {
                    match v {
                        Some(Value::Array(items)) => {
                            for it in items {
                                let mut s: Vec<&Value> = stack.to_vec();
                                s.push(it);
                                out.push_str(&inner(body, &s, partials, depth)?);
                            }
                        }
                        Some(obj @ Value::Object(_)) => {
                            let mut s: Vec<&Value> = stack.to_vec();
                            s.push(obj);
                            out.push_str(&inner(body, &s, partials, depth)?);
                        }
                        _ => out.push_str(&inner(body, stack, partials, depth)?),
                    }
                }
                i = close.end();
            }
            _ => return Err(format!("a close `{{{{/{name}}}}}` with no open")),
        }
    }
    out.push_str(&tpl[i..]);
    Ok(out)
}

/// The post-pass: drop a `## ` heading whose section is empty, collapse runs
/// of blank lines, close single blank lines between list items, trim.
pub fn tidy(text: &str) -> String {
    // Sections: text before the first heading, then each heading with its body.
    let mut parts: Vec<(Option<&str>, String)> = vec![(None, String::new())];
    for line in text.split_inclusive('\n') {
        if line.starts_with("## ") {
            parts.push((Some(line), String::new()));
        } else {
            parts.last_mut().unwrap().1.push_str(line);
        }
    }
    let mut kept = String::new();
    for (h, body) in parts {
        match h {
            None => kept.push_str(&body),
            Some(h) if !body.trim().is_empty() => {
                kept.push_str(h);
                kept.push_str(&body);
            }
            Some(_) => {}
        }
    }
    // Collapse 3+ newlines to 2.
    let mut collapsed = String::with_capacity(kept.len());
    let mut nl = 0;
    for c in kept.chars() {
        if c == '\n' {
            nl += 1;
            if nl <= 2 {
                collapsed.push(c);
            }
        } else {
            nl = 0;
            collapsed.push(c);
        }
    }
    let t = collapsed.trim();
    // Close a single blank line between two list items.
    let lines: Vec<&str> = t.split('\n').collect();
    let mut out: Vec<&str> = vec![];
    for (i, l) in lines.iter().enumerate() {
        let gap = l.is_empty() && i > 0 && i + 1 < lines.len() && lines[i - 1].starts_with("- ") && lines[i + 1].starts_with("- ");
        if !gap {
            out.push(l);
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn r(tpl: &str, ctx: Value) -> String {
        render(tpl, &ctx, &BTreeMap::new()).unwrap()
    }

    #[test]
    fn variables_sections_lists_and_dotted_names() {
        assert_eq!(r("a {{x}} {{{y}}}", json!({"x": "1", "y": "<2>"})), "a 1 <2>");
        assert_eq!(r("{{#x}}yes{{/x}}{{^x}}no{{/x}}", json!({"x": ""})), "no");
        assert_eq!(r("{{#l}}- {{.}}\n{{/l}}", json!({"l": ["a", "b"]})), "- a\n- b\n");
        assert_eq!(r("{{#item.key}}{{item.key}}: {{item.title}}{{/item.key}}", json!({"item": {"key": "K-1", "title": "T"}})), "K-1: T");
        assert_eq!(r("{{missing}}.", json!({})), ".");
        assert_eq!(r("{{#o}}{{k}}{{/o}}", json!({"o": {"k": "v"}})), "v");
    }

    #[test]
    fn a_standalone_section_line_leaves_no_line() {
        let tpl = "A\n{{#x}}\nB\n{{/x}}\nC\n";
        assert_eq!(r(tpl, json!({"x": true})), "A\nB\nC\n");
        assert_eq!(r(tpl, json!({})), "A\nC\n");
    }

    #[test]
    fn partials_render_in_the_callers_context() {
        let mut p = BTreeMap::new();
        p.insert("frag".to_string(), "{{#rules}}\n- {{.}}\n{{/rules}}\n".to_string());
        let out = render("X\n{{> frag}}\nY", &json!({"rules": ["r1"]}), &p).unwrap();
        assert_eq!(out, "X\n- r1\n\nY");
        assert!(render("{{> nope}}", &json!({}), &p).is_err());
        assert!(render("{{#a}}open", &json!({}), &p).is_err());
        assert!(render("{{/a}}", &json!({}), &p).is_err());
    }

    #[test]
    fn tidy_drops_empty_sections_and_list_gaps() {
        let t = "## Goal\nDo it\n\n## Context\n\n\n## Constraints\n- a\n\n- b\n\n\n\nend\n";
        assert_eq!(tidy(t), "## Goal\nDo it\n\n## Constraints\n- a\n- b\n\nend");
    }

    #[test]
    fn tags_lists_every_slot() {
        let t = tags("{{a}} {{{b}}} {{#c.d}}{{/c.d}} {{> e-f}}");
        assert_eq!(t, vec![("".into(), "a".into()), ("{".into(), "b".into()), ("#".into(), "c.d".into()), ("/".into(), "c.d".into()), (">".into(), "e-f".into())]);
    }
}
