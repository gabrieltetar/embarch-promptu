//! `promptu calls`: a recorded session's tool calls and result characters,
//! per subagent and per activity class.
//!
//! Reads Claude Code's transcripts: `<session>.jsonl` (the main thread) and
//! `<session>/subagents/*.jsonl` with each one's `*.meta.json` description.
//! A `tool_use` block is joined to its `tool_result` by id; the size is the
//! characters of the result's text. [`classify`] is the one place the class
//! rules live.

use crate::claude;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Activity classes, in the order [`classify`] tries them.
pub const CLASSES: [&str; 5] = ["git", "build", "config", "source", "other"];

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("class pattern"))
}

/// The class of one call. THE class table; the first match wins:
///
/// 1. `git`: a Bash command running `git log`, `git diff` or `git show`
///    (global options such as `-C <dir>` allowed between).
/// 2. `build`: a Bash command whose first word (after `cd x &&`, env
///    assignments) is `cargo`, `west`, `cmake`, `make`, `ninja`, `pytest`,
///    `npm`, `gradle` or `tsc`.
/// 3. `config`: a call whose path or command names Kconfig, devicetree or
///    manifest files: `Kconfig*`, `*.dts`, `*.dtsi`, `*.overlay`, `*.conf`,
///    `west.yml`, `CMakeLists.txt`, `*.defconfig`.
/// 4. `source`: Read, Grep or Glob; or Bash whose first word is `cat`, `grep`,
///    `rg`, `sed`, `head`, `tail`, `ls`, `find`, `wc`, `awk`, `tree`, `nl`.
/// 5. `other`: everything else.
pub fn classify(tool: &str, input: &Value) -> &'static str {
    static GIT: OnceLock<Regex> = OnceLock::new();
    static BUILD: OnceLock<Regex> = OnceLock::new();
    static CONFIG: OnceLock<Regex> = OnceLock::new();
    static SRC: OnceLock<Regex> = OnceLock::new();
    let cmd = if tool == "Bash" { input.get("command").and_then(Value::as_str).unwrap_or("") } else { "" };
    // What a call looks at: the command, or the path/pattern of a file tool.
    let target: String = if tool == "Bash" {
        cmd.to_string()
    } else {
        ["file_path", "path", "pattern", "glob"].iter().filter_map(|k| input.get(*k).and_then(Value::as_str)).collect::<Vec<_>>().join(" ")
    };
    if tool == "Bash" {
        if re(&GIT, r"(^|[\s;&|(])git(\s+(-C\s+\S+|-c\s+\S+|--[a-z-]+(=\S+)?))*\s+(log|diff|show)\b").is_match(cmd) {
            return "git";
        }
        if re(&BUILD, r"^\s*(cd\s+\S+\s*&&\s*)?([A-Za-z_]+=\S*\s+)*(cargo|west|cmake|make|ninja|pytest|npm|gradle|tsc)\b").is_match(cmd) {
            return "build";
        }
    }
    if re(&CONFIG, r"(?i)(Kconfig[\w.-]*|\.dtsi?\b|\.overlay\b|\.conf\b|\.defconfig\b|west\.yml|CMakeLists\.txt)").is_match(&target) {
        return "config";
    }
    match tool {
        "Read" | "Grep" | "Glob" => "source",
        "Bash" if re(&SRC, r"^\s*(cd\s+\S+\s*&&\s*)?(cat|grep|rg|sed|head|tail|ls|find|wc|awk|tree|nl)\b").is_match(cmd) => "source",
        _ => "other",
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Call {
    pub tool: String,
    pub class: String,
    /// The command or path, one line, cut for display.
    pub what: String,
    pub result_chars: usize,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Tally {
    pub calls: usize,
    pub result_chars: usize,
}

#[derive(Debug, Serialize)]
pub struct Thread {
    /// `main`, or the subagent's id (file stem without `agent-`).
    pub id: String,
    pub description: String,
    pub total: Tally,
    pub by_class: BTreeMap<String, Tally>,
    #[serde(skip)]
    pub list: Vec<Call>,
}

fn result_chars(content: &Value) -> usize {
    match content {
        Value::String(s) => s.chars().count(),
        Value::Array(a) => a.iter().map(|b| b.get("text").and_then(Value::as_str).map_or(0, |t| t.chars().count())).sum(),
        _ => 0,
    }
}

fn one_line(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 100 {
        format!("{}…", flat.chars().take(100).collect::<String>())
    } else {
        flat
    }
}

/// The calls of one transcript, in order, results joined by id.
pub fn read_calls(text: &str) -> Vec<Call> {
    let mut order: Vec<String> = vec![];
    let mut uses: HashMap<String, (String, Value)> = HashMap::new();
    let mut results: HashMap<String, usize> = HashMap::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else { continue };
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let (Some(id), Some(name)) = (b.get("id").and_then(Value::as_str), b.get("name").and_then(Value::as_str)) else { continue };
                    if uses.insert(id.to_string(), (name.to_string(), b.get("input").cloned().unwrap_or(Value::Null))).is_none() {
                        order.push(id.to_string());
                    }
                }
                Some("tool_result") => {
                    if let Some(id) = b.get("tool_use_id").and_then(Value::as_str) {
                        results.entry(id.to_string()).or_insert_with(|| result_chars(&b["content"]));
                    }
                }
                _ => {}
            }
        }
    }
    order
        .into_iter()
        .map(|id| {
            let (tool, input) = &uses[&id];
            let what = if tool == "Bash" {
                input.get("command").and_then(Value::as_str).unwrap_or("").to_string()
            } else {
                ["file_path", "path", "pattern", "description"].iter().filter_map(|k| input.get(*k).and_then(Value::as_str)).collect::<Vec<_>>().join(" ")
            };
            Call { tool: tool.clone(), class: classify(tool, input).to_string(), what: one_line(&what), result_chars: results.get(&id).copied().unwrap_or(0) }
        })
        .collect()
}

/// A commit a session made: the short hash `git commit` printed, and when the
/// call was made (the record's `timestamp`, as written).
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub at: String,
}

/// Whether a Bash command runs `git commit` (global options such as `-C <dir>`
/// allowed between). `git commit-tree` and `--dry-run` are not commits.
pub fn is_commit(cmd: &str) -> bool {
    static COMMIT: OnceLock<Regex> = OnceLock::new();
    re(&COMMIT, r"(^|[\s;&|(])git(\s+(-C\s+\S+|-c\s+\S+|--[a-z-]+(=\S+)?))*\s+commit(\s|$)").is_match(cmd) && !cmd.contains("--dry-run")
}

/// The commits one transcript made, in order: a `git commit` call whose result
/// shows git's `[<branch> <sha>] <subject>` line and is not an error. Reads
/// every record, so a subagent's own transcript is read as it is written.
pub fn commits(text: &str) -> Vec<Commit> {
    static LINE: OnceLock<Regex> = OnceLock::new();
    let line = re(&LINE, r"(?m)^\[[^\]\n]* ([0-9a-f]{7,40})\]");
    let mut order: Vec<String> = vec![];
    let mut at: HashMap<String, String> = HashMap::new();
    let mut done: HashMap<String, Option<String>> = HashMap::new();
    for l in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else { continue };
        let stamp = v.get("timestamp").and_then(Value::as_str).unwrap_or("");
        let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) else { continue };
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") if b.get("name").and_then(Value::as_str) == Some("Bash") => {
                    let cmd = b.pointer("/input/command").and_then(Value::as_str).unwrap_or("");
                    if let Some(id) = b.get("id").and_then(Value::as_str).filter(|_| is_commit(cmd)) {
                        if at.insert(id.to_string(), stamp.to_string()).is_none() {
                            order.push(id.to_string());
                        }
                    }
                }
                Some("tool_result") => {
                    let Some(id) = b.get("tool_use_id").and_then(Value::as_str) else { continue };
                    if b.get("is_error").and_then(Value::as_bool) == Some(true) {
                        done.entry(id.to_string()).or_insert(None);
                        continue;
                    }
                    let body = crate::used::texts(&b["content"]).join("\n");
                    done.entry(id.to_string()).or_insert_with(|| line.captures(&body).map(|c| c[1].to_string()));
                }
                _ => {}
            }
        }
    }
    order.into_iter().filter_map(|id| done.get(&id).cloned().flatten().map(|sha| Commit { sha, at: at[&id].clone() })).collect()
}

/// Every commit of a session: its main transcript and each subagent's, newest
/// duplicates (one hash seen twice) dropped, in time order of the records.
pub fn session_commits(session: &Path) -> Vec<(Commit, bool)> {
    let mut out: Vec<(Commit, bool)> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    let mut take = |text: &str, sub: bool, out: &mut Vec<(Commit, bool)>| {
        for c in commits(text) {
            if seen.insert(c.sha.clone()) {
                out.push((c, sub));
            }
        }
    };
    if let Ok(t) = std::fs::read_to_string(session) {
        take(&t, false, &mut out);
    }
    let dir = session.with_extension("").join("subagents");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    files.sort();
    for f in files {
        if let Ok(t) = std::fs::read_to_string(&f) {
            take(&t, true, &mut out);
        }
    }
    out.sort_by(|a, b| a.0.at.cmp(&b.0.at));
    out
}

fn thread(id: &str, description: &str, text: &str) -> Thread {
    let list = read_calls(text);
    let mut by_class: BTreeMap<String, Tally> = CLASSES.iter().map(|c| (c.to_string(), Tally::default())).collect();
    let mut total = Tally::default();
    for c in &list {
        let t = by_class.entry(c.class.clone()).or_default();
        t.calls += 1;
        t.result_chars += c.result_chars;
        total.calls += 1;
        total.result_chars += c.result_chars;
    }
    Thread { id: id.to_string(), description: description.to_string(), total, by_class, list }
}

/// A session given as a `.jsonl` path, or a session id found under
/// `<config home>/projects/*/`.
pub fn find_session(arg: &str) -> Option<PathBuf> {
    let p = Path::new(arg);
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    let projects = claude::config_dir()?.join("projects");
    let id = arg.trim_end_matches(".jsonl");
    std::fs::read_dir(projects).ok()?.flatten().map(|d| d.path().join(format!("{id}.jsonl"))).find(|p| p.is_file())
}

/// The main thread, then each subagent by file name. `only`: a subagent id
/// (or a prefix of one); the main thread is left out when it is given.
pub fn load(session: &Path, only: Option<&str>) -> Result<Vec<Thread>, String> {
    let mut out = vec![];
    if only.is_none() {
        let text = std::fs::read_to_string(session).map_err(|e| format!("{}: {e}", session.display()))?;
        out.push(thread("main", "main thread", &text));
    }
    let dir = session.with_extension("").join("subagents");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
    files.sort();
    let mut seen: HashSet<String> = HashSet::new();
    for f in files {
        let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let id = stem.strip_prefix("agent-").unwrap_or(&stem).to_string();
        if only.is_some_and(|o| !id.starts_with(o.strip_prefix("agent-").unwrap_or(o))) || !seen.insert(id.clone()) {
            continue;
        }
        let description = std::fs::read_to_string(f.with_extension("meta.json"))
            .ok()
            .and_then(|m| serde_json::from_str::<Value>(&m).ok())
            .and_then(|v| v.get("description").and_then(Value::as_str).map(one_line))
            .unwrap_or_default();
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        out.push(thread(&id, &description, &text));
    }
    if let Some(o) = only {
        if out.is_empty() {
            return Err(format!("no subagent '{o}' in {}", dir.display()));
        }
    }
    Ok(out)
}

/// The report: a table per thread, or with a class the calls of it.
pub fn render(threads: &[Thread], class: Option<&str>) -> String {
    let mut s = String::new();
    if let Some(c) = class {
        for t in threads {
            let mine: Vec<&Call> = t.list.iter().filter(|x| x.class == c).collect();
            s += &format!("{} {} [{c}]: {} calls, {} chars\n", t.id, t.description, mine.len(), mine.iter().map(|x| x.result_chars).sum::<usize>());
            for x in mine {
                s += &format!("  {:>8}  {}  {}\n", x.result_chars, x.tool, x.what);
            }
        }
        return s;
    }
    s += &format!("{:<26} {:>6} {:>10}", "thread", "calls", "chars");
    for c in CLASSES {
        s += &format!(" {:>13}", format!("{c} c/ch"));
    }
    s.push('\n');
    let mut sum = Tally::default();
    for t in threads {
        let name = if t.description.is_empty() { t.id.clone() } else { format!("{} {}", t.id.chars().take(8).collect::<String>(), t.description) };
        s += &format!("{:<26} {:>6} {:>10}", name.chars().take(26).collect::<String>(), t.total.calls, t.total.result_chars);
        for c in CLASSES {
            let k = &t.by_class[c];
            s += &format!(" {:>13}", format!("{}/{}", k.calls, k.result_chars));
        }
        s.push('\n');
        sum.calls += t.total.calls;
        sum.result_chars += t.total.result_chars;
    }
    if threads.len() > 1 {
        s += &format!("{:<26} {:>6} {:>10}\n", "total", sum.calls, sum.result_chars);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(c: &str) -> &'static str {
        classify("Bash", &json!({ "command": c }))
    }

    #[test]
    fn each_class_matches_what_the_table_says() {
        assert_eq!(bash("git log --oneline -5"), "git");
        assert_eq!(bash("git -C /x diff HEAD~1"), "git");
        assert_eq!(bash("git show abc123:src/a.rs"), "git");
        assert_eq!(bash("git status"), "other");
        assert_eq!(bash("cargo test --all"), "build");
        assert_eq!(bash("cd x && west build -b b"), "build");
        assert_eq!(bash("CC=clang make"), "build");
        assert_eq!(bash("cat boards/x/Kconfig.board"), "config");
        assert_eq!(bash("grep -n FOO app/prj.conf"), "config");
        assert_eq!(bash("cat src/main.c"), "source");
        assert_eq!(bash("rg foo src/"), "source");
        assert_eq!(bash("echo hi"), "other");
        let t = |n: &str, k: &str, v: &str| classify(n, &json!({ k: v }));
        assert_eq!(t("Read", "file_path", "/w/src/a.rs"), "source");
        assert_eq!(t("Read", "file_path", "/w/boards/x.dts"), "config");
        assert_eq!(t("Read", "file_path", "/w/west.yml"), "config");
        assert_eq!(t("Grep", "pattern", "fn main"), "source");
        assert_eq!(t("Glob", "pattern", "**/*.overlay"), "config");
        assert_eq!(t("Edit", "file_path", "/w/src/a.rs"), "other");
        // git wins over config: history of a Kconfig is history.
        assert_eq!(bash("git log -- Kconfig"), "git");
    }

    #[test]
    fn a_commit_is_a_git_commit_call_whose_result_shows_a_hash() {
        assert!(is_commit("git commit -m x"));
        assert!(is_commit("cd /w && git -C /w commit -am 'x'"));
        assert!(!is_commit("git commit --dry-run"));
        assert!(!is_commit("git commit-tree abc"));
        assert!(!is_commit("git log"));
        let rec = |t: &str, ts: &str, b: Value| json!({"type": t, "timestamp": ts, "message": {"content": [b]}}).to_string();
        let text = [
            rec("assistant", "2026-11-20T10:00:00Z", json!({"type":"tool_use","id":"a","name":"Bash","input":{"command":"git commit -m one"}})),
            rec("user", "2026-11-20T10:00:01Z", json!({"type":"tool_result","tool_use_id":"a","content":"[main 9f3a1c2] one\n 1 file changed"})),
            rec("assistant", "2026-11-20T10:01:00Z", json!({"type":"tool_use","id":"b","name":"Bash","input":{"command":"git commit -m two"}})),
            rec("user", "2026-11-20T10:01:01Z", json!({"type":"tool_result","tool_use_id":"b","is_error":true,"content":"nothing to commit"})),
            rec("assistant", "2026-11-20T10:02:00Z", json!({"type":"tool_use","id":"c","name":"Bash","input":{"command":"git commit -m three"}})),
            rec("user", "2026-11-20T10:02:01Z", json!({"type":"tool_result","tool_use_id":"c","content":[{"type":"text","text":"[feature/x (root-commit) abcdef0123] three"}]})),
            rec("assistant", "2026-11-20T10:03:00Z", json!({"type":"tool_use","id":"d","name":"Bash","input":{"command":"git commit -m four"}})),
            rec("user", "2026-11-20T10:03:01Z", json!({"type":"tool_result","tool_use_id":"d","content":"On branch main\nnothing to commit, working tree clean"})),
        ]
        .join("\n");
        let got: Vec<(String, String)> = commits(&text).into_iter().map(|c| (c.sha, c.at)).collect();
        assert_eq!(got, vec![("9f3a1c2".into(), "2026-11-20T10:00:00Z".into()), ("abcdef0123".into(), "2026-11-20T10:02:00Z".into())]);
    }

    #[test]
    fn a_result_joins_its_call_by_id_and_counts_characters() {
        let text = [
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Read","input":{"file_path":"/w/x.rs"}}]}}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"b","name":"Bash","input":{"command":"git log"}}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b","content":[{"type":"text","text":"héllo"},{"type":"text","text":"ab"}]}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"12345"}]}}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"c","name":"Edit","input":{}}]}}),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        let calls = read_calls(&text);
        let got: Vec<(&str, &str, usize)> = calls.iter().map(|c| (c.tool.as_str(), c.class.as_str(), c.result_chars)).collect();
        assert_eq!(got, vec![("Read", "source", 5), ("Bash", "git", 7), ("Edit", "other", 0)]);
        let t = thread("main", "", &text);
        assert_eq!((t.total.calls, t.total.result_chars), (3, 12));
        assert_eq!(t.by_class["git"], Tally { calls: 1, result_chars: 7 });
    }
}
