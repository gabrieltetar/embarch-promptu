//! The machine's Promptu settings: `~/.config/embarch-promptu/config.toml`
//! (`$XDG_CONFIG_HOME` respected; `PROMPTU_CONFIG_DIR` overrides the folder,
//! which is how the tests stay off the real one). Step 1 reads one key:
//!
//! ```toml
//! memory_hook = "warn"   # "warn" (default) · "block" · "off"
//! ```
//!
//! `PROMPTU_MEMORY_HOOK` in the environment wins over the file. Step 2 adds
//! the pass's keys (docs/contract.md §7.6):
//!
//! ```toml
//! pass_effort = "medium"     # "low" · "medium" · "high": the pass's `--effort`
//! pass_model = "sonnet"
//! pass_timeout_s = 15
//! voice_skill = "my-voice"   # any string key reaches templates as {{config.<key>}}
//! memory_claude_md_sections = ["Working rules"]   # default: every section
//! ```

use chrono::NaiveDate;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    /// The hook checks nothing.
    Off,
    /// Every write goes ahead; Claude is told what to fix.
    Warn,
    /// A write with an error is refused; warnings still go ahead.
    Block,
}

impl HookMode {
    fn parse(s: &str) -> Option<HookMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Some(HookMode::Off),
            "warn" => Some(HookMode::Warn),
            "block" => Some(HookMode::Block),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            HookMode::Off => "off",
            HookMode::Warn => "warn",
            HookMode::Block => "block",
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("PROMPTU_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let base = match std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => crate::claude::home()?.join(".config"),
    };
    Some(base.join("embarch-promptu"))
}

pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

/// The hook's mode. An unreadable file or an unknown value means the
/// default, warn: a typo never turns blocking on.
pub fn hook_mode() -> HookMode {
    if let Some(m) = std::env::var("PROMPTU_MEMORY_HOOK").ok().as_deref().and_then(HookMode::parse) {
        return m;
    }
    config_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("memory_hook").and_then(|v| v.as_str()).and_then(HookMode::parse))
        .unwrap_or(HookMode::Warn)
}

/// The whole file as a TOML table; empty when missing or unreadable.
pub fn table() -> toml::Table {
    config_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .unwrap_or_default()
}

/// The pass's effort: `low`, `medium` or `high`; anything else means the
/// default, `medium` (the owner's pick, docs/forks.md S1).
pub fn pass_effort(t: &toml::Table) -> &'static str {
    match t.get("pass_effort").and_then(|v| v.as_str()).map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("low") => "low",
        Some("high") => "high",
        _ => "medium",
    }
}

pub fn pass_model(t: &toml::Table) -> String {
    t.get("pass_model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).unwrap_or("sonnet").to_string()
}

pub fn pass_timeout(t: &toml::Table) -> std::time::Duration {
    let s = t.get("pass_timeout_s").and_then(|v| v.as_integer()).filter(|n| (1..=600).contains(n)).unwrap_or(15);
    std::time::Duration::from_secs(s as u64)
}

/// The global `CLAUDE.md` sections whose bullets join the memory source;
/// `None` means every section.
pub fn claude_md_sections(t: &toml::Table) -> Option<Vec<String>> {
    let a = t.get("memory_claude_md_sections")?.as_array()?;
    Some(a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
}

/// Every string key, for `{{config.<key>}}` in templates.
pub fn template_values(t: &toml::Table) -> serde_json::Map<String, serde_json::Value> {
    t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), serde_json::Value::String(s.to_string())))).collect()
}

/// Which EmbArch parts a run may use (decision 29): the `[embarch]` table
/// forces each to `on` or `off`; `auto` (the default, or anything else)
/// means "if the session has it", which the caller says with `--have`.
pub fn parts(t: &toml::Table, have: &[String]) -> Vec<String> {
    let table = t.get("embarch").and_then(|v| v.as_table());
    crate::pattern::DETECTED
        .iter()
        .filter(|part| match table.and_then(|e| e.get(**part)).and_then(|v| v.as_str()).map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("on") => true,
            Some("off") => false,
            _ => have.iter().any(|h| h == *part),
        })
        .map(|p| p.to_string())
        .collect()
}

/// Promptu's own state: `PROMPTU_STATE_DIR`, else `$XDG_STATE_HOME/embarch-promptu`,
/// else `~/.local/state/embarch-promptu`.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("PROMPTU_STATE_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let base = match std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => crate::claude::home()?.join(".local/state"),
    };
    Some(base.join("embarch-promptu"))
}

/// Today, local time; `PROMPTU_TODAY=YYYY-MM-DD` pins it for tests.
pub fn today() -> NaiveDate {
    std::env::var("PROMPTU_TODAY")
        .ok()
        .and_then(|d| NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d").ok())
        .unwrap_or_else(|| chrono::Local::now().date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_settings_default_to_the_owners_picks() {
        let t: toml::Table = toml::from_str("pass_effort = \"LOW\"\nvoice_skill = \"v\"\nn = 3").unwrap();
        assert_eq!(pass_effort(&t), "low");
        assert_eq!(pass_effort(&toml::Table::new()), "medium");
        assert_eq!(pass_effort(&toml::from_str("pass_effort = \"max\"").unwrap()), "medium");
        assert_eq!(pass_model(&t), "sonnet");
        assert_eq!(pass_timeout(&t).as_secs(), 15);
        assert_eq!(template_values(&t).len(), 2);
    }

    #[test]
    fn embarch_parts_are_detected_unless_forced() {
        let have = vec!["desk".to_string()];
        assert_eq!(parts(&toml::Table::new(), &have), vec!["desk"]);
        assert!(parts(&toml::Table::new(), &[]).is_empty());
        let t: toml::Table = toml::from_str("[embarch]\ndesk = \"off\"\nembarch-api = \"on\"").unwrap();
        assert_eq!(parts(&t, &have), vec!["embarch-api"]);
        let t: toml::Table = toml::from_str("[embarch]\ndesk = \"maybe\"").unwrap();
        assert_eq!(parts(&t, &have), vec!["desk"], "anything else is auto");
    }

    #[test]
    fn modes_parse_and_an_unknown_one_is_none() {
        assert_eq!(HookMode::parse(" Block "), Some(HookMode::Block));
        assert_eq!(HookMode::parse("off"), Some(HookMode::Off));
        assert_eq!(HookMode::parse("strict"), None);
    }
}
