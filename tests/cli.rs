//! The `promptu` binary end to end, against a scratch Claude Code config
//! folder and scratch Promptu config, with invented memory files
//! (`tests/fixtures/memory`). Every run clears the environment, so nothing
//! here reads the real `~/.claude` or `~/.config`.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_promptu");
const TODAY: &str = "2026-10-29";

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "promptu-it-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("claude/projects")).unwrap();
        std::fs::create_dir_all(root.join("cfg")).unwrap();
        Scratch { root }
    }

    fn claude(&self) -> PathBuf {
        self.root.join("claude")
    }

    /// A git checkout under the scratch home.
    fn repo(&self, name: &str) -> PathBuf {
        let r = self.root.join("home/work").join(name);
        std::fs::create_dir_all(r.join(".git")).unwrap();
        r
    }

    fn memdir(&self, project: &Path) -> PathBuf {
        let folder: String = project.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        self.claude().join("projects").join(folder).join("memory")
    }

    /// The fixture memory folder, copied in for `project`.
    fn seed(&self, project: &Path) -> PathBuf {
        let dir = self.memdir(project);
        std::fs::create_dir_all(&dir).unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/memory");
        for e in std::fs::read_dir(src).unwrap().flatten() {
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
        dir
    }

    fn mode(&self, mode: &str) {
        std::fs::write(self.root.join("cfg/config.toml"), format!("# test\nmemory_hook = \"{mode}\"\nvoice_skill = \"x\"\n")).unwrap();
    }

    fn cmd(&self, cwd: &Path) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear()
            .env("HOME", self.root.join("home"))
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_TODAY", TODAY)
            .env("TZ", "UTC")
            .current_dir(cwd);
        c
    }

    fn hook(&self, ev: &Value) -> Output {
        self.hook_raw(ev.to_string().as_bytes(), &[])
    }

    fn hook_raw(&self, input: &[u8], env: &[(&str, &str)]) -> Output {
        let mut c = self.cmd(&self.root);
        c.arg("hook").envs(env.iter().copied()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn pre(tool: &str, file: &Path, input: Value) -> Value {
    let mut ti = input;
    ti["file_path"] = json!(file.to_string_lossy());
    json!({
        "session_id": "s-test", "transcript_path": "/dev/null", "cwd": "/",
        "permission_mode": "default", "hook_event_name": "PreToolUse",
        "tool_name": tool, "tool_input": ti, "tool_use_id": "t1"
    })
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// The context a PreToolUse/PostToolUse reply carries, checking its shape.
fn context(o: &Output, event: &str) -> String {
    assert_eq!(o.status.code(), Some(0), "stderr: {}", stderr(o));
    let v: Value = serde_json::from_str(stdout(o).trim()).unwrap_or_else(|e| panic!("{e}: {}", stdout(o)));
    let h = &v["hookSpecificOutput"];
    assert_eq!(h["hookEventName"], event);
    // Warn never decides a permission: the user's own prompt still applies.
    assert!(h.get("permissionDecision").is_none(), "{v}");
    assert_eq!(v.as_object().unwrap().len(), 1, "{v}");
    h["additionalContext"].as_str().unwrap().to_string()
}

fn silent(o: &Output) {
    assert_eq!(o.status.code(), Some(0), "stderr: {}", stderr(o));
    assert!(o.stdout.is_empty(), "stdout: {}", stdout(o));
    assert!(o.stderr.is_empty(), "stderr: {}", stderr(o));
}

const GOOD_RULE: &str = "---\nname: changelog-line\ndescription: Every change gets one changelog line\nkind: rule\nmetadata:\n  type: feedback\n---\n\nEvery change gets one changelog line.\n";
const NO_KIND: &str = "---\nname: changelog-line\ndescription: Every change gets one changelog line\nmetadata:\n  type: feedback\n---\n\nEvery change gets one changelog line.\n";

#[test]
fn writes_outside_memory_and_other_tools_pass_untouched() {
    let s = Scratch::new("outside");
    let repo = s.repo("app");
    let dir = s.seed(&repo);
    silent(&s.hook(&pre("Write", &repo.join("notes.md"), json!({ "content": NO_KIND }))));
    silent(&s.hook(&pre("Read", &dir.join("x.md"), json!({}))));
    silent(&s.hook(&pre("Write", &dir.join("data.json"), json!({ "content": "{}" }))));
    // A subfolder of a memory folder is not a memory file.
    silent(&s.hook(&pre("Write", &dir.join("logs/x.md"), json!({ "content": NO_KIND }))));
    // A MEMORY.md that is not in a memory folder.
    silent(&s.hook(&pre("Write", &repo.join("MEMORY.md"), json!({ "content": "- [x](y.md) — z\n" }))));
}

#[test]
fn a_well_labelled_write_is_silent() {
    let s = Scratch::new("good");
    let dir = s.seed(&s.repo("app"));
    silent(&s.hook(&pre("Write", &dir.join("changelog-line.md"), json!({ "content": GOOD_RULE }))));
}

#[test]
fn warn_mode_lets_the_write_through_and_tells_claude_what_to_fix() {
    let s = Scratch::new("warn");
    let dir = s.seed(&s.repo("app"));
    // No config file: warn is the default.
    let o = s.hook(&pre("Write", &dir.join("changelog-line.md"), json!({ "content": NO_KIND })));
    let ctx = context(&o, "PreToolUse");
    assert!(ctx.contains("warn mode") && ctx.contains("- T2:"), "{ctx}");
    assert!(ctx.contains("changelog-line.md"));
}

#[test]
fn block_mode_refuses_an_error_with_exit_2_and_lets_a_warning_through() {
    let s = Scratch::new("block");
    let dir = s.seed(&s.repo("app"));
    s.mode("block");
    let o = s.hook(&pre("Write", &dir.join("changelog-line.md"), json!({ "content": NO_KIND })));
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    assert!(stderr(&o).contains("refused") && stderr(&o).contains("- T2:"), "{}", stderr(&o));

    // A trap with no source is a warning only.
    let trap = GOOD_RULE.replace("kind: rule", "kind: trap");
    let ctx = context(&s.hook(&pre("Write", &dir.join("t.md"), json!({ "content": trap }))), "PreToolUse");
    assert!(ctx.contains("T5") && !ctx.contains("warn mode"), "{ctx}");

    // The environment wins over the file.
    let o = s.hook_raw(pre("Write", &dir.join("changelog-line.md"), json!({ "content": NO_KIND })).to_string().as_bytes(), &[("PROMPTU_MEMORY_HOOK", "warn")]);
    assert!(context(&o, "PreToolUse").contains("T2"));
}

#[test]
fn off_mode_and_bad_input_fail_open() {
    let s = Scratch::new("open");
    let dir = s.seed(&s.repo("app"));
    s.mode("block");
    for input in [&b""[..], b"not json", b"{\"hook_event_name\": 3}", b"[]", b"{\"tool_name\":\"Write\",\"tool_input\":{\"file_path\":7}}"] {
        silent(&s.hook_raw(input, &[]));
    }
    s.mode("off");
    silent(&s.hook(&pre("Write", &dir.join("changelog-line.md"), json!({ "content": NO_KIND }))));
    // An unknown mode is warn, never block.
    s.mode("strict");
    assert!(context(&s.hook(&pre("Write", &dir.join("changelog-line.md"), json!({ "content": NO_KIND }))), "PreToolUse").contains("warn mode"));
}

#[test]
fn an_edit_is_checked_as_the_file_it_leaves_and_one_that_would_not_apply_is_left_alone() {
    let s = Scratch::new("edit");
    let dir = s.seed(&s.repo("app"));
    s.mode("block");
    let f = dir.join("clamp-trap.md");
    // Dropping a field: T7.
    let o = s.hook(&pre("Edit", &f, json!({ "old_string": "source: docs/missing-fixture-note.md\n", "new_string": "" })));
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("T7") && stderr(&o).contains("`source`"), "{}", stderr(&o));
    // An appended update paragraph: T6, with its line.
    let o = s.hook(&pre("Edit", &f, json!({ "old_string": "Push the lever fully down first.\n", "new_string": "Push the lever fully down first.\n\n**Update 2026-10-29:** it was the clamp.\n" })));
    assert!(stderr(&o).contains("T6 (line 12)"), "{}", stderr(&o));
    // old_string not in the file: Claude Code refuses the edit itself.
    silent(&s.hook(&pre("Edit", &f, json!({ "old_string": "no such text", "new_string": "x" }))));
    // A clean body edit passes.
    silent(&s.hook(&pre("Edit", &f, json!({ "old_string": "Push the lever fully down first.", "new_string": "Push the lever all the way down first." }))));
    // MultiEdit applies in order.
    let o = s.hook(&pre("MultiEdit", &f, json!({ "edits": [
        { "old_string": "kind: trap", "new_string": "kind: status" },
        { "old_string": "first.", "new_string": "first!" }
    ] })));
    let err = stderr(&o);
    assert!(err.contains("T3") && err.contains("T4"), "{err}");
}

#[test]
fn an_index_write_is_checked_on_the_lines_it_touches_only() {
    let s = Scratch::new("index");
    let dir = s.seed(&s.repo("app"));
    s.mode("block");
    let idx = dir.join("MEMORY.md");
    // The legacy long line above every header is untouched: a short new line passes.
    let o = s.hook(&pre("Edit", &idx, json!({
        "old_string": "## tooling\n",
        "new_string": "## tooling\n- [Orphan](orphan.md) — prefers metric units in reports\n"
    })));
    silent(&o);
    // A long new line, a duplicate link, and a new domain.
    let long = format!("- [Soak again](soak-test.md) — {}\n", "a fact that rambles ".repeat(9));
    let o = s.hook(&pre("Edit", &idx, json!({ "old_string": "## tooling\n", "new_string": format!("## tooling\n{long}\n## fleet\n") })));
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(2));
    for want in ["I1", "I4", "I3", "use one of: testing · tooling"] {
        assert!(err.contains(want), "{want}: {err}");
    }
    // A link to a file not there yet is a warning, so it goes through.
    let o = s.hook(&pre("Edit", &idx, json!({ "old_string": "## tooling\n", "new_string": "## tooling\n- [Soon](soon.md) — written next\n" })));
    assert!(context(&o, "PreToolUse").contains("I5"));
}

#[test]
fn block_mode_can_create_a_new_index_and_give_a_legacy_one_its_domains() {
    let s = Scratch::new("headers");
    let repo = s.repo("app");
    let dir = s.memdir(&repo);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("soak.md"), "").unwrap();
    s.mode("block");
    let idx = dir.join("MEMORY.md");
    let with_headers = "## testing\n- [Soak](soak.md) — the soak runs nightly\n";
    // A new index: its headers are free.
    silent(&s.hook(&pre("Write", &idx, json!({ "content": with_headers }))));
    // An index with no headers yet: the write that adds them goes through...
    std::fs::write(&idx, "- [Soak](soak.md) — the soak runs nightly\n").unwrap();
    silent(&s.hook(&pre("Write", &idx, json!({ "content": with_headers }))));
    // ...and once it has them, a new one is refused.
    std::fs::write(&idx, with_headers).unwrap();
    let o = s.hook(&pre("Edit", &idx, json!({ "old_string": "## testing\n", "new_string": "## testing\n\n## other\n" })));
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("I3") && stderr(&o).contains("use one of: testing"), "{}", stderr(&o));
}

#[test]
fn post_tool_use_on_the_index_names_the_orphans() {
    let s = Scratch::new("post");
    let dir = s.seed(&s.repo("app"));
    let ev = json!({
        "session_id": "s", "cwd": "/", "hook_event_name": "PostToolUse", "tool_name": "Edit",
        "tool_input": { "file_path": dir.join("MEMORY.md"), "old_string": "a", "new_string": "b" },
        "tool_response": {}, "tool_use_id": "t"
    });
    let ctx = context(&s.hook(&ev), "PostToolUse");
    assert!(ctx.contains("O1") && ctx.contains("orphan.md"), "{ctx}");
    // A topic-file write gets nothing after the fact.
    let mut topic = ev.clone();
    topic["tool_input"]["file_path"] = json!(dir.join("orphan.md"));
    silent(&s.hook(&topic));
}

#[test]
fn a_configured_memory_folder_and_a_relative_path_are_recognised() {
    let s = Scratch::new("cfgdir");
    let repo = s.repo("app");
    let custom = s.root.join("home/elsewhere/mem");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(s.claude().join("settings.json"), json!({ "autoMemoryDirectory": "~/elsewhere/mem" }).to_string()).unwrap();
    let mut ev = pre("Write", Path::new("note.md"), json!({ "content": NO_KIND }));
    ev["cwd"] = json!(repo.to_string_lossy());
    // Relative to cwd it is repo/note.md: not memory.
    silent(&s.hook(&ev));
    ev["cwd"] = json!(custom.to_string_lossy());
    ev["tool_input"]["file_path"] = json!(custom.join("note.md"));
    let mut in_repo = ev.clone();
    in_repo["cwd"] = json!(repo.to_string_lossy());
    assert!(context(&s.hook(&in_repo), "PreToolUse").contains("T2"));
}

fn inventory(s: &Scratch, cwd: &Path) -> Value {
    let o = s.cmd(cwd).args(["memory", "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn entry<'a>(inv: &'a Value, file: &str) -> &'a Value {
    inv["entries"].as_array().unwrap().iter().find(|e| e["file"].as_str().unwrap().ends_with(file)).unwrap_or_else(|| panic!("{file}"))
}

fn flags(e: &Value) -> Vec<&str> {
    e["flags"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect()
}

#[test]
fn the_inventory_groups_and_flags_the_fixture() {
    let s = Scratch::new("inv");
    let repo = s.repo("app");
    std::fs::create_dir_all(repo.join("tools")).unwrap();
    std::fs::write(repo.join("tools/lint.sh"), "#!/bin/sh\n").unwrap();
    std::fs::write(repo.join("CLAUDE.md"), "# app\n\nIntro line.\n\n## Build\nUse the build script.\n\n## Git\nWork on main.\n").unwrap();
    std::fs::write(s.claude().join("CLAUDE.md"), "## Units\nUse metric units.\n").unwrap();
    let dir = s.seed(&repo);
    std::fs::create_dir_all(dir.join("notes")).unwrap();
    std::fs::write(dir.join("notes/extra.md"), "x").unwrap();
    let mut index = std::fs::read_to_string(dir.join("MEMORY.md")).unwrap();
    index.push_str("- [Top](#top) — an anchor\n- [Site](https://example.com/a) — a page\n- [Extra](notes/extra.md) — a file in a subfolder\n");
    std::fs::write(dir.join("MEMORY.md"), index).unwrap();
    let inv = inventory(&s, &repo.join("tools"));

    assert_eq!(inv["promptu"], 1);
    assert_eq!(inv["hook"], "warn");
    assert_eq!(inv["project_root"], repo.to_string_lossy().as_ref());
    assert_eq!(inv["memory"]["path"], dir.to_string_lossy().as_ref());
    assert_eq!(inv["memory"]["chosen_by"], "default");
    assert_eq!(inv["memory"]["source_root"], repo.to_string_lossy().as_ref());
    let scopes: Vec<&str> = inv["scopes"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(scopes, ["user", "auto", "project"]);
    assert_eq!(inv["scopes"][1]["domains"], json!(["testing", "tooling"]));
    assert_eq!(inv["scopes"][2]["domains"], json!(["Build", "Git"]));

    let board = entry(&inv, "soak-test.md");
    assert_eq!((board["domain"].as_str(), board["kind"].as_str(), board["type"].as_str()), (Some("testing"), Some("status"), Some("project")));
    assert_eq!(flags(board), ["review_by_past"]);
    assert_eq!(board["modified"], "2026-09-15");
    assert_eq!(board["age_days"], 44);
    assert_eq!(board["id"].as_str().unwrap().len(), 6);
    assert_eq!(flags(entry(&inv, "clamp-trap.md")), ["source_missing"]);
    // The harness-moved shape (every label under metadata:) reads the same.
    let flash = entry(&inv, "lint-script.md");
    assert_eq!((flash["kind"].as_str(), flash["type"].as_str()), (Some("rule"), Some("feedback")));
    assert_eq!(flash["source"], json!(["tools/lint.sh"]));
    assert!(flags(flash).is_empty());
    assert_eq!(entry(&inv, "lint-script.md")["stale"], false);
    assert_eq!(flags(entry(&inv, "old-habit.md")), ["no_kind", "no_domain"]);
    assert_eq!(flags(entry(&inv, "orphan.md")), ["unindexed"]);
    assert_eq!(flags(entry(&inv, "gone.md")), ["dead_link"]);
    // An anchor and a URL are not memories; a file in a subfolder is not dead.
    assert!(inv["entries"].as_array().unwrap().iter().all(|e| !e["file"].as_str().unwrap().contains("#top") && !e["file"].as_str().unwrap().contains("example.com")));
    let sub = entry(&inv, "notes/extra.md");
    assert!(flags(sub).is_empty() && sub["stale"] == false, "{sub}");
    assert!(entry(&inv, "old-habit.md")["chars"].as_u64().unwrap() > 150);

    // Order: domains in header order, the domainless last; kinds in their
    // fixed order inside a domain, the unlabelled last.
    let auto: Vec<&str> = inv["entries"].as_array().unwrap().iter().filter(|e| e["scope"] == "auto").map(|e| e["file"].as_str().unwrap().rsplit('/').next().unwrap()).collect();
    assert_eq!(auto, ["clamp-trap.md", "soak-test.md", "lint-script.md", "gone.md", "extra.md", "orphan.md", "old-habit.md"]);
    let md = entry(&inv, "app/CLAUDE.md");
    assert_eq!(md["id"], "claude-md");
    assert_eq!(inv["counts"]["stale"], 5);

    // The text view says the same, one line per entry.
    let o = s.cmd(&repo).arg("memory").output().unwrap();
    let text = stdout(&o);
    assert!(text.contains("  ## testing") && text.contains("review_by_past") && text.contains("warn mode"), "{text}");
}

#[test]
fn a_worktree_session_reads_the_main_checkouts_memory_and_sources() {
    let s = Scratch::new("wt");
    let main = s.repo("app");
    // A source only the main checkout holds (a git-excluded folder, say).
    std::fs::create_dir_all(main.join("docs")).unwrap();
    std::fs::write(main.join("docs/missing-fixture-note.md"), "x").unwrap();
    s.seed(&main);
    let wt = s.root.join("home/work/app-ABC-1");
    std::fs::create_dir_all(&wt).unwrap();
    let wtgit = main.join(".git/worktrees/app-ABC-1");
    std::fs::create_dir_all(&wtgit).unwrap();
    std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wtgit.display())).unwrap();
    std::fs::write(wtgit.join("commondir"), "../..\n").unwrap();
    std::fs::write(wtgit.join("gitdir"), format!("{}\n", wt.join(".git").display())).unwrap();

    let inv = inventory(&s, &wt);
    assert_eq!(inv["project_root"], main.to_string_lossy().as_ref());
    assert_eq!(inv["memory"]["source_root"], main.to_string_lossy().as_ref());
    assert!(flags(entry(&inv, "clamp-trap.md")).is_empty());
}

#[test]
fn memory_check_reads_non_ascii_files_without_panicking() {
    let s = Scratch::new("utf8");
    let repo = s.repo("app");
    let dir = s.memdir(&repo);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("café.md"),
        "---\nname: café\ndescription: Ünïcödé everywhere\nkind: status\nreview-by: 2026-1é-01\n---\n\nUpdated 2026-10-0é: still fine\n",
    )
    .unwrap();
    std::fs::write(dir.join("MEMORY.md"), "## résumé\n- [Café](café.md) — façade, naïve, 日本語\n").unwrap();
    let o = s.cmd(&repo).args(["memory", "check", "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["errors"], 1); // T4 only: the "Updated" line has no real date
    let o = s.cmd(&repo).args(["memory"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
}

#[test]
fn memory_check_reports_and_gates() {
    let s = Scratch::new("check");
    let repo = s.repo("app");
    s.seed(&repo);
    let o = s.cmd(&repo).args(["memory", "check", "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    let by: Vec<(String, Vec<String>)> = r["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let name = f["path"].as_str().unwrap().rsplit('/').next().unwrap().to_string();
            (name, f["findings"].as_array().unwrap().iter().map(|x| x["check"].as_str().unwrap().to_string()).collect())
        })
        .collect();
    let get = |n: &str| by.iter().find(|(f, _)| f == n).map(|(_, c)| c.clone()).unwrap();
    assert_eq!(by[0].0, "MEMORY.md");
    assert_eq!(get("MEMORY.md"), ["I1", "I2", "I5", "O1"]);
    assert_eq!(get("soak-test.md"), ["T4"]);
    assert_eq!(get("old-habit.md"), ["T2"]);
    assert!(get("lint-script.md").is_empty());
    assert!(get("clamp-trap.md").is_empty());
    assert_eq!(r["errors"], 4);

    // A clean file alone passes; a missing path is a usage error.
    let clean = s.memdir(&repo).join("lint-script.md");
    assert_eq!(s.cmd(&repo).args(["memory", "check"]).arg(&clean).output().unwrap().status.code(), Some(0));
    assert_eq!(s.cmd(&repo).args(["memory", "check", "/no/such/place"]).output().unwrap().status.code(), Some(2));
}
