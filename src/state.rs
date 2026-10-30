//! Promptu's local state (docs/contract.md §7.5), under the state folder
//! (`PROMPTU_STATE_DIR`, else `$XDG_STATE_HOME/embarch-promptu`, else
//! `~/.local/state/embarch-promptu`):
//!
//! - `runs/<run>.json`: one run between `prep` and `render`; pruned after 7 days.
//! - `last/<project>.json`: each pattern's last answers in one project, the
//!   `last` source. `<project>` is the main checkout's memory-folder name.
//! - `drafts/<run>.md`: the rendered prompt, for "edit first".
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
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
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

/// Delete run and draft files older than `days`.
pub fn prune(days: u64) {
    let Ok(base) = dir() else { return };
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
    for sub in ["runs", "drafts"] {
        for e in std::fs::read_dir(base.join(sub)).into_iter().flatten().flatten() {
            let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
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
