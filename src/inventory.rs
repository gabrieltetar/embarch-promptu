//! `promptu memory [--json]`: every memory that loads in a directory, grouped
//! scope → domain → kind, each with its stale flags (docs/contract.md §5).
//! What the memory menu draws; read-only.
//!
//! Scopes, in load order: **user** (the config home's `CLAUDE.md` and
//! `rules/`), **auto** (this project's auto-memory folder: `MEMORY.md` and its
//! topic files), **project** (the `CLAUDE.md` files of this folder and its
//! ancestors, and the checkout's `.claude/rules/`). A domain is a `## ` header:
//! the index's for auto memory, a file's own sections for a `CLAUDE.md`.

use crate::checks::{self, KINDS};
use crate::claude;
use crate::frontmatter;
use crate::sha1::memory_id;
use chrono::NaiveDate;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub promptu: u32,
    pub cwd: String,
    /// The main checkout this folder belongs to: what auto memory is keyed on.
    pub project_root: String,
    pub today: String,
    /// The hook's mode on this machine: `warn`, `block` or `off`.
    pub hook: &'static str,
    pub memory: MemoryDir,
    pub scopes: Vec<Scope>,
    pub entries: Vec<Entry>,
    pub counts: Counts,
    /// Whether `used` was filled from footers. Always false before step 2.
    pub used_scanned: bool,
}

#[derive(Debug, Serialize)]
pub struct MemoryDir {
    pub path: Option<String>,
    /// `default`, `env:<VAR>` or `setting:<file>`.
    pub chosen_by: String,
    pub exists: bool,
    pub index: Option<String>,
    /// Where a relative `source:` path is resolved: the main checkout the
    /// folder's name encodes, else `project_root`.
    pub source_root: String,
}

#[derive(Debug, Serialize)]
pub struct Scope {
    pub id: &'static str,
    pub label: &'static str,
    pub files: Vec<String>,
    /// Domains in order: the `## ` headers.
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub scope: &'static str,
    pub domain: Option<String>,
    pub kind: Option<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// The footer's id: six hex digits for a topic file, `claude-md` for a
    /// `CLAUDE.md` section.
    pub id: String,
    pub file: String,
    pub title: String,
    /// The index line, or a section's first line.
    pub line: Option<String>,
    pub line_no: Option<usize>,
    pub chars: Option<usize>,
    pub modified: Option<String>,
    pub age_days: Option<i64>,
    pub review_by: Option<String>,
    pub source: Vec<String>,
    pub flags: Vec<&'static str>,
    pub stale: bool,
    /// Footers naming this entry, newest first. Empty until step 2.
    pub used: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub entries: usize,
    pub stale: usize,
    pub by_scope: BTreeMap<&'static str, usize>,
    pub by_kind: BTreeMap<String, usize>,
    pub by_flag: BTreeMap<&'static str, usize>,
}

fn show(p: &Path) -> String {
    p.display().to_string()
}

pub fn build(cwd: &Path, today: NaiveDate) -> Inventory {
    let root = claude::project_root(cwd);
    let mem = claude::auto_memory_dir(cwd);
    let mem_path = mem.as_ref().map(|(p, _)| p.clone());
    let source_root = mem_path
        .as_ref()
        .filter(|p| p.parent().and_then(Path::parent).zip(claude::memory_base()).is_some_and(|(a, b)| a == b.join("projects")))
        .and_then(|p| p.parent()?.file_name()?.to_str().and_then(claude::decode_folder))
        .unwrap_or_else(|| root.clone());

    let mut entries = vec![];
    let mut scopes = vec![];

    let user_files = user_files();
    scopes.push(claude_md_scope("user", "User", &user_files, &mut entries));

    let mut auto = Scope { id: "auto", label: "Auto memory", files: vec![], domains: vec![] };
    if let Some(dir) = mem_path.as_ref().filter(|d| d.is_dir()) {
        auto_entries(dir, &source_root, today, &mut auto, &mut entries);
    }
    scopes.push(auto);

    let project_files = project_files(cwd);
    scopes.push(claude_md_scope("project", "Project", &project_files, &mut entries));

    let mut counts = Counts { entries: entries.len(), ..Default::default() };
    for e in &entries {
        *counts.by_scope.entry(e.scope).or_default() += 1;
        if let Some(k) = &e.kind {
            *counts.by_kind.entry(k.clone()).or_default() += 1;
        }
        for f in &e.flags {
            *counts.by_flag.entry(f).or_default() += 1;
        }
        counts.stale += e.stale as usize;
    }

    Inventory {
        promptu: 1,
        cwd: show(cwd),
        project_root: show(&root),
        today: today.to_string(),
        hook: crate::config::hook_mode().name(),
        memory: MemoryDir {
            exists: mem_path.as_ref().is_some_and(|p| p.is_dir()),
            index: mem_path.as_ref().map(|p| p.join("MEMORY.md")).filter(|p| p.is_file()).map(|p| show(&p)),
            path: mem_path.as_ref().map(|p| show(p)),
            chosen_by: mem.as_ref().map(|(_, s)| s.describe()).unwrap_or_else(|| "none".into()),
            source_root: show(&source_root),
        },
        scopes,
        entries,
        counts,
        used_scanned: false,
    }
}

fn rules_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            rules_under(&p, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
}

fn user_files() -> Vec<PathBuf> {
    let Some(c) = claude::config_dir() else { return vec![] };
    let mut out: Vec<PathBuf> = [c.join("CLAUDE.md")].into_iter().filter(|p| p.is_file()).collect();
    rules_under(&c.join("rules"), &mut out);
    out
}

/// The `CLAUDE.md` files that load for `cwd`: each ancestor's, outermost
/// first, then the checkout's `.claude/rules/`. Imports are not followed.
fn project_files(cwd: &Path) -> Vec<PathBuf> {
    let config = claude::config_dir();
    let mut dirs: Vec<&Path> = cwd.ancestors().collect();
    dirs.reverse();
    let mut out = vec![];
    for d in dirs {
        let own = config.as_deref() == Some(d.join(".claude").as_path());
        for name in ["CLAUDE.md", "CLAUDE.local.md"] {
            out.push(d.join(name));
        }
        if !own {
            out.push(d.join(".claude").join("CLAUDE.md"));
        }
    }
    out.retain(|p| p.is_file());
    let checkout = claude::git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    if config.as_deref() != Some(checkout.join(".claude").as_path()) {
        rules_under(&checkout.join(".claude").join("rules"), &mut out);
    }
    out
}

fn claude_md_scope(id: &'static str, label: &'static str, files: &[PathBuf], entries: &mut Vec<Entry>) -> Scope {
    let mut scope = Scope { id, label, files: files.iter().map(|p| show(p)).collect(), domains: vec![] };
    for f in files {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let (_, headers) = checks::sections(&text);
        let lines: Vec<&str> = text.lines().collect();
        let first_after = |from: usize, to: usize| {
            (from..to).find(|&i| {
                let t = lines[i].trim();
                !t.is_empty() && !t.starts_with('#')
            })
        };
        let base = |domain: Option<String>, title: String, at: Option<usize>| Entry {
            scope: id,
            domain,
            kind: None,
            type_: None,
            id: "claude-md".into(),
            file: show(f),
            title,
            line: at.map(|i| clip(lines[i].trim(), 200)),
            line_no: at.map(|i| i + 1),
            chars: None,
            modified: None,
            age_days: None,
            review_by: None,
            source: vec![],
            flags: vec![],
            stale: false,
            used: vec![],
        };
        let first_header = headers.first().map(|(i, _)| *i).unwrap_or(lines.len());
        if let Some(i) = first_after(0, first_header) {
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            entries.push(base(None, name, Some(i)));
        }
        for (k, (at, h)) in headers.iter().enumerate() {
            let end = headers.get(k + 1).map(|(i, _)| *i).unwrap_or(lines.len());
            entries.push(base(Some(h.clone()), h.clone(), first_after(at + 1, end.min(lines.len()))));
            if !scope.domains.contains(h) {
                scope.domains.push(h.clone());
            }
        }
    }
    scope
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn date_of(raw: &str) -> Option<NaiveDate> {
    raw.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

/// `modified` is a UTC timestamp when the harness wrote it: its local date,
/// so an evening write is not a day in the future.
fn local_date_of(raw: &str) -> Option<NaiveDate> {
    match chrono::DateTime::parse_from_rfc3339(raw.trim()) {
        Ok(t) => Some(t.with_timezone(&chrono::Local).date_naive()),
        Err(_) => date_of(raw),
    }
}

/// Where a `source:` path points, or `None` for a URL or a commit.
pub fn source_path(s: &str, root: &Path) -> Option<PathBuf> {
    let s = s.trim();
    if checks::is_url(s) || checks::is_commit(s) {
        return None;
    }
    // A trailing `:120` or `#anchor` names a place inside the file.
    let s = s.split('#').next().unwrap_or(s);
    let s = match s.rsplit_once(':') {
        Some((p, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => p,
        _ => s,
    };
    if let Some(rest) = s.strip_prefix("~/") {
        return claude::home().map(|h| h.join(rest));
    }
    let p = Path::new(s);
    Some(if p.is_absolute() { p.to_path_buf() } else { claude::normalize(&root.join(p)) })
}

struct IndexLine {
    no: usize,
    text: String,
    title: String,
    domain: Option<String>,
}

fn auto_entries(dir: &Path, source_root: &Path, today: NaiveDate, scope: &mut Scope, entries: &mut Vec<Entry>) {
    let index_path = dir.join("MEMORY.md");
    let index = std::fs::read_to_string(&index_path).unwrap_or_default();
    if index_path.is_file() {
        scope.files.push(show(&index_path));
    }
    let (per_line, headers) = checks::sections(&index);
    scope.domains = headers.iter().map(|(_, h)| h.clone()).collect();
    let mut linked: BTreeMap<String, IndexLine> = BTreeMap::new();
    for (i, l) in index.lines().enumerate() {
        if !checks::is_index_line(l) {
            continue;
        }
        let Some(t) = checks::link_target(l) else { continue };
        let title = l.trim_start().strip_prefix("- [").and_then(|r| r.split_once("](")).map(|(t, _)| t.to_string()).unwrap_or_default();
        linked.entry(t).or_insert(IndexLine { no: i + 1, text: l.to_string(), title, domain: per_line.get(i).cloned().flatten() });
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.file_name().is_some_and(|n| n != "MEMORY.md"))
        .collect();
    files.sort();

    let mut auto = vec![];
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        scope.files.push(show(path));
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let front = frontmatter::parse(&text);
        let map = front.map.unwrap_or_default();
        let mut flags = vec![];
        if !front.present || front.error.is_some() {
            flags.push("bad_frontmatter");
        }
        let kind = frontmatter::text(&map, "kind").filter(|k| KINDS.contains(&k.as_str()));
        if kind.is_none() {
            flags.push("no_kind");
        }
        let review_by = frontmatter::text(&map, "review-by");
        if kind.as_deref() == Some("status") {
            match review_by.as_deref().and_then(date_of) {
                None => flags.push("no_review_by"),
                Some(d) if d <= today => flags.push("review_by_past"),
                Some(_) => {}
            }
        }
        let source: Vec<String> = frontmatter::list(&map, "source").unwrap_or_default().into_iter().filter(|s| !s.trim().is_empty()).collect();
        if kind.as_deref() == Some("reference") && source.is_empty() {
            flags.push("no_source");
        }
        if source.iter().filter_map(|s| source_path(s, source_root)).any(|p| !p.exists()) {
            flags.push("source_missing");
        }
        let modified = frontmatter::text(&map, "modified").and_then(|m| local_date_of(&m)).or_else(|| {
            let t = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
            Some(chrono::DateTime::<chrono::Local>::from(t).date_naive())
        });
        let line = linked.remove(&name);
        match &line {
            None => flags.push("unindexed"),
            Some(l) if l.domain.is_none() => flags.push("no_domain"),
            _ => {}
        }
        let title = line
            .as_ref()
            .map(|l| l.title.clone())
            .filter(|t| !t.is_empty())
            .or_else(|| frontmatter::text(&map, "name"))
            .unwrap_or_else(|| name.trim_end_matches(".md").to_string());
        auto.push(Entry {
            scope: "auto",
            domain: line.as_ref().and_then(|l| l.domain.clone()),
            kind,
            type_: frontmatter::text(&map, "type"),
            id: memory_id(&name),
            file: show(path),
            title,
            chars: line.as_ref().map(|l| l.text.chars().count()),
            line_no: line.as_ref().map(|l| l.no),
            line: line.map(|l| l.text),
            age_days: modified.map(|d| (today - d).num_days().max(0)),
            modified: modified.map(|d| d.to_string()),
            review_by,
            source,
            stale: !flags.is_empty(),
            flags,
            used: vec![],
        });
    }
    // Index lines whose file is gone.
    for (target, l) in linked {
        if checks::is_url(&target) {
            continue;
        }
        let mut flags = vec!["dead_link"];
        if l.domain.is_none() {
            flags.push("no_domain");
        }
        let name = Path::new(&target).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(target.clone());
        auto.push(Entry {
            scope: "auto",
            domain: l.domain,
            kind: None,
            type_: None,
            id: memory_id(&name),
            file: show(&dir.join(&target)),
            title: l.title,
            chars: Some(l.text.chars().count()),
            line_no: Some(l.no),
            line: Some(l.text),
            modified: None,
            age_days: None,
            review_by: None,
            source: vec![],
            stale: true,
            flags,
            used: vec![],
        });
    }
    let domain_rank = |d: &Option<String>| d.as_ref().and_then(|d| scope.domains.iter().position(|x| x == d)).unwrap_or(usize::MAX);
    let kind_rank = |k: &Option<String>| k.as_ref().and_then(|k| KINDS.iter().position(|x| x == k)).unwrap_or(KINDS.len());
    auto.sort_by(|a, b| {
        (domain_rank(&a.domain), kind_rank(&a.kind), a.line_no.unwrap_or(usize::MAX), &a.file)
            .cmp(&(domain_rank(&b.domain), kind_rank(&b.kind), b.line_no.unwrap_or(usize::MAX), &b.file))
    });
    entries.extend(auto);
}

fn tilde(s: &str) -> String {
    match claude::home().map(|h| h.display().to_string()) {
        Some(h) if s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s.to_string(),
    }
}

/// The terminal view: the same data, one line per entry.
pub fn render(inv: &Inventory) -> String {
    let mut out = vec![];
    out.push(format!("Memory for {}  (today {})", tilde(&inv.project_root), inv.today));
    match &inv.memory.path {
        Some(p) => out.push(format!(
            "auto memory: {}{}",
            tilde(p),
            if inv.memory.exists { String::new() } else { "  (not created yet)".into() }
        )),
        None => out.push("auto memory: none".into()),
    }
    out.push(format!("{} entries, {} stale; the memory hook is in {} mode", inv.counts.entries, inv.counts.stale, inv.hook));
    for scope in &inv.scopes {
        let es: Vec<&Entry> = inv.entries.iter().filter(|e| e.scope == scope.id).collect();
        out.push(String::new());
        if scope.id == "auto" {
            let stale = es.iter().filter(|e| e.stale).count();
            out.push(format!("{} · {} entries, {} stale", scope.label, es.len(), stale));
            let mut last: Option<Option<String>> = None;
            for e in es {
                if last.as_ref() != Some(&e.domain) {
                    out.push(match &e.domain {
                        Some(d) => format!("  ## {d}"),
                        None => "  (no domain)".into(),
                    });
                    last = Some(e.domain.clone());
                }
                let age = e.age_days.map(|d| format!("{d}d")).unwrap_or_else(|| "-".into());
                out.push(format!(
                    "    {:<10} {}  {:<56} {:>5}  {}",
                    e.kind.as_deref().unwrap_or("-"),
                    e.id,
                    clip(&e.title, 56),
                    age,
                    e.flags.join(" ")
                ));
            }
        } else {
            out.push(format!("{} · {} file{}", scope.label, scope.files.len(), if scope.files.len() == 1 { "" } else { "s" }));
            for f in &scope.files {
                let doms: Vec<&str> = es.iter().filter(|e| &e.file == f).filter_map(|e| e.domain.as_deref()).collect();
                out.push(format!("  {}: {}", tilde(f), if doms.is_empty() { "-".into() } else { doms.join(" · ") }));
            }
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_path_resolves_against_the_root_and_drops_a_place_suffix() {
        let root = Path::new("/r/main");
        assert_eq!(source_path("docs/a.md", root), Some(PathBuf::from("/r/main/docs/a.md")));
        assert_eq!(source_path("src/x.c:120", root), Some(PathBuf::from("/r/main/src/x.c")));
        assert_eq!(source_path("README.md#install", root), Some(PathBuf::from("/r/main/README.md")));
        assert_eq!(source_path("../sib/x", root), Some(PathBuf::from("/r/sib/x")));
        assert_eq!(source_path("/abs/y", root), Some(PathBuf::from("/abs/y")));
        assert_eq!(source_path("https://x.example/a", root), None);
        assert_eq!(source_path("1a2b3c4d", root), None);
    }

    #[test]
    fn clipping_keeps_short_text_and_marks_a_cut() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 4), "abc…");
    }
}
