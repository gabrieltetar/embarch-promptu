//! `promptu prep`, `next`, `render` and `lint` end to end, against a scratch
//! Claude Code config, scratch Promptu config and state, and a fake `claude`
//! (`PROMPTU_CLAUDE`) that records what it was sent and prints a canned
//! reply. Nothing here reads the real `~/.claude`, `~/.config` or
//! `~/.local/state`, and no model is called. Every prompt is invented.

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
            "promptu-pt-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for d in ["home", "claude/projects", "cfg", "state", "fake"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let fake = root.join("fake/claude");
        std::fs::write(
            &fake,
            "#!/bin/sh\ncat > \"$PROMPTU_FAKE_DIR/stdin.txt\"\nprintf '%s\\n' \"$@\" > \"$PROMPTU_FAKE_DIR/argv.txt\"\nenv > \"$PROMPTU_FAKE_DIR/env.txt\"\nif [ -n \"$PROMPTU_FAKE_SLEEP\" ]; then sleep \"$PROMPTU_FAKE_SLEEP\"; fi\nif [ -n \"$PROMPTU_FAKE_NOISE\" ]; then head -c 300000 /dev/zero | tr '\\0' x >&2; sleep 6 & fi\ncat \"$PROMPTU_FAKE_DIR/reply.json\"\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(root.join("cfg/config.toml"), "voice_skill = \"my-voice\"\n").unwrap();
        Scratch { root }
    }

    /// A git checkout under the scratch home, on `branch`.
    fn repo(&self, name: &str, branch: &str) -> PathBuf {
        let r = self.root.join("home/work").join(name);
        std::fs::create_dir_all(r.join(".git")).unwrap();
        std::fs::write(r.join(".git/HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
        r
    }

    fn memdir(&self, project: &Path) -> PathBuf {
        let folder: String = project.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        self.claude().join("projects").join(folder).join("memory")
    }

    fn claude(&self) -> PathBuf {
        self.root.join("claude")
    }

    /// The model's reply: the pass's JSON inside Claude Code's result envelope.
    fn reply(&self, pass: Value) {
        let env = json!({"type": "result", "is_error": false, "terminal_reason": "completed", "result": pass.to_string(), "total_cost_usd": 0.0123});
        std::fs::write(self.root.join("fake/reply.json"), env.to_string()).unwrap();
    }

    fn sent(&self, what: &str) -> String {
        std::fs::read_to_string(self.root.join("fake").join(what)).unwrap_or_default()
    }

    fn cmd(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear()
            .env("HOME", self.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .env("CLAUDE_CONFIG_DIR", self.claude())
            .env("PROMPTU_CONFIG_DIR", self.root.join("cfg"))
            .env("PROMPTU_STATE_DIR", self.root.join("state"))
            .env("PROMPTU_CLAUDE", self.root.join("fake/claude"))
            .env("PROMPTU_FAKE_DIR", self.root.join("fake"))
            .env("PROMPTU_TODAY", TODAY)
            .env("TZ", "UTC")
            .current_dir(cwd)
            .args(args);
        c
    }

    fn run(&self, cwd: &Path, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> Output {
        let mut c = self.cmd(cwd, args);
        c.envs(env.iter().copied()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    }

    fn json(&self, cwd: &Path, args: &[&str], stdin: &str) -> Value {
        let o = self.run(cwd, args, stdin, &[]);
        assert_eq!(o.status.code(), Some(0), "{args:?}: stderr {}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&o.stdout)))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn ids(page: &Value) -> Vec<String> {
    page["ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

/// Every page must be one AskUserQuestion call Claude Code accepts.
fn assert_askable(out: &Value) {
    for page in out["pages"].as_array().unwrap().iter().chain(out["choose"].as_object().map(|_| &out["choose"])) {
        let qs = page["questions"].as_array().unwrap();
        assert!((1..=4).contains(&qs.len()), "{page}");
        assert_eq!(qs.len(), page["ids"].as_array().unwrap().len());
        for q in qs {
            let h = q["header"].as_str().unwrap();
            assert!(!h.is_empty() && h.chars().count() <= 12, "header {h}");
            let opts = q["options"].as_array().unwrap();
            assert!((2..=4).contains(&opts.len()), "{q}");
            assert!(q["question"].as_str().is_some_and(|s| !s.is_empty()));
            let labels: Vec<&str> = opts.iter().map(|o| o["label"].as_str().unwrap()).collect();
            let mut uniq = labels.clone();
            uniq.sort();
            uniq.dedup();
            assert_eq!(uniq.len(), labels.len(), "duplicate labels {labels:?}");
            // A recommended option is always first.
            if let Some(i) = labels.iter().position(|l| l.ends_with("(Recommended)")) {
                assert!(labels[..i].iter().all(|l| l.ends_with("(Recommended)")), "{labels:?}");
            }
        }
    }
}

const BENCH_ASK: &str = "Validate the two reworks on the stepper driver board: a pull-down on the enable line and a cap on the fault pin";

fn bench_reply() -> Value {
    json!({"pattern": "bench-session", "confidence": 0.9, "alt": ["debug-failure"],
           "flags": {"hardware": true, "firmware": true, "brief": false},
           "filled": {
               "flash": {"value": "EmbArch", "source": "memory", "entry": "de8cea", "evidence": "flash through EmbArch only"},
               "pass": {"value": "the fault pin stays high", "source": "prompt", "evidence": "a cap on the fault pin and it stays high"},
               "board": {"value": "Stepper driver board", "source": "prompt", "evidence": "the two reworks on the stepper driver board"}},
           "candidates": {"pass": ["The fault pin stays high through 50 enable toggles", "The motor holds still with enable low"], "unblocks": ["The next board revision"]},
           "relevant": {"heavy": false, "timebox": false, "unblocks": true, "flash": true},
           "ambiguous": []})
}

fn seed_memory(s: &Scratch, repo: &Path) {
    let dir = s.memdir(repo);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MEMORY.md"), "## bench\n\n- [Cache trap](cache-trap.md) — flash through EmbArch only; the vendor tool drops the bootloader\n").unwrap();
    std::fs::write(s.claude().join("CLAUDE.md"), "# Rules\n\n## Working rules\n\n- Only I open pull requests.\n\n## Style\n\n- Short answers.\n").unwrap();
}

#[test]
fn a_fill_carries_its_question_and_can_be_changed_in_place() {
    // The owner's P13: the dialog edits a pre-filled answer; the change is an answer, `!` in the footer.
    let s = Scratch::new("fill-edit");
    let repo = s.repo("motor-fw", "main");
    seed_memory(&s, &repo);
    s.reply(bench_reply());
    let out = s.json(&repo, &["prep", "--have", "desk,embarch-api"], BENCH_ASK);
    let fill = |out: &Value, q: &str| out["filled"].as_array().unwrap().iter().find(|f| f["q"] == q).cloned().unwrap();
    let flash = fill(&out, "flash");
    assert_eq!(flash["ask"]["header"], "Flash via");
    assert_eq!(flash["ask"]["multiSelect"], false);
    let labels: Vec<&str> = flash["ask"]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap()).collect();
    assert_eq!(labels[0], "EmbArch (Recommended)");
    assert_eq!(flash["picked"], json!(["EmbArch (Recommended)"]));
    assert_eq!(flash["other"], Value::Null);
    assert_eq!(flash["changed"], false);
    // The always-asked board is its page's to ask: no `ask` here.
    assert!(fill(&out, "board").get("ask").is_none());
    let run = out["run"].as_str().unwrap();
    let next = s.json(&repo, &["next", run], &json!({"answers": {"flash": "The vendor tool"}}).to_string());
    let flash = fill(&next, "flash");
    assert_eq!(flash["changed"], true);
    assert_eq!(flash["picked"], json!(["The vendor tool"]));
    let r = s.json(&repo, &["render", run, "--rest", "recommended"], "{}");
    assert!(r["footer"].as_str().unwrap().contains("flash:memory.de8cea!"), "{}", r["footer"]);
}

#[test]
fn a_bench_run_from_prep_to_render() {
    let s = Scratch::new("bench");
    let repo = s.repo("motor-fw", "feat/reworks");
    std::fs::write(repo.join("west.yml"), "manifest: {}\n").unwrap();
    seed_memory(&s, &repo);
    s.reply(bench_reply());
    let out = s.json(&repo, &["prep", "--have", "desk,embarch-api"], BENCH_ASK);

    // What the model was sent: the stripped call, the decided env, the sources.
    let argv = s.sent("argv.txt");
    for a in ["-p", "--model\nsonnet", "--effort\nmedium", "--no-session-persistence", "--output-format\njson", "--strict-mcp-config", "--setting-sources\n\n", "--tools\n\n", "--system-prompt"] {
        assert!(argv.contains(a), "argv lacks {a:?}:\n{argv}");
    }
    assert!(argv.contains("You are Promptu's pass.") && argv.contains("## bench-session"));
    let env = s.sent("env.txt");
    for e in ["EMBARCH_DESK_INTERNAL=1", "PROMPTU_INTERNAL=1", "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1"] {
        assert!(env.contains(e), "{e}");
    }
    let sent = s.sent("stdin.txt");
    assert!(sent.contains(&format!("<prompt>\n{BENCH_ASK}\n</prompt>")));
    assert!(sent.contains("repo: motor-fw\nbranch: feat/reworks\nlinked worktree: no\nfirmware: yes"));
    assert!(sent.contains("[de8cea] - [Cache trap](cache-trap.md)"));
    assert!(sent.contains("[claude-md] - Only I open pull requests."));
    assert!(sent.contains("[claude-md] - Short answers."), "every section by default");
    assert!(sent.contains("<regex_hint>\nbench-session"));

    assert_eq!(out["pattern"]["id"], "bench-session");
    assert_eq!(out["pattern"]["picked_by"], "model");
    assert_eq!(out["offline"], false);
    assert_eq!(out["mode"], "normal");
    assert_eq!(out["flags"], json!({"firmware": true, "hardware": true, "item": false}));
    assert_eq!(out["parts"], json!(["desk", "embarch-api"]));
    assert!(out["choose"].is_null());
    // Kept: the memory fill and the prompt pre-selection of an always-asked
    // question. Dropped: a quote that is not in the prompt.
    let filled: Vec<&str> = out["filled"].as_array().unwrap().iter().map(|f| f["q"].as_str().unwrap()).collect();
    assert_eq!(filled, vec!["board", "flash"]);
    assert_eq!(out["dropped"][0]["q"], "pass");
    assert_eq!(out["dropped"][0]["reason"], "the evidence is not in the source");
    assert_askable(&out);
    // The bench state first (decision 31): board, what is plugged in,
    // instruments, the gate; then the required pass and the relevant optional
    // one. 3 + the Rest control, then the last 3.
    let pages = out["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{out:#}");
    assert_eq!(ids(&pages[0]), vec!["board", "attached", "instruments", "_rest"]);
    assert_eq!(ids(&pages[1]), vec!["gate", "pass", "unblocks"]);
    let board = &pages[0]["questions"][0];
    assert_eq!(board["options"][0]["label"], "Stepper driver board (Recommended)");
    let pass = &pages[1]["questions"][1];
    assert_eq!(pass["options"][0]["label"], "The fault pin stays high through 50 enable toggles (Recommended)");
    let rest = &pages[0]["questions"][3];
    assert_eq!(rest["header"], "Rest");
    let d = rest["options"][1]["description"].as_str().unwrap();
    assert_eq!(d, "The optional ones take their recommendation: Unblocks: The next board revision. Still asked: Bench gate, Pass means.");
    assert!(out["defaults"].as_array().unwrap().iter().any(|d| d["q"] == "heavy" && d["value"] == "Overlay or config first"));
    assert_eq!(out["pass"]["cost_usd"], 0.0123);

    let run = out["run"].as_str().unwrap().to_string();
    let next = s.json(
        &repo,
        &["next", &run],
        &json!({"answers": {
            "board": {"other": "Stepper driver board rev C, unit 2"},
            "attached": "Debug probe, Bench supply",
            "instruments": ["None"],
            "_rest": "Keep going (Recommended)"}})
        .to_string(),
    );
    assert_askable(&next);
    assert_eq!(ids(&next["pages"][0]), vec!["gate", "pass", "unblocks"]);
    let next = s.json(
        &repo,
        &["next", &run],
        &json!({"answers": {"gate": "My go for each step (Recommended)", "pass": "The fault pin stays high through 50 enable toggles (Recommended)", "unblocks": "Leave it out"}}).to_string(),
    );
    assert_eq!(next["done"], true);
    assert!(next["pages"].as_array().unwrap().is_empty());

    let r = s.json(&repo, &["render", &run], "");
    let prompt = r["prompt"].as_str().unwrap();
    assert!(prompt.starts_with(&format!("## Goal\n{BENCH_ASK}\n\n## Context")), "{prompt}");
    assert!(prompt.contains("- Board: Stepper driver board rev C, unit 2"));
    assert!(prompt.contains("- A debug probe (SWD/JTAG) is attached."));
    assert!(prompt.contains("- It is powered from a bench supply."));
    assert!(prompt.contains("- Instruments: no instruments are connected."));
    assert!(prompt.contains("Pass means: The fault pin stays high through 50 enable toggles."));
    assert!(prompt.contains("- Build, flash and run through EmbArch's tools."));
    assert!(prompt.contains("- If firmware must change for this, use the lightest form"));
    assert!(!prompt.contains("It unblocks"));
    assert_eq!(
        r["footer"],
        "— promptu bench-session@1 asked=pass:rec,instruments:o4,gate:rec,unblocks:o1,board:other,attached:o1+o2 filled=flash:memory.de8cea skipped=heavy,timebox"
    );
    assert!(prompt.ends_with(r["footer"].as_str().unwrap()));
    assert_eq!(r["sections"], json!(["Goal", "Context", "Done when", "Constraints", "Who does it"]));
    let draft = PathBuf::from(r["draft"].as_str().unwrap());
    assert!(draft.starts_with(s.root.join("state/drafts")));
    assert_eq!(std::fs::read_to_string(draft).unwrap(), prompt);

    // `--text` prints the prompt alone; render is repeatable.
    let o = s.run(&repo, &["render", &run, "--text"], "", &[]);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim_end(), prompt);

    // The next run in this project sends last time's answers as `last`.
    let o = s.run(&repo, &["prep"], "Back on the bench for the stepper driver board", &[]);
    assert_eq!(o.status.code(), Some(0));
    let sent = s.sent("stdin.txt");
    assert!(sent.contains("bench-session.gate: My go for each step"), "{sent}");
    assert!(sent.contains("bench-session.flash: EmbArch"));
    // Answering a filled question overrides the fill: `!` only when it changed.
    let run2 = s.json(&repo, &["prep", "--have", "embarch-api"], BENCH_ASK)["run"].as_str().unwrap().to_string();
    let r = s.json(&repo, &["render", &run2, "--rest", "recommended"], &json!({"answers": {"flash": "EmbArch (Recommended)"}}).to_string());
    assert!(r["footer"].as_str().unwrap().contains("filled=flash:memory.de8cea "), "{}", r["footer"]);
    let run3 = s.json(&repo, &["prep", "--have", "embarch-api"], BENCH_ASK)["run"].as_str().unwrap().to_string();
    let r = s.json(&repo, &["render", &run3, "--rest", "recommended"], &json!({"answers": {"flash": "The vendor tool"}}).to_string());
    assert!(r["footer"].as_str().unwrap().contains("filled=flash:memory.de8cea! "), "{}", r["footer"]);
    assert!(r["prompt"].as_str().unwrap().contains("- Flash with the chip vendor's own tool."));

    // An always-asked question still gets last time's answer, as a suggestion only.
    assert!(sent.contains("bench-session.board: Stepper driver board rev C, unit 2"));
    assert!(!sent.contains("bench-session.instruments:"), "instruments takes only the prompt");
}

#[test]
fn take_the_rest_as_recommended_and_esc() {
    let s = Scratch::new("rest");
    let repo = s.repo("motor-fw", "main");
    s.reply(bench_reply());
    let out = s.json(&repo, &["prep"], BENCH_ASK);
    let run = out["run"].as_str().unwrap().to_string();
    assert_eq!(ids(&out["pages"][0]), vec!["board", "attached", "instruments", "_rest"]);
    // "Take the rest as recommended" on page 1: the optional questions left
    // (flash, unblocks) take their recommendation, but the required `pass`
    // and the always-asked gate are still asked, on a page of their own.
    let r = s.json(
        &repo,
        &["next", &run],
        &json!({"answers": {"board": "Stepper driver board (Recommended)", "attached": "Debug probe", "instruments": "None", "_rest": "Take the rest as recommended"}}).to_string(),
    );
    assert_eq!(r["done"], false);
    assert_askable(&r);
    assert_eq!(ids(&r["pages"][0]), vec!["gate", "pass"], "{r:#}");
    let r = s.json(&repo, &["next", &run], &json!({"answers": {"gate": "My go for each step (Recommended)", "pass": {"other": "it boots"}}}).to_string());
    assert_eq!(r["done"], true);
    let r = s.json(&repo, &["render", &run], "");
    assert_eq!(
        r["footer"],
        "— promptu bench-session@1 asked=pass:other,instruments:o4,gate:rec,flash:all,unblocks:all,board:rec,attached:o1 skipped=heavy,timebox"
    );

    // Esc on the first page: what was not answered becomes stated assumptions.
    let out = s.json(&repo, &["prep"], BENCH_ASK);
    let run = out["run"].as_str().unwrap().to_string();
    let r = s.json(&repo, &["render", &run, "--rest", "assume"], "");
    let p = r["prompt"].as_str().unwrap();
    assert!(p.contains("- Not answered; state your assumption before you start: What observation on the bench means it passed? Which instruments are wired up?"), "{p}");
    assert!(r["footer"].as_str().unwrap().contains(" assume=pass,instruments,gate,flash,unblocks,board,attached"), "{}", r["footer"]);
    assert!(p.contains("- Board: not stated. Ask me before relying on it.\n- What is plugged in: not stated."), "{p}");
    // Rendering with pages left and no `rest` means the same as Esc.
    let out = s.json(&repo, &["prep"], BENCH_ASK);
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap()], "");
    assert!(r["footer"].as_str().unwrap().contains(" assume="));

    // --rec: no questions at all.
    let out = s.json(&repo, &["prep", "--rec"], BENCH_ASK);
    assert_eq!(out["done"], true);
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap()], "");
    assert!(r["footer"].as_str().unwrap().contains("pass:all"));
}

#[test]
fn a_weak_pick_opens_the_chip_row_and_a_switch_is_recorded() {
    let s = Scratch::new("chip");
    let repo = s.repo("app", "main");
    s.reply(json!({"pattern": "implement-issue", "confidence": 0.4, "alt": ["debug-failure"], "flags": {}, "filled": {}, "candidates": {}, "relevant": {}, "ambiguous": []}));
    let out = s.json(&repo, &["prep"], "the upload is slow, look at it");
    assert_askable(&out);
    let chip = &out["choose"]["questions"][0];
    assert_eq!(chip["header"], "Pattern");
    let labels: Vec<&str> = chip["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap()).collect();
    assert_eq!(labels[0], "implement-issue (Recommended)");
    assert!(labels.contains(&"debug-failure") && labels.contains(&"No pattern, send as is"));
    assert!(out["pages"].as_array().unwrap().is_empty());
    let run = out["run"].as_str().unwrap().to_string();
    let next = s.json(&repo, &["next", &run], &json!({"pattern": "debug-failure"}).to_string());
    assert_eq!(next["pattern"]["id"], "debug-failure");
    assert_eq!(next["pattern"]["picked_by"], "owner");
    assert_eq!(ids(&next["pages"][0]), vec!["symptom", "first"]);
    let r = s.json(&repo, &["render", &run, "--rest", "recommended"], "");
    assert!(r["footer"].as_str().unwrap().ends_with(" pick=owner"), "{}", r["footer"]);

    // A re-pick keeps a fill only where it fits the new pattern's question:
    // review-loop's `stop` label is not one of delegate-agents' `stop` labels,
    // so it is dropped and asked, never left empty under `filled`.
    s.reply(json!({"pattern": "review-loop", "confidence": 0.5, "alt": ["delegate-agents"], "flags": {},
        "filled": {"stop": {"value": "No high or medium left", "source": "prompt", "evidence": "until no high or medium is left"},
                   "rules": {"value": ["Only I open PRs"], "source": "prompt", "evidence": "only I open PRs here"}},
        "candidates": {}, "relevant": {}}));
    let out = s.json(&repo, &["prep"], "workers review it until no high or medium is left; only I open PRs here");
    let run = out["run"].as_str().unwrap().to_string();
    let next = s.json(&repo, &["next", &run], &json!({"pattern": "delegate-agents"}).to_string());
    let filled: Vec<&str> = next["filled"].as_array().unwrap().iter().map(|f| f["q"].as_str().unwrap()).collect();
    assert_eq!(filled, vec!["rules"], "{next:#}");
    assert!(next["dropped"].as_array().unwrap().iter().any(|d| d["q"] == "stop" && d["reason"].as_str().unwrap().contains("delegate-agents")));
    assert!(next["pages"].as_array().unwrap().iter().any(|p| ids(p).contains(&"stop".to_string())), "{next:#}");
    let r = s.json(&repo, &["render", &run, "--rest", "recommended"], "");
    assert!(r["footer"].as_str().unwrap().contains("stop:all"), "{}", r["footer"]);
    assert!(r["prompt"].as_str().unwrap().contains("A worker stops when its brief's done-check passes"));

    // "No pattern" sends the rough prompt as typed; the chip row's answer may
    // also arrive as its page's `_pattern` id.
    let out = s.json(&repo, &["prep"], "push it");
    let run = out["run"].as_str().unwrap().to_string();
    s.json(&repo, &["next", &run], &json!({"answers": {"_pattern": "No pattern, send as is"}}).to_string());
    let r = s.json(&repo, &["render", &run], "");
    assert_eq!(r["prompt"], "push it\n\n— promptu none");
    // The pass finds none and no keyword hits: nothing to choose between, so
    // no chip row (it would have one option) and the prompt goes as typed.
    s.reply(json!({"pattern": "none", "confidence": 0.9, "alt": [], "flags": {}}));
    let out = s.json(&repo, &["prep"], "thanks, that was all");
    assert!(out["choose"].is_null(), "{out:#}");
    assert!(out["pattern"].is_null());
    assert_eq!(out["done"], true);
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap()], "");
    assert_eq!(r["prompt"], "thanks, that was all\n\n— promptu none");
    // With one keyword hit it has a pattern and "No pattern": two options.
    let out = s.json(&repo, &["prep"], "thanks, now investigate it");
    assert_askable(&out);
    assert_eq!(out["choose"]["questions"][0]["options"].as_array().unwrap().len(), 2);

    // So does --pattern none, without calling the model.
    std::fs::remove_file(s.root.join("fake/argv.txt")).unwrap();
    let out = s.json(&repo, &["prep", "--pattern", "none"], "push it");
    assert!(s.sent("argv.txt").is_empty());
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap()], "");
    assert_eq!(r["footer"], "— promptu none");
}

#[test]
fn the_pass_failing_falls_back_to_the_keyword_pick() {
    let s = Scratch::new("offline");
    let repo = s.repo("app", "main");
    let o = s.run(&repo, &["prep"], "investigate why the sensor node crashes after an update", &[("PROMPTU_CLAUDE", "/nonexistent/claude")]);
    assert_eq!(o.status.code(), Some(0));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["offline"], true);
    assert!(out["offline_reason"].as_str().unwrap().contains("could not run"));
    assert_eq!(out["pattern"]["id"], "debug-failure");
    assert_eq!(out["pattern"]["picked_by"], "regex");
    assert!(out["choose"].is_object());

    // Too slow: killed at the configured timeout.
    std::fs::write(s.root.join("cfg/config.toml"), "pass_timeout_s = 1\n").unwrap();
    s.reply(json!({"pattern": "debug-failure"}));
    let t0 = std::time::Instant::now();
    let o = s.run(&repo, &["prep"], "investigate the crash", &[("PROMPTU_FAKE_SLEEP", "5")]);
    assert!(t0.elapsed().as_secs() < 4);
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(out["offline_reason"].as_str().unwrap().contains("longer than 1 s"));

    // 300 KB on stderr and a grandchild that keeps stdout open after the
    // reply: neither holds prep past the grace.
    std::fs::write(s.root.join("cfg/config.toml"), "pass_timeout_s = 15\n").unwrap();
    s.reply(json!({"pattern": "debug-failure", "confidence": 0.9}));
    let t0 = std::time::Instant::now();
    let o = s.run(&repo, &["prep"], "investigate the crash", &[("PROMPTU_FAKE_NOISE", "1")]);
    assert!(t0.elapsed().as_secs() < 4, "{:?}", t0.elapsed());
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(out["offline"], false, "{out:#}");
    assert_eq!(out["pattern"]["id"], "debug-failure");

    // No JSON in the reply.
    std::fs::write(s.root.join("fake/reply.json"), json!({"is_error": false, "result": "sorry"}).to_string()).unwrap();
    let out = s.json(&repo, &["prep"], "investigate the crash");
    assert_eq!(out["offline_reason"], "the pass returned no JSON");

    // --offline skips the model on purpose; the effort setting reaches argv.
    std::fs::write(s.root.join("cfg/config.toml"), "pass_effort = \"low\"\n").unwrap();
    let out = s.json(&repo, &["prep", "--offline"], "investigate the crash");
    assert_eq!(out["offline_reason"], "--offline");
    s.reply(json!({"pattern": "debug-failure", "confidence": 0.9}));
    s.json(&repo, &["prep"], "investigate the crash");
    assert!(s.sent("argv.txt").contains("--effort\nlow\n"));
}

#[test]
fn a_pasted_brief_keeps_its_words_and_gets_only_its_gaps() {
    let s = Scratch::new("brief");
    let repo = s.repo("logger", "main");
    let filler = "The logger tools export CSV today and the reader wants JSON beside it. ".repeat(14);
    let brief = format!("Continue the export work on the logger tools.\n\nRead first:\n1. docs/export.md\n2. src/export.rs\n3. the open issue\n\n## Hard rules\n- Do not change the CSV format.\n\n## Work, in order\n{filler}\n");
    s.reply(json!({"pattern": "implement-issue", "confidence": 0.85, "alt": [], "flags": {"brief": false},
        "filled": {"goal": {"value": "JSON beside CSV", "source": "prompt", "evidence": "the reader wants JSON beside it"}},
        "candidates": {}, "relevant": {"heavy": true, "read": true}, "ambiguous": []}));
    let out = s.json(&repo, &["prep"], &brief);
    assert_eq!(out["mode"], "brief", "seen without the model");
    // Only the required questions the brief left open (and none optional).
    assert_eq!(ids(&out["pages"][0]), vec!["done", "plan"]);
    let run = out["run"].as_str().unwrap().to_string();
    s.json(&repo, &["next", &run], &json!({"answers": {"done": "Build and tests pass (Recommended)"}}).to_string());
    let r = s.json(&repo, &["render", &run, "--rest", "assume"], "");
    let p = r["prompt"].as_str().unwrap();
    assert!(p.starts_with(brief.trim_end()), "the brief stays verbatim");
    let added = &p[brief.trim_end().len()..];
    assert_eq!(
        added.split("\n\n— promptu").next().unwrap(),
        "\n\n## Done when\n- It builds and the existing tests pass. Show the exact commands and their output, and don't edit existing tests to make them pass.\n\n## Constraints\n- Not answered; state your assumption before you start: What happens before any file changes?"
    );
    assert_eq!(r["footer"], "— promptu implement-issue@1 asked=done:rec filled=goal:prompt assume=plan mode=brief");
}

#[test]
fn ambiguity_conditions_and_the_desk() {
    let s = Scratch::new("relay");
    let repo = s.repo("fan-fw", "main");
    let desk = s.root.join("desk.json");
    std::fs::write(&desk, json!({"key": "ABC-58", "title": "Enclosure fan header", "url": "https://example.com/ABC-58", "text": "The ME asks about the fan rail."}).to_string()).unwrap();
    s.reply(json!({"pattern": "colleague-relay", "confidence": 0.9, "alt": [], "flags": {},
        "filled": {}, "candidates": {"tag": ["the mechanical engineer"]}, "relevant": {"check": false, "speed": false, "reply_to": true, "tag": true},
        "ambiguous": [{"phrase": "the rail", "meanings": ["the 5 V rail", "the 12 V rail"]}]}));
    let out = s.json(&repo, &["prep", "--desk", desk.to_str().unwrap()], "The ME asked: can the fan run from the rail at full speed? respond");
    assert!(s.sent("stdin.txt").contains("<desk>\nWork item ABC-58: Enclosure fan header\nThe ME asks about the fan rail.\n</desk>"));
    assert_eq!(out["flags"]["item"], true);
    assert_askable(&out);
    // The meaning first. reply_to and tag test the deliverable, and its
    // recommendation leaves them out, so they are not on the forecast.
    assert_eq!(ids(&out["pages"][0]), vec!["_meaning", "deliverable"]);
    assert_eq!(out["pages"].as_array().unwrap().len(), 1);
    let run = out["run"].as_str().unwrap().to_string();
    let next = s.json(&repo, &["next", &run], &json!({"answers": {"_meaning": "the 5 V rail", "deliverable": ["Explain it to me", "Draft a reply"]}}).to_string());
    assert_askable(&next);
    assert_eq!(ids(&next["pages"][0]), vec!["tag", "reply_to"]);
    let r = s.json(&repo, &["render", &run, "--rest", "recommended"], "");
    let p = r["prompt"].as_str().unwrap();
    assert!(p.contains("By \"the rail\" I mean the 5 V rail."), "{p}");
    assert!(p.contains("The reply goes to the thread the message came from, tagging the mechanical engineer."));
    assert!(p.contains("- Write the reply with the my-voice skill."));
    assert!(r["footer"].as_str().unwrap().starts_with("— promptu colleague-relay@1 asked=meaning:o1,deliverable:o1+o2,reply_to:all,tag:all"));

    // Without "Draft a reply" the two conditional questions never come up.
    let out = s.json(&repo, &["prep"], "The ME asked: can the fan run from the rail? explain");
    let run = out["run"].as_str().unwrap().to_string();
    let next = s.json(&repo, &["next", &run], &json!({"answers": {"_meaning": {"other": "the battery rail"}, "deliverable": "Explain it to me (Recommended)"}}).to_string());
    assert_eq!(next["done"], true, "{next:#}");
    let r = s.json(&repo, &["render", &run], "");
    assert!(r["prompt"].as_str().unwrap().contains("By \"the rail\" I mean the battery rail."));
    assert_eq!(r["footer"], "— promptu colleague-relay@1 asked=meaning:other,deliverable:rec skipped=check,speed");
}

#[test]
fn prep_degrades_instead_of_failing() {
    let s = Scratch::new("degrade");
    let repo = s.repo("app", "main");
    s.reply(json!({"pattern": "review-loop", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {}}));
    // An unreadable work item: the desk source is left out.
    let out = s.json(&repo, &["prep", "--desk", "/no/such/item.json"], "review the branch");
    assert_eq!(out["flags"]["item"], false);
    assert!(out["warnings"][0].as_str().unwrap().contains("the work item could not be read"), "{out:#}");
    assert!(s.sent("stdin.txt").contains("<desk>\n(none)\n</desk>"));
    assert!(out["prompt"].is_null() && !out["pages"].as_array().unwrap().is_empty());
    // The state folder cannot be written: the prompt comes back at once.
    let blocked = s.root.join("state-is-a-file");
    std::fs::write(&blocked, "").unwrap();
    let o = s.run(&repo, &["prep"], "review the branch", &[("PROMPTU_STATE_DIR", blocked.to_str().unwrap())]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let out: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(out["warnings"][0].as_str().unwrap().contains("the run could not be saved"), "{out:#}");
    assert_eq!(out["done"], true);
    let p = out["prompt"].as_str().unwrap();
    assert!(p.starts_with("## Goal\nreview the branch\n"), "{p}");
    assert!(out["footer"].as_str().unwrap().starts_with("— promptu review-loop@1 asked=stop:all"));
}

#[test]
fn a_skipped_house_rules_question_renders_nothing() {
    let s = Scratch::new("house-rules");
    let repo = s.repo("app", "main");
    s.reply(json!({"pattern": "review-loop", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {"rules": false}}));
    let out = s.json(&repo, &["prep"], "review the branch");
    assert!(out["defaults"].as_array().unwrap().iter().any(|d| d["q"] == "rules" && d["value"] == "nothing"), "{out:#}");
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap(), "--rest", "recommended"], "");
    let p = r["prompt"].as_str().unwrap();
    assert!(!p.contains("Only I open pull requests"), "{p}");
    assert!(r["footer"].as_str().unwrap().ends_with(" skipped=commits,budget,rules"), "{}", r["footer"]);
}

#[test]
fn a_typed_read_first_answer_renders_as_itself() {
    let s = Scratch::new("read-first");
    let repo = s.repo("app", "main");
    let desk = s.root.join("desk.json");
    std::fs::write(&desk, json!({"key": "ABC-7", "title": "Export", "url": "https://example.com/ABC-7"}).to_string()).unwrap();
    s.reply(json!({"pattern": "implement-issue", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {"read": true}}));
    for (answer, want) in [
        (json!({"other": "docs/export.md; there are no design files"}), "## Read first\ndocs/export.md; there are no design files\n"),
        (json!("The issue and its PR"), "## Read first\nRead the work item's description, every comment and any linked PR with the desk's linear_get_issue tool before anything else.\n"),
    ] {
        let out = s.json(&repo, &["prep", "--have", "desk", "--desk", desk.to_str().unwrap()], "work on ABC-7");
        let run = out["run"].as_str().unwrap().to_string();
        let r = s.json(&repo, &["render", &run, "--rest", "recommended"], &json!({"answers": {"read": answer}}).to_string());
        assert!(r["prompt"].as_str().unwrap().contains(want), "{}", r["prompt"]);
    }
}

#[test]
fn pasted_markdown_in_the_rough_prompt_stays_as_typed() {
    let s = Scratch::new("markdown");
    let repo = s.repo("app", "main");
    let rough = "Fix the export, details below.\n\n\n## Constraints\n\n- keep the header row\n\n- quote every field\n\n## Empty heading\n\nThat is all.";
    s.reply(json!({"pattern": "implement-issue", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {}}));
    let out = s.json(&repo, &["prep"], rough);
    assert_eq!(out["mode"], "normal");
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap(), "--rest", "assume"], "");
    let p = r["prompt"].as_str().unwrap();
    assert!(p.starts_with(&format!("## Goal\n{rough}\n\n## ")), "{p}");
    // His "## Constraints" keeps its list; the assumptions go to the template's.
    let after = &p[rough.len()..];
    assert!(after.contains("## Constraints\n") && after.contains("Not answered; state your assumption"), "{p}");
    assert_eq!(r["sections"].as_array().unwrap().iter().filter(|x| *x == "Constraints").count(), 1);
}

#[cfg(unix)]
#[test]
fn render_returns_the_prompt_when_its_own_writes_fail() {
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new("render-writes");
    let repo = s.repo("app", "main");
    s.reply(json!({"pattern": "review-loop", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {}}));
    let out = s.json(&repo, &["prep"], "review the branch");
    let run = out["run"].as_str().unwrap().to_string();
    // No drafts folder can be made, and the runs folder is read-only.
    std::fs::write(s.root.join("state/drafts"), "").unwrap();
    let runs = s.root.join("state/runs");
    std::fs::set_permissions(&runs, std::fs::Permissions::from_mode(0o555)).unwrap();
    let o = s.run(&repo, &["render", &run, "--rest", "recommended"], "", &[]);
    std::fs::set_permissions(&runs, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(r["draft"].is_null(), "{r:#}");
    assert!(r["prompt"].as_str().unwrap().starts_with("## Goal\nreview the branch"));
}

#[test]
fn usage_errors_exit_2() {
    let s = Scratch::new("usage");
    let repo = s.repo("app", "main");
    let code = |args: &[&str], stdin: &str| s.run(&repo, args, stdin, &[]).status.code();
    assert_eq!(code(&["prep"], "  \n"), Some(2));
    assert_eq!(code(&["prep", "--pattern", "nope"], "x"), Some(2));
    assert_eq!(code(&["render", "no-such-run"], ""), Some(2));
    assert_eq!(code(&["render", "../../etc/passwd"], ""), Some(2));
    assert_eq!(code(&["next", "no-such-run"], "{}"), Some(2));
    s.reply(json!({"pattern": "review-loop", "confidence": 0.9}));
    let out = s.json(&repo, &["prep"], "review the branch");
    let run = out["run"].as_str().unwrap();
    assert_eq!(code(&["next", run], "not json"), Some(2));
    assert_eq!(code(&["next", run], &json!({"answers": {"nope": "x"}}).to_string()), Some(2));
    assert_eq!(code(&["next", run], &json!({"rest": "later"}).to_string()), Some(2));
    assert_eq!(code(&["lint", "/no/such/dir"], ""), Some(2));
}

#[test]
fn lint_passes_the_shipped_set_and_fails_a_broken_one() {
    let s = Scratch::new("lint");
    let o = s.run(&s.root, &["lint"], "", &[]);
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stdout));
    assert!(String::from_utf8_lossy(&o.stdout).ends_with("13 file(s): 0 error(s), 0 warning(s)\n"));
    let dir = s.root.join("pats");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("x.md"), "---\nname: x\ndescription: d\nversion: 1\nkind: task\nquestions:\n  - {id: a, header: Way too long header, ask: A?, type: choice, options: [{label: One}], recommended: One, sources: [prompt], gap: G1, why: w}\n---\n## Goal\n{{{ask}}} {{b}}\n").unwrap();
    let o = s.run(&s.root, &["lint", dir.to_str().unwrap(), "--json"], "", &[]);
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    let checks: Vec<&str> = v["files"][0]["findings"].as_array().unwrap().iter().map(|f| f["check"].as_str().unwrap()).collect();
    assert!(checks.contains(&"header") && checks.contains(&"options") && checks.contains(&"slot"), "{checks:?}");
}

/// What must not show to someone without EmbArch.
const EMBARCH_WORDS: [&str; 6] = ["embarch", "desk", "linear_get_issue", "build_and_flash", "study tool", "atlas"];

fn embarch_words_in(text: &str) -> Vec<&'static str> {
    let t = text.to_lowercase();
    EMBARCH_WORDS.iter().copied().filter(|w| t.contains(w)).collect()
}

/// Every task pattern's name, and every question id across the set.
fn shipped_patterns() -> (Vec<String>, Vec<String>) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("patterns");
    let mut names = vec![];
    let mut ids = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        let file = e.file_name().to_string_lossy().to_string();
        if !file.starts_with('_') {
            names.push(file.trim_end_matches(".md").to_string());
        }
        for l in text.lines() {
            if let Some(id) = l.strip_prefix("  - id: ") {
                ids.push(id.trim().to_string());
            }
        }
    }
    names.sort();
    (names, ids)
}

#[test]
fn every_pattern_works_without_embarch() {
    let s = Scratch::new("standalone");
    let repo = s.repo("app", "main");
    let (names, qids) = shipped_patterns();
    assert_eq!(names.len(), 11);
    let relevant: serde_json::Map<String, Value> = qids.iter().map(|i| (i.clone(), json!(true))).collect();
    for name in &names {
        s.reply(json!({"pattern": name, "confidence": 0.9, "alt": [], "flags": {"hardware": true, "firmware": true}, "filled": {}, "relevant": relevant, "ambiguous": []}));
        // Every page, answered with each question's first option, to the end.
        let mut out = s.json(&repo, &["prep"], "do the thing on the board");
        assert_eq!(out["parts"], json!([]), "auto, and the session said it has nothing");
        let run = out["run"].as_str().unwrap().to_string();
        let mut rounds = 0;
        while let Some(page) = out["pages"].as_array().and_then(|p| p.first()).cloned() {
            assert_askable(&out);
            for q in page["questions"].as_array().unwrap() {
                let text = serde_json::to_string(q).unwrap();
                assert!(embarch_words_in(&text).is_empty(), "{name}: {text}");
            }
            let answers: serde_json::Map<String, Value> = ids(&page)
                .into_iter()
                .zip(page["questions"].as_array().unwrap())
                .map(|(id, q)| (id, q["options"][0]["label"].clone()))
                .collect();
            out = s.json(&repo, &["next", &run], &json!({"answers": answers}).to_string());
            rounds += 1;
            assert!(rounds < 10, "{name}: the pages never end");
        }
        let r = s.json(&repo, &["render", &run], "");
        let p = r["prompt"].as_str().unwrap();
        assert!(embarch_words_in(p).is_empty(), "{name}:\n{p}");
        // And with every question at its recommendation.
        let out = s.json(&repo, &["prep"], "do the thing on the board");
        let r = s.json(&repo, &["render", out["run"].as_str().unwrap(), "--rest", "recommended"], "");
        assert!(embarch_words_in(r["prompt"].as_str().unwrap()).is_empty(), "{name}:\n{}", r["prompt"]);
    }
    // Every example, rendered as a session without EmbArch would render it.
    let o = s.run(&s.root, &["lint", "--show", "--have", "none"], "", &[]);
    assert_eq!(o.status.code(), Some(0));
    let shown = String::from_utf8_lossy(&o.stdout);
    for block in shown.split("######## ").skip(1) {
        let body = block.split("\npattern ").next().unwrap();
        assert!(embarch_words_in(body.split_once('\n').unwrap().1).is_empty(), "{block}");
    }
}

#[test]
fn work_item_options_need_a_work_item_not_just_the_desk() {
    let s = Scratch::new("item-gate");
    let repo = s.repo("app", "main");
    let desk = s.root.join("desk.json");
    std::fs::write(&desk, json!({"key": "ABC-9", "title": "Export"}).to_string()).unwrap();
    s.reply(json!({"pattern": "handoff", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {"save_to": true}}));
    let save_to = |out: &Value| -> Vec<String> {
        let pages = out["pages"].as_array().unwrap();
        let page = pages.iter().find(|p| ids(p).contains(&"save_to".to_string())).unwrap();
        let i = ids(page).iter().position(|x| x == "save_to").unwrap();
        page["questions"][i]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap().to_string()).collect()
    };
    // The desk, but no work item in this session: no note on an item that does not exist.
    let out = s.json(&repo, &["prep", "--have", "desk"], "generate a handoff prompt for a new thread");
    assert_eq!(out["parts"], json!(["desk"]));
    assert!(!save_to(&out).iter().any(|l| l.starts_with("A note on the item")), "{:?}", save_to(&out));
    // The desk and a work item: it shows.
    let out = s.json(&repo, &["prep", "--have", "desk", "--desk", desk.to_str().unwrap()], "generate a handoff prompt for a new thread");
    assert_eq!(out["parts"], json!(["desk", "item"]));
    assert!(save_to(&out).iter().any(|l| l == "A note on the item"), "{:?}", save_to(&out));
    // A work item without the desk is still no `item`.
    let out = s.json(&repo, &["prep", "--desk", desk.to_str().unwrap()], "generate a handoff prompt for a new thread");
    assert_eq!(out["parts"], json!([]));
}

#[test]
fn lint_never_has_an_item_without_the_desk() {
    let s = Scratch::new("lint-item");
    let show = |have: &str| {
        let o = s.run(&s.root, &["lint", "--show", "--have", have], "", &[]);
        assert_eq!(o.status.code(), Some(0), "--have {have}");
        let out = String::from_utf8_lossy(&o.stdout).to_string();
        out.split("######## implement-issue example 1\n").nth(1).unwrap().split("\n######## ").next().unwrap().to_string()
    };
    // `item` is not a part the command line can name: no desk, no work-item option.
    assert!(!show("item").contains("linear_get_issue"));
    assert!(!show("item,embarch-api").contains("linear_get_issue"));
    // The desk, and the example's own work item, bring it.
    assert!(show("desk").contains("linear_get_issue"));
}

#[test]
fn a_hidden_label_offered_as_text_is_dropped() {
    let s = Scratch::new("hidden-text");
    let repo = s.repo("app", "main");
    s.reply(json!({"pattern": "implement-issue", "confidence": 0.9, "alt": [], "flags": {}, "relevant": {"read": true},
        "filled": {"read": {"value": "The issue and its PR", "source": "prompt", "evidence": "read the issue and its PR first"}}}));
    let out = s.json(&repo, &["prep"], "add CSV export; read the issue and its PR first");
    assert_eq!(out["parts"], json!([]));
    assert!(out["dropped"].as_array().unwrap().iter().any(|d| d["q"] == "read"), "{out:#}");
    // The model was never shown the label: the catalogue is built without the desk.
    assert!(!s.sent("argv.txt").contains("The issue and its PR"));
    let r = s.json(&repo, &["render", out["run"].as_str().unwrap(), "--rest", "recommended"], "");
    assert!(embarch_words_in(r["prompt"].as_str().unwrap()).is_empty(), "{}", r["prompt"]);
}

#[test]
fn the_config_forces_an_embarch_part_on_or_off() {
    let s = Scratch::new("parts");
    let repo = s.repo("motor-fw", "main");
    s.reply(json!({"pattern": "bench-session", "confidence": 0.9, "alt": [], "flags": {}, "filled": {}, "relevant": {"flash": true}}));
    let flash_labels = |out: &Value| -> Vec<String> {
        out["pages"].as_array().unwrap().iter().flat_map(|p| {
            ids(p).into_iter().zip(p["questions"].as_array().unwrap().clone()).filter(|(id, _)| id == "flash").flat_map(|(_, q)| q["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap().to_string()).collect::<Vec<_>>()).collect::<Vec<_>>()
        }).collect()
    };
    // Auto: the session's word decides.
    let out = s.json(&repo, &["prep", "--have", "embarch-api"], BENCH_ASK);
    assert_eq!(flash_labels(&out)[0], "EmbArch (Recommended)");
    let out = s.json(&repo, &["prep"], BENCH_ASK);
    assert_eq!(flash_labels(&out), vec!["The project's own way (Recommended)", "The vendor tool", "Commands for me"]);
    // Forced off, whatever the session says; forced on with no word from it.
    std::fs::write(s.root.join("cfg/config.toml"), "[embarch]\nembarch-api = \"off\"\ndesk = \"on\"\n").unwrap();
    let out = s.json(&repo, &["prep", "--have", "embarch-api"], BENCH_ASK);
    assert_eq!(out["parts"], json!(["desk"]));
    assert!(!flash_labels(&out).iter().any(|l| l.starts_with("EmbArch")));
    // A fill for a hidden option is dropped, so the question is asked.
    std::fs::write(s.root.join("cfg/config.toml"), "").unwrap();
    s.reply(json!({"pattern": "bench-session", "confidence": 0.9, "alt": [], "flags": {}, "relevant": {},
        "filled": {"flash": {"value": "EmbArch", "source": "prompt", "evidence": "the stepper driver board"}}}));
    let out = s.json(&repo, &["prep"], BENCH_ASK);
    assert_eq!(out["dropped"][0]["q"], "flash");
}

/// Every example of every shipped pattern renders exactly as its golden file
/// (`tests/golden/<pattern>-<n>.md`). `PROMPTU_BLESS=1` rewrites them.
#[test]
fn every_example_renders_as_its_golden_file() {
    let s = Scratch::new("golden");
    let o = s.run(&s.root, &["lint", "--show"], "", &[]);
    assert_eq!(o.status.code(), Some(0));
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let bless = std::env::var_os("PROMPTU_BLESS").is_some();
    let mut seen = 0;
    for block in out.split("######## ").skip(1) {
        let (head, body) = block.split_once('\n').unwrap();
        let (name, n) = head.split_once(" example ").unwrap();
        let body = body.split("\npattern ").next().unwrap().trim_end().to_string() + "\n";
        let file = golden.join(format!("{name}-{n}.md"));
        if bless {
            std::fs::create_dir_all(&golden).unwrap();
            std::fs::write(&file, &body).unwrap();
        }
        let want = std::fs::read_to_string(&file).unwrap_or_else(|_| panic!("{} is missing (PROMPTU_BLESS=1)", file.display()));
        assert_eq!(body, want, "{name} example {n} renders differently from {}", file.display());
        seen += 1;
    }
    assert_eq!(seen, 15);
    assert_eq!(std::fs::read_dir(&golden).unwrap().count(), 15, "a golden file with no example");
}

#[test]
fn start_work_takes_the_desk_default_over_a_never_pattern() {
    let s = Scratch::new("start");
    let repo = s.repo("app", "abc-12-watchdog");
    let never = ["draft-message", "handoff", "colleague-relay", "delegate-agents"];
    let chip_labels = |out: &Value| -> Vec<String> {
        out["choose"]["questions"][0]["options"].as_array().unwrap().iter().map(|o| o["label"].as_str().unwrap().to_string()).collect()
    };
    // The model picks a `never` pattern, confidently, with a fill: Start work
    // takes implement-issue, the fill goes, and the chip row asks once.
    s.reply(json!({"pattern": "draft-message", "confidence": 0.9, "alt": ["handoff", "debug-failure"], "flags": {},
        "filled": {"channel": {"value": "Slack", "source": "prompt", "evidence": "Work on ABC-12 — Slack the vendor"}}, "candidates": {}, "relevant": {}}));
    let out = s.json(&repo, &["prep", "--start"], "Work on ABC-12 — Slack the vendor about the watchdog");
    assert_askable(&out);
    assert_eq!(out["start"], true);
    assert_eq!(out["pattern"]["id"], "implement-issue");
    assert_eq!(out["pattern"]["start_default"], true);
    assert_eq!(out["pattern"]["picked_by"], "model");
    assert!(out["filled"].as_array().unwrap().is_empty(), "{out:#}");
    assert_eq!(out["pattern"]["alt"], json!(["debug-failure"]));
    let labels = chip_labels(&out);
    assert_eq!(labels[0], "implement-issue (Recommended)");
    assert!(labels.iter().all(|l| !never.iter().any(|n| l.starts_with(n))), "{labels:?}");

    // A `possible` pick stands, with its fills and no chip row.
    s.reply(json!({"pattern": "debug-failure", "confidence": 0.85, "alt": ["handoff", "review-loop"], "flags": {}, "filled": {}, "candidates": {}, "relevant": {}}));
    let out = s.json(&repo, &["prep", "--start"], "Work on ABC-13 — the watchdog resets the board");
    assert_eq!((out["pattern"]["id"].as_str(), out["pattern"]["start_default"].as_bool()), (Some("debug-failure"), Some(false)));
    assert_eq!(out["pattern"]["alt"], json!(["review-loop"]));
    assert!(out["choose"].is_null());

    // Offline: the desk's default, never the keyword top.
    let rough = "Work on ABC-14 — draft a Slack message to the team";
    let plain = s.json(&repo, &["prep", "--offline"], rough);
    assert_eq!((plain["pattern"]["id"].as_str(), plain["start"].as_bool()), (Some("draft-message"), Some(false)));
    let out = s.json(&repo, &["prep", "--offline", "--start"], rough);
    assert_eq!(out["pattern"]["id"], "implement-issue");
    assert_eq!((out["pattern"]["picked_by"].as_str(), out["pattern"]["start_default"].as_bool()), (Some("regex"), Some(true)));
    let labels = chip_labels(&out);
    assert_eq!(labels[0], "implement-issue (Recommended)");
    assert!(labels.iter().all(|l| !never.iter().any(|n| l.starts_with(n))), "{labels:?}");
    // --pattern still wins, even a `never` one: the owner asked for it.
    let out = s.json(&repo, &["prep", "--offline", "--start", "--pattern", "handoff"], rough);
    assert_eq!((out["pattern"]["id"].as_str(), out["pattern"]["start_default"].as_bool()), (Some("handoff"), Some(false)));
}

/// The skill names only flags the CLI takes: its `promptu prep` line against `prep --help`.
#[test]
fn the_skills_prep_line_uses_real_flags() {
    let skill = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin/skills/promptu/SKILL.md")).unwrap();
    let line = skill.lines().find(|l| l.starts_with("promptu prep ")).unwrap();
    let s = Scratch::new("skill");
    let help = String::from_utf8(s.run(&s.root, &["prep", "--help"], "", &[]).stdout).unwrap();
    let flags: Vec<&str> = line.split(|c: char| c.is_whitespace() || c == '[' || c == ']').filter(|w| w.starts_with("--")).collect();
    assert!(flags.contains(&"--start") && flags.contains(&"--desk"), "{line}");
    for f in flags {
        assert!(help.contains(f), "{f} is not a prep flag");
    }
    assert!(skill.contains("argument-hint: \"[--item <wNN>] [pattern=<id>] [--rec] <rough prompt>\""));
}
