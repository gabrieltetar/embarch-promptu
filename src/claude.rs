//! Where Claude Code keeps a project's auto memory, computed the way Claude
//! Code 2.1.287 computes it (read from its own bundle; docs/contract.md §1.2).
//!
//! - The config home is `CLAUDE_CONFIG_DIR`, else `~/.claude`.
//! - Auto-memory folders live under `CLAUDE_CODE_REMOTE_MEMORY_DIR`, else the
//!   config home, as `projects/<folder>/memory/`.
//! - `<folder>` encodes the project's **main checkout**: the canonical git root
//!   of the directory the session started in — a linked worktree resolves to
//!   the checkout it was made from — or that directory outside git.
//! - `CLAUDE_COWORK_MEMORY_PATH_OVERRIDE` and an `autoMemoryDirectory` setting
//!   move the folder elsewhere.

use std::path::{Component, Path, PathBuf};

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Claude Code's config home: `CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn config_dir() -> Option<PathBuf> {
    env_path("CLAUDE_CONFIG_DIR").or_else(|| home().map(|h| h.join(".claude")))
}

/// The folder holding `projects/`: `CLAUDE_CODE_REMOTE_MEMORY_DIR`, else the
/// config home.
pub fn memory_base() -> Option<PathBuf> {
    env_path("CLAUDE_CODE_REMOTE_MEMORY_DIR").or_else(config_dir)
}

/// Above this many characters a folder name is cut and a hash appended.
const FOLDER_MAX: usize = 200;

/// Claude Code's folder name for a path: every UTF-16 unit that is not an
/// ASCII letter or digit becomes `-` (so a character outside the BMP becomes
/// two), and a name over 200 is cut to 200 plus `-` and a base-36 hash.
pub fn project_folder(path: &str) -> String {
    let units: Vec<u16> = path.encode_utf16().collect();
    let name: String = units
        .iter()
        .map(|&u| if u < 128 && (u as u8).is_ascii_alphanumeric() { u as u8 as char } else { '-' })
        .collect();
    if name.len() <= FOLDER_MAX {
        return name;
    }
    format!("{}-{}", &name[..FOLDER_MAX], base36(js_hash(&units).unsigned_abs() as u64))
}

/// Java's `String.hashCode` over UTF-16 units, which is what Claude Code uses.
fn js_hash(units: &[u16]) -> i32 {
    units.iter().fold(0i32, |h, &u| h.wrapping_shl(5).wrapping_sub(h).wrapping_add(u as i32))
}

fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut digits = vec![];
    while n > 0 {
        digits.push(std::char::from_digit((n % 36) as u32, 36).unwrap_or('0'));
        n /= 36;
    }
    digits.iter().rev().collect()
}

/// `CLAUDE_CODE_PROJECT_DIR_NAME` replaces the folder name, but only with
/// `CLAUDE_CONFIG_DIR` set, only when the memory base is the config home, and
/// only when it is a safe name.
fn folder_override() -> Option<String> {
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty())?;
    if env_path("CLAUDE_CODE_REMOTE_MEMORY_DIR").is_some() {
        return None;
    }
    let name = std::env::var("CLAUDE_CODE_PROJECT_DIR_NAME").ok()?;
    let safe = !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let reserved = {
        let l = name.to_ascii_lowercase();
        ["con", "prn", "aux", "nul"].contains(&l.as_str())
            || ((l.starts_with("com") || l.starts_with("lpt")) && l.len() == 4 && l.as_bytes()[3].is_ascii_digit())
    };
    (safe && !reserved).then_some(name)
}

/// `.` and `..` folded away without touching the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn resolve(base: &Path, p: &str) -> PathBuf {
    normalize(&base.join(p))
}

/// The nearest directory at or above `dir` holding a `.git`, file or folder.
pub fn git_root(dir: &Path) -> Option<PathBuf> {
    let mut at = Some(dir);
    while let Some(d) = at {
        if std::fs::symlink_metadata(d.join(".git")).is_ok() {
            return Some(d.to_path_buf());
        }
        at = d.parent();
    }
    None
}

/// A linked worktree's main checkout, as Claude Code resolves it: `.git` is a
/// file naming `<common>/worktrees/<name>`, whose `commondir` leads back to the
/// common `.git` and whose `gitdir` points back at this worktree. Anything
/// else — a plain repository, a submodule, a broken link — is its own root.
pub fn canonical_root(root: &Path) -> PathBuf {
    canonical_root_of(root).unwrap_or_else(|| root.to_path_buf())
}

fn canonical_root_of(root: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(root.join(".git")).ok()?;
    let gitdir = text.trim().strip_prefix("gitdir:")?.trim().to_string();
    let wt = resolve(root, &gitdir);
    let common = resolve(&wt, std::fs::read_to_string(wt.join("commondir")).ok()?.trim());
    if normalize(wt.parent()?) != common.join("worktrees") {
        return None;
    }
    let back = resolve(&wt, std::fs::read_to_string(wt.join("gitdir")).ok()?.trim());
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if real(&back) != real(root).join(".git") {
        return None;
    }
    if common.file_name().is_some_and(|n| n == ".git") {
        return common.parent().map(Path::to_path_buf);
    }
    // A bare repository's worktree: the bare folder itself, unless it is
    // really a checkout.
    if common.join(".git").exists() {
        return None;
    }
    Some(common)
}

/// The path Claude Code keys a session's memory on.
pub fn project_root(cwd: &Path) -> PathBuf {
    match git_root(cwd) {
        Some(r) => canonical_root(&r),
        None => cwd.to_path_buf(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DirSource {
    Default,
    Env(&'static str),
    Setting(PathBuf),
}

impl DirSource {
    pub fn describe(&self) -> String {
        match self {
            DirSource::Default => "default".into(),
            DirSource::Env(v) => format!("env:{v}"),
            DirSource::Setting(p) => format!("setting:{}", p.display()),
        }
    }
}

fn expand(raw: &str, tilde: bool) -> Option<PathBuf> {
    let raw = raw.trim();
    let p = match raw.strip_prefix("~/") {
        Some(rest) if tilde => {
            let rest = normalize(Path::new(rest));
            if rest.as_os_str().is_empty() || rest.starts_with("..") {
                return None;
            }
            home()?.join(rest)
        }
        _ => PathBuf::from(raw),
    };
    let s = p.to_string_lossy().trim_end_matches('/').to_string();
    // Claude Code refuses a path this short, a bare drive, or one with a NUL.
    if s.len() < 3 || s.contains('\0') {
        return None;
    }
    Some(normalize(Path::new(&s)))
}

fn setting_dir(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    expand(v.get("autoMemoryDirectory")?.as_str()?, true)
}

/// The auto-memory folder of a session started in `cwd`, and what chose it.
/// Settings are read from managed policy, the project's
/// `.claude/settings.local.json` and the user's `settings.json`, in that order;
/// a checked-in `.claude/settings.json` is ignored, as Claude Code ignores it.
pub fn auto_memory_dir(cwd: &Path) -> Option<(PathBuf, DirSource)> {
    if let Some(p) = std::env::var("CLAUDE_COWORK_MEMORY_PATH_OVERRIDE").ok().and_then(|v| expand(&v, false)) {
        return Some((p, DirSource::Env("CLAUDE_COWORK_MEMORY_PATH_OVERRIDE")));
    }
    let root = project_root(cwd);
    let mut settings = vec![PathBuf::from("/etc/claude-code/managed-settings.json"), root.join(".claude/settings.local.json")];
    if let Some(c) = config_dir() {
        settings.push(c.join("settings.json"));
    }
    for s in settings {
        if let Some(p) = setting_dir(&s) {
            return Some((p, DirSource::Setting(s)));
        }
    }
    Some((default_memory_dir(&root)?, DirSource::Default))
}

pub fn default_memory_dir(root: &Path) -> Option<PathBuf> {
    let folder = folder_override().unwrap_or_else(|| project_folder(&root.to_string_lossy()));
    Some(memory_base()?.join("projects").join(folder).join("memory"))
}

/// The main checkout a default memory folder's name encodes, found on disk:
/// the name cannot be read back alone (`/a/b-c` and `/a/b/c` share one), so
/// each step takes the child whose own encoding continues the name. `None`
/// for a cut (hashed) name or one nothing on disk matches.
pub fn decode_folder(folder: &str) -> Option<PathBuf> {
    if folder.len() > FOLDER_MAX || !folder.starts_with('-') {
        return None;
    }
    let mut budget = 2000usize;
    descend(Path::new("/"), "-", folder, &mut budget)
}

fn descend(dir: &Path, enc: &str, folder: &str, budget: &mut usize) -> Option<PathBuf> {
    if enc == folder {
        return Some(dir.to_path_buf());
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    children.sort();
    for c in children {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let name = c.file_name()?.to_string_lossy().to_string();
        let next = if enc == "-" { format!("-{}", project_folder(&name)) } else { format!("{enc}-{}", project_folder(&name)) };
        let fits = folder.starts_with(&next) && (folder.len() == next.len() || folder.as_bytes()[next.len()] == b'-');
        if fits {
            if let Some(found) = descend(&c, &next, folder, budget) {
                return Some(found);
            }
        }
    }
    None
}

/// A write target Claude Code treats as auto memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// The memory folder.
    pub dir: PathBuf,
    /// The file's name inside it.
    pub name: String,
    /// `MEMORY.md`, the index.
    pub is_index: bool,
}

fn same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Is `file_path` a `.md` file directly inside an auto-memory folder — any
/// project's `<memory base>/projects/<folder>/memory/`, or the folder `cwd`'s
/// session resolves to? Subfolders of a memory folder are not checked.
pub fn memory_target(file_path: &str, cwd: Option<&Path>) -> Option<Target> {
    let raw = Path::new(file_path);
    let path = if raw.is_absolute() { normalize(raw) } else { normalize(&cwd?.join(raw)) };
    let name = path.file_name()?.to_string_lossy().to_string();
    if !name.ends_with(".md") {
        return None;
    }
    let dir = path.parent()?.to_path_buf();
    let under_projects = dir.file_name().is_some_and(|n| n == "memory")
        && dir.parent().and_then(Path::parent).zip(memory_base()).is_some_and(|(p, b)| same(p, &b.join("projects")));
    let configured = || cwd.and_then(auto_memory_dir).is_some_and(|(d, _)| same(&dir, &d));
    if !under_projects && !configured() {
        return None;
    }
    Some(Target { is_index: name == "MEMORY.md", dir, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_name_replaces_every_non_alphanumeric_unit() {
        assert_eq!(project_folder("/home/u/Github/embarch/embarch-doc"), "-home-u-Github-embarch-embarch-doc");
        assert_eq!(project_folder("/tmp/a.b_c"), "-tmp-a-b-c");
        // é is one UTF-16 unit, an emoji two.
        assert_eq!(project_folder("/x/é"), "-x--");
        assert_eq!(project_folder("/x/😀"), "-x---");
    }

    #[test]
    fn a_long_folder_name_is_cut_at_200_and_hashed() {
        let p = format!("/{}", "a".repeat(250));
        // Reference from node: k.slice(0,200) + "-" + Math.abs(h(p)).toString(36)
        assert_eq!(project_folder(&p), format!("-{}-feo44x", "a".repeat(199)));
    }

    #[test]
    fn the_hash_matches_javascripts() {
        // Values printed by node with Claude Code's own loop:
        // for(...) e=(e<<5)-e+s.charCodeAt(n)|0
        let h = |s: &str| js_hash(&s.encode_utf16().collect::<Vec<_>>());
        assert_eq!(h("hello"), 99162322);
        assert_eq!(h("/a/very/long/path"), -1470736420);
        assert_eq!(base36(h("/a/very/long/path").unsigned_abs() as u64), "obmzhg");
        assert_eq!(base36(h("/x/😀").unsigned_abs() as u64), "t1x55");
        assert_eq!(base36(36), "10");
    }

    #[test]
    fn normalizing_folds_dots() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "promptu-claude-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_linked_worktree_resolves_to_its_main_checkout() {
        let d = scratch("wt");
        let main = d.join("main");
        let wt = d.join("main-ABC-1");
        std::fs::create_dir_all(main.join(".git/worktrees/main-ABC-1")).unwrap();
        std::fs::create_dir_all(wt.join("src")).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", main.join(".git/worktrees/main-ABC-1").display())).unwrap();
        std::fs::write(main.join(".git/worktrees/main-ABC-1/commondir"), "../..\n").unwrap();
        std::fs::write(main.join(".git/worktrees/main-ABC-1/gitdir"), format!("{}\n", wt.join(".git").display())).unwrap();
        assert_eq!(project_root(&wt.join("src")), main);
        assert_eq!(project_root(&main), main);
        // A submodule-style link with no commondir is its own root.
        let sub = d.join("sub");
        std::fs::create_dir_all(main.join(".git/modules/sub")).unwrap();
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(".git"), format!("gitdir: {}\n", main.join(".git/modules/sub").display())).unwrap();
        assert_eq!(project_root(&sub), sub);
        // Outside git, the folder itself.
        let plain = d.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(canonical_root(&plain), plain);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_folder_name_decodes_back_to_the_checkout_on_disk() {
        let d = scratch("dec");
        let target = d.join("my-repo.v2").join("inner");
        std::fs::create_dir_all(&target).unwrap();
        // A decoy the greedy first match would take: my/repo-v2 does not exist.
        std::fs::create_dir_all(d.join("my")).unwrap();
        let folder = project_folder(&target.to_string_lossy());
        assert_eq!(decode_folder(&folder), Some(target.clone()));
        assert_eq!(decode_folder("-no-such-place-anywhere-xyz"), None);
        std::fs::remove_dir_all(d).unwrap();
    }
}
