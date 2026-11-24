//! Promptu's local state (docs/contract.md §7.5), under the state folder
//! (`PROMPTU_STATE_DIR`, else `$XDG_STATE_HOME/embarch-promptu`, else
//! `~/.local/state/embarch-promptu`):
//!
//! - `runs/<run>.json`: one run between `prep` and `render`; pruned after 7 days.
//! - `last/<project>.json`: each pattern's last answers in one project, the
//!   `last` source. `<project>` is the main checkout's memory-folder name.
//! - `drafts/<run>.md`: the rendered prompt, for "edit first"; kept past 7 days until a `week pack` has covered its date, at most 35 (forks W8).
//! - `log/<run>.json`: one entry per rendered run, the rough prompt as typed, what
//!   landed in the draft, the rewrite's model and wait (decision 42). Never pruned.
//! - `pass/`: an empty folder the model call runs in.

use crate::run::{Answer, Run};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn dir() -> Result<PathBuf, String> {
    crate::config::state_dir().ok_or_else(|| "cannot work out the state folder (is HOME set?)".to_string())
}

fn ensure(p: &Path) -> Result<(), String> {
    std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Write a file whole: a temporary beside it, then a rename.
pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        ensure(parent)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn run_path(id: &str) -> Result<PathBuf, String> {
    if !valid_run_id(id) {
        return Err(format!("`{id}` is not a run id"));
    }
    Ok(dir()?.join("runs").join(format!("{id}.json")))
}

pub fn save_run(run: &Run) -> Result<(), String> {
    write_atomic(&run_path(&run.run)?, &serde_json::to_string_pretty(run).map_err(|e| e.to_string())?)
}

/// `Ok(None)` when no such run exists (a usage error for the caller).
pub fn load_run(id: &str) -> Result<Option<Run>, String> {
    let p = run_path(id)?;
    let Ok(text) = std::fs::read_to_string(&p) else { return Ok(None) };
    serde_json::from_str(&text).map(Some).map_err(|e| format!("{}: {e}", p.display()))
}

/// How long a draft may outlive the runs' `days` while no pack has covered its
/// date (docs/forks.md W8).
const DRAFT_MAX_DAYS: u64 = 35;

/// The last date a successful `promptu week pack` covered (`week/packed.json`).
pub fn packed_through() -> Option<chrono::NaiveDate> {
    let text = std::fs::read_to_string(dir().ok()?.join("week").join("packed.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    chrono::NaiveDate::parse_from_str(v.get("through")?.as_str()?, "%Y-%m-%d").ok()
}

/// The date a run id starts with (`YYYYMMDD-…`).
fn run_id_date(id: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(id.get(..8)?, "%Y%m%d").ok()
}

/// Delete run files older than `days`, and draft files too, except that a
/// draft is kept until a `week pack` has covered its run's date, and at most
/// 35 days (docs/forks.md W8): it is a terminal run's edit baseline.
pub fn prune(days: u64) {
    let Ok(base) = dir() else { return };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
    let draft_cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days.max(DRAFT_MAX_DAYS) * 86_400);
    let through = packed_through();
    for sub in ["runs", "drafts"] {
        for e in std::fs::read_dir(base.join(sub)).into_iter().flatten().flatten() {
            let Some(modified) = e.metadata().and_then(|m| m.modified()).ok() else { continue };
            let old = if sub == "drafts" {
                let stem = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let packed = match (through, run_id_date(&stem)) {
                    (Some(t), Some(d)) => d <= t,
                    _ => true,
                };
                modified < draft_cutoff || (modified < cutoff && packed)
            } else {
                modified < cutoff
            };
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// A new run id: local time plus four hex digits.
pub fn new_run_id() -> String {
    let now = chrono::Local::now();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mix = (nanos as u64 ^ (std::process::id() as u64).rotate_left(17)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{}-{:04x}", now.format("%Y%m%d-%H%M%S"), (mix >> 48) as u16)
}

pub fn write_draft(id: &str, text: &str) -> Result<PathBuf, String> {
    if !valid_run_id(id) {
        return Err(format!("`{id}` is not a run id"));
    }
    let p = dir()?.join("drafts").join(format!("{id}.md"));
    write_atomic(&p, text)?;
    Ok(p)
}

/// One run's log entry (docs/contract.md §7.5): the original rough prompt, kept
/// so a rewrite can be undone, and what the rewrite cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub promptu: u32,
    pub run: String,
    /// Local time, RFC 3339.
    pub date: String,
    pub cwd: String,
    pub project_key: Option<String>,
    pub pattern: Option<String>,
    pub version: u32,
    /// `brief` or `prompt`.
    pub mode: String,
    /// The rough prompt exactly as typed.
    pub original: String,
    /// The prompt as it landed in the draft, footer included: what the weekly
    /// loop compares the owner's edits against.
    pub rendered: String,
    pub rewritten: bool,
    /// Why there was no rewrite: `brief`, `no-pattern`, `offline`, `off`, or `failed: <reason>`.
    #[serde(default)]
    pub rewrite_skipped: Option<String>,
    pub rewrite_model: Option<String>,
    pub rewrite_effort: Option<String>,
    /// The rewrite call alone.
    pub rewrite_ms: Option<u64>,
    /// The `render` command start to the prompt ready: the owner's wait after the last question.
    pub wait_ms: u64,
}

fn log_path(id: &str) -> Result<PathBuf, String> {
    if !valid_run_id(id) {
        return Err(format!("`{id}` is not a run id"));
    }
    Ok(dir()?.join("log").join(format!("{id}.json")))
}

pub fn save_log(e: &LogEntry) -> Result<(), String> {
    write_atomic(&log_path(&e.run)?, &serde_json::to_string_pretty(e).map_err(|e| e.to_string())?)
}

/// `Ok(None)` when the run has no log entry.
pub fn load_log(id: &str) -> Result<Option<LogEntry>, String> {
    let p = log_path(id)?;
    let Ok(text) = std::fs::read_to_string(&p) else { return Ok(None) };
    serde_json::from_str(&text).map(Some).map_err(|e| format!("{}: {e}", p.display()))
}

/// Every readable log entry, run-id order (oldest first).
pub fn all_logs() -> Vec<LogEntry> {
    let Ok(base) = dir() else { return vec![] };
    let mut v: Vec<LogEntry> = std::fs::read_dir(base.join("log"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    v.sort_by(|a, b| a.run.cmp(&b.run));
    v
}

pub fn pass_dir() -> Result<PathBuf, String> {
    let p = dir()?.join("pass");
    ensure(&p)?;
    Ok(p)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastAnswer {
    pub answer: Answer,
    /// What it reads as, for the `last` source.
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastRun {
    pub date: String,
    pub version: u32,
    pub answers: BTreeMap<String, LastAnswer>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Last {
    pub promptu: u32,
    pub patterns: BTreeMap<String, LastRun>,
}

fn last_path(project_key: &str) -> Result<PathBuf, String> {
    if project_key.is_empty() || project_key.contains('/') || project_key.starts_with('.') {
        return Err(format!("`{project_key}` is not a project key"));
    }
    Ok(dir()?.join("last").join(format!("{project_key}.json")))
}

pub fn load_last(project_key: &str) -> Last {
    last_path(project_key)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Last { promptu: 1, ..Default::default() })
}

pub fn save_last(project_key: &str, pattern: &str, run: LastRun) -> Result<(), String> {
    let mut last = load_last(project_key);
    last.promptu = 1;
    last.patterns.insert(pattern.to_string(), run);
    write_atomic(&last_path(project_key)?, &serde_json::to_string_pretty(&last).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ids_are_safe_file_names() {
        let id = new_run_id();
        assert!(valid_run_id(&id), "{id}");
        assert!(!valid_run_id("../x"));
        assert!(!valid_run_id(""));
        assert!(last_path("../etc").is_err());
    }
}
