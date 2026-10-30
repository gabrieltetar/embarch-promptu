//! The machine's Promptu settings: `~/.config/embarch-promptu/config.toml`
//! (`$XDG_CONFIG_HOME` respected; `PROMPTU_CONFIG_DIR` overrides the folder,
//! which is how the tests stay off the real one). Step 1 reads one key:
//!
//! ```toml
//! memory_hook = "warn"   # "warn" (default) · "block" · "off"
//! ```
//!
//! `PROMPTU_MEMORY_HOOK` in the environment wins over the file. Any other key
//! is left for later steps and ignored here.

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
    fn modes_parse_and_an_unknown_one_is_none() {
        assert_eq!(HookMode::parse(" Block "), Some(HookMode::Block));
        assert_eq!(HookMode::parse("off"), Some(HookMode::Off));
        assert_eq!(HookMode::parse("strict"), None);
    }
}
