//! `promptu prep`, `promptu next` and `promptu render` (docs/contract.md
//! §7–§8): gather the sources with no model, run the pass, guard it, plan
//! the pages; take answers page by page; write the prompt and its footer.

use crate::claude;
use crate::config;
use crate::pass::{self, Call, Sources};
use crate::pattern::Set;
use crate::run::{self, Ctx, Flags, Item, Run};
use crate::state;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Memory block budget, in characters: the index first, then `CLAUDE.md`.
const MEMORY_BUDGET: usize = 12_000;

/// A usage error (exit 2) or a failure (exit 3 is for panics only).
pub enum Fail {
    Usage(String),
    Other(String),
}

impl From<String> for Fail {
    fn from(s: String) -> Fail {
        Fail::Other(s)
    }
}

fn read_head(git_root: &Path) -> Option<String> {
    let dot = git_root.join(".git");
    let gitdir = if dot.is_dir() {
        dot
    } else {
        let t = std::fs::read_to_string(&dot).ok()?;
        let g = t.trim().strip_prefix("gitdir:")?.trim().to_string();
        claude::normalize(&git_root.join(g))
    };
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: ") {
        Some(r) => Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string()),
        None => Some(head.chars().take(12).collect()),
    }
}

/// Firmware, judged from the checkout's top level.
fn is_firmware(root: &Path) -> bool {
    ["west.yml", "prj.conf", "Kconfig", "sdkconfig", "platformio.ini", "zephyr/module.yml", "boards"].iter().any(|m| root.join(m).exists())
        || std::fs::read_dir(root).into_iter().flatten().flatten().any(|e| e.path().extension().is_some_and(|x| x == "ioc" || x == "overlay"))
}

pub struct Gathered {
    pub ctx: Ctx,
    pub flags: Flags,
    pub src: Sources,
    pub project_key: Option<String>,
    /// What was skipped and why, one line each (`prep`'s `warnings`).
    pub warnings: Vec<String>,
}

/// The `desk` source: a JSON work item (`key`, `title`, `url`, `id`, `text`)
/// or plain text.
fn read_desk(path: &Path) -> Result<(Option<Item>, String), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    match serde_json::from_str::<Value>(&text) {
        Ok(v @ Value::Object(_)) => {
            let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
            let item = Item { key: s("key"), title: s("title"), url: s("url"), id: s("id") };
            let mut body = vec![];
            if let Some(k) = &item.key {
                body.push(format!("Work item {k}{}", item.title.as_ref().map(|t| format!(": {t}")).unwrap_or_default()));
            }
            body.extend(s("text").or_else(|| s("brief")).or_else(|| s("description")));
            let has = item.key.is_some() || item.title.is_some() || item.url.is_some() || item.id.is_some();
            Ok((has.then_some(item), body.join("\n")))
        }
        _ => Ok((None, text)),
    }
}

/// The project's memory index, each line tagged with its memory id, then
/// the global `CLAUDE.md` bullets tagged `claude-md`, within the budget.
fn memory_source(cwd: &Path, table: &toml::Table, src: &mut Sources) {
    let mut lines: Vec<String> = vec![];
    let mut used = 0;
    if let Some((dir, _)) = claude::auto_memory_dir(cwd) {
        if let Ok(text) = std::fs::read_to_string(dir.join("MEMORY.md")) {
            for l in text.lines() {
                let t = l.trim();
                let tagged = if crate::checks::is_index_line(t) {
                    let Some(target) = crate::checks::link_target(t) else { continue };
                    let name = Path::new(&target).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(target);
                    let id = crate::sha1::memory_id(&name);
                    src.memory_by_id.entry(id.clone()).or_default().push(t.to_string());
                    format!("[{id}] {t}")
                } else if t.starts_with("## ") {
                    t.to_string()
                } else {
                    continue;
                };
                if used + tagged.len() > MEMORY_BUDGET {
                    break;
                }
                used += tagged.len() + 1;
                lines.push(tagged);
            }
        }
    }
    let want = config::claude_md_sections(table);
    if let Some(text) = claude::config_dir().and_then(|d| std::fs::read_to_string(d.join("CLAUDE.md")).ok()) {
        let mut section = String::new();
        let mut header_done = false;
        for l in text.lines() {
            if let Some(h) = l.strip_prefix("## ") {
                section = h.trim().to_string();
                continue;
            }
            if !l.starts_with("- ") || want.as_ref().is_some_and(|w| !w.contains(&section)) {
                continue;
            }
            let tagged = format!("[claude-md] {l}");
            if used + tagged.len() > MEMORY_BUDGET {
                break;
            }
            if !header_done {
                lines.push("## standing rules".into());
                header_done = true;
            }
            used += tagged.len() + 1;
            src.memory_by_id.entry("claude-md".into()).or_default().push(l.to_string());
            lines.push(tagged);
        }
    }
    src.memory = lines.join("\n");
}

pub fn gather(cwd: &Path, prompt: &str, desk: Option<&Path>, pattern_hint: Option<&str>, table: &toml::Table, set: &Set) -> Result<Gathered, Fail> {
    let main_root = claude::project_root(cwd);
    let here_root = claude::git_root(cwd);
    let in_git = here_root.is_some();
    let worktree = here_root.as_ref().filter(|r| claude::normalize(r) != claude::normalize(&main_root)).map(|r| r.display().to_string());
    let branch = here_root.as_deref().and_then(read_head);
    let firmware = here_root.as_deref().is_some_and(is_firmware);
    let repo = in_git.then(|| main_root.file_name().map(|n| n.to_string_lossy().to_string())).flatten();
    let mut warnings = vec![];
    // An unreadable work item drops the desk source; the run goes on.
    let (item, desk_text) = match desk.map(read_desk) {
        Some(Ok(d)) => d,
        Some(Err(e)) => {
            warnings.push(format!("the work item could not be read, so the desk source is left out: {e}"));
            (None, String::new())
        }
        None => (None, String::new()),
    };
    let mut src = Sources { prompt: prompt.to_string(), desk: desk_text, ..Default::default() };
    let mut ctx_lines = vec![];
    if let Some(r) = &repo {
        ctx_lines.push(format!("repo: {r}"));
    }
    if let Some(b) = &branch {
        ctx_lines.push(format!("branch: {b}"));
    }
    ctx_lines.push(format!("linked worktree: {}", worktree.as_deref().unwrap_or("no")));
    ctx_lines.push(format!("firmware: {}", if firmware { "yes" } else { "no" }));
    src.context = ctx_lines.join("\n");
    memory_source(cwd, table, &mut src);
    let project_key = Some(claude::project_folder(&main_root.to_string_lossy()));
    // `last`: this pattern's answers last time, when the pattern is known
    // before the pass (`--pattern`), else every pattern's (the model picks).
    if let Some(key) = &project_key {
        let last = state::load_last(key);
        let mut l = vec![];
        for (pat, lr) in &last.patterns {
            if pattern_hint.is_some_and(|h| h != pat) {
                continue;
            }
            let Some(p) = set.get(pat) else { continue };
            let qs = set.questions(p);
            for (qid, a) in &lr.answers {
                if qs.iter().any(|q| &q.id == qid && q.sources.iter().any(|s| s == "last")) && !a.text.is_empty() {
                    l.push(format!("{pat}.{qid}: {}", a.text));
                }
            }
        }
        src.last = l.join("\n");
    }
    let flags = Flags { firmware, hardware: false, item: item.is_some() };
    let ctx = Ctx { repo, branch, worktree, item, today: config::today().to_string() };
    Ok(Gathered { ctx, flags, src, project_key, warnings })
}

pub struct PrepArgs {
    pub pattern: Option<String>,
    pub rec: bool,
    pub desk: Option<PathBuf>,
    pub offline: bool,
    /// The EmbArch parts the session has (`--have desk,embarch-api`).
    pub have: Vec<String>,
    /// A desk Start work run: the `signals.desk` rules apply (contract §7.8).
    pub start: bool,
}

fn call_for(table: &toml::Table) -> Result<Call, String> {
    Ok(Call {
        claude: std::env::var("PROMPTU_CLAUDE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "claude".into()),
        model: config::pass_model(table),
        effort: config::pass_effort(table).to_string(),
        timeout: config::pass_timeout(table),
        // The model runs in an empty folder; without the state folder, the temp one.
        cwd: state::pass_dir().unwrap_or_else(|_| std::env::temp_dir()),
    })
}

pub fn prep(prompt: &str, args: PrepArgs) -> Result<Value, Fail> {
    let prompt = prompt.trim_end_matches(['\n', '\r']).to_string();
    if prompt.trim().is_empty() {
        return Err(Fail::Usage("no rough prompt on stdin".into()));
    }
    let set = Set::load().map_err(Fail::Usage)?;
    if let Some(p) = &args.pattern {
        if p != "none" && set.get(p).is_none() {
            return Err(Fail::Usage(format!("no pattern `{p}`; known: {}", set.patterns.keys().cloned().collect::<Vec<_>>().join(", "))));
        }
    }
    let table = config::table();
    let cwd = std::env::current_dir().map_err(|e| Fail::Other(e.to_string()))?;
    let g = gather(&cwd, &prompt, args.desk.as_deref(), args.pattern.as_deref(), &table, &set)?;
    state::prune(7);
    let scores = pass::regex_scores(&set, &prompt);
    let mut run = Run {
        promptu: 1,
        run: state::new_run_id(),
        created: chrono::Local::now().to_rfc3339(),
        cwd: cwd.display().to_string(),
        project_key: g.project_key.clone(),
        prompt: prompt.clone(),
        pattern: None,
        version: 0,
        picked_by: "model".into(),
        model_pick: None,
        confidence: None,
        alt: vec![],
        offline: false,
        offline_reason: None,
        brief: pass::looks_like_brief(&prompt),
        flags: g.flags.clone(),
        ctx: g.ctx.clone(),
        fills: BTreeMap::new(),
        dropped: vec![],
        candidates: BTreeMap::new(),
        relevant: BTreeMap::new(),
        ambiguous: None,
        choose: false,
        regex: scores.clone(),
        answers: BTreeMap::new(),
        rest: None,
        parts: config::parts(&table, &args.have),
        ask_first: vec![],
        rest_optional: false,
        start: args.start,
        start_default: false,
    };
    // `item`: the desk is here and this run has a work item from it.
    if run.parts.iter().any(|p| p == "desk") && run.ctx.item.is_some() {
        run.parts.push("item".into());
    }
    let mut cost = None;
    let mut ms = None;
    if args.pattern.as_deref() == Some("none") {
        run.picked_by = "flag".into();
    } else {
        let reply = if args.offline {
            Err(Fail::Other("--offline".to_string()))
        } else {
            call_for(&table).map_err(Fail::Other).and_then(|c| pass::call_model(&c, &pass::system_prompt(&set, &run.parts), &pass::user_message(&g.src, &scores)).map_err(Fail::Other))
        };
        match reply.map(|r| (pass::first_json_object(&r.result), r)) {
            Ok((Some(v), r)) => {
                cost = r.cost_usd;
                ms = Some(r.ms);
                // `--pattern` forces the pick; the model's fills only count for it.
                let mut v = v;
                if let Some(forced) = &args.pattern {
                    if v["pattern"].as_str() != Some(forced) {
                        v["filled"] = json!({});
                        v["candidates"] = json!({});
                        v["relevant"] = json!({});
                    }
                    v["pattern"] = json!(forced);
                }
                // Start work (contract §7.8): no pick, an unknown one or a
                // `never` one takes the desk's default, its fills cleared as
                // a forced pick's are.
                if args.start && args.pattern.is_none() && !v["pattern"].as_str().is_some_and(|p| set.startable(p)) {
                    if let Some(d) = set.desk_default() {
                        v["filled"] = json!({});
                        v["candidates"] = json!({});
                        v["relevant"] = json!({});
                        v["pattern"] = json!(d);
                        v["confidence"] = Value::Null;
                        run.start_default = true;
                    }
                }
                let gd = pass::guard(&v, &set, &g.src, &run.parts);
                run.model_pick = gd.pattern.clone();
                run.pattern = gd.pattern.clone();
                run.confidence = gd.confidence;
                run.alt = gd.alt;
                run.flags.hardware = gd.hardware;
                run.flags.firmware |= gd.firmware;
                run.brief |= gd.brief;
                run.fills = gd.fills;
                run.dropped = gd.dropped;
                run.candidates = gd.candidates;
                run.relevant = gd.relevant;
                run.ambiguous = gd.ambiguous;
                if args.pattern.is_some() {
                    run.picked_by = "flag".into();
                }
            }
            other => {
                let reason = match other {
                    Ok((None, _)) => "the pass returned no JSON".to_string(),
                    Err(Fail::Other(e)) | Err(Fail::Usage(e)) => e,
                    Ok((Some(_), _)) => unreachable!(),
                };
                run.offline = true;
                run.offline_reason = Some(reason);
                match &args.pattern {
                    Some(p) => {
                        run.pattern = Some(p.clone());
                        run.picked_by = "flag".into();
                    }
                    None => {
                        run.pattern = pass::regex_top(&scores);
                        // Start work's default rather than a keyword guess.
                        if args.start {
                            if let Some(d) = set.desk_default() {
                                run.start_default = true;
                                run.pattern = Some(d.to_string());
                            }
                        }
                        run.model_pick = run.pattern.clone();
                        run.picked_by = "regex".into();
                    }
                }
            }
        }
        if run.start {
            run.alt.retain(|a| set.startable(a));
        }
        // The chip row: a weak pick, no pick, or an offline guess.
        // With no pattern to offer it, the run goes on with none: an AskUserQuestion
        // page needs at least two options.
        if args.pattern.is_none() && (run.offline || run.pattern.is_none() || run.confidence.unwrap_or(0.0) < 0.6) {
            run.choose = !run::chip_names(&run, &set).is_empty();
        }
    }
    if let Some(p) = run.pattern(&set) {
        run.version = p.front.version;
        run.ask_first = p.front.ask_first.clone();
        if p.name() == "bench-session" {
            run.flags.hardware = true;
        }
    }
    if args.rec {
        run.rest = Some("recommended".into());
        run.choose = false;
    }
    let mut warnings = g.warnings;
    let saved = state::save_run(&run);
    let mut rendered = None;
    if let Err(e) = saved {
        // No run file, so no `next` or `render` later: nothing is asked, and the
        // prompt comes back now, every question at its recommendation.
        warnings.push(format!("the run could not be saved ({e}), so nothing is asked: the prompt takes every recommendation"));
        run.choose = false;
        run.rest = Some("recommended".into());
        rendered = run::render(&run, &set, &config::template_values(&table)).ok();
    }
    let mut out = describe(&run, &set);
    out["pass"] = json!({ "ms": ms, "cost_usd": cost, "effort": config::pass_effort(&table) });
    out["warnings"] = json!(warnings);
    if let Some(r) = rendered {
        out["prompt"] = json!(r.prompt);
        out["footer"] = json!(r.footer);
    }
    Ok(out)
}

/// What a run looks like to the skill or the chat: prep's and next's output.
pub fn describe(run: &Run, set: &Set) -> Value {
    let p = run.pattern(set);
    let qs = p.map(|p| set.questions(p)).unwrap_or_default();
    let header = |id: &str| qs.iter().find(|q| q.id == id).map(|q| q.header.clone()).unwrap_or_default();
    let filled: Vec<Value> = run
        .fills
        .iter()
        .map(|(q, f)| json!({ "q": q, "header": header(q), "value": f.value, "source": f.source, "entry": f.entry, "evidence": f.evidence }))
        .collect();
    let defaults: Vec<Value> = if run.choose {
        vec![]
    } else {
        run::resolve(run, &qs, true)
            .into_iter()
            .filter_map(|(q, st)| match st {
                run::Status::Skipped(a) => {
                    let t = run::answer_text(&q, &a);
                    Some(json!({ "q": q.id, "header": q.header, "value": if t.is_empty() { "nothing".to_string() } else { t } }))
                }
                _ => None,
            })
            .collect()
    };
    let pages = run::pages(run, set);
    json!({
        "promptu": 1,
        "run": run.run,
        "pattern": p.map(|p| json!({
            "id": p.name(), "version": p.front.version, "picked_by": run.picked_by,
            "confidence": run.confidence, "alt": run.alt, "start_default": run.start_default,
        })),
        "start": run.start,
        "offline": run.offline,
        "offline_reason": run.offline_reason,
        "mode": if run.brief { "brief" } else { "normal" },
        "parts": run.parts,
        "flags": run.flags,
        "filled": filled,
        "dropped": run.dropped,
        "defaults": defaults,
        "choose": run.choose.then(|| json!({ "ids": [run::PATTERN_ID], "questions": [run::choose_question(run, set)] })),
        "pages": pages,
        "done": !run.choose && pages.is_empty(),
    })
}

/// Read `{"run", "answers", "rest", "pattern"}` from stdin into the run.
pub fn apply(input: &Value, run: &mut Run, set: &Set) -> Result<(), Fail> {
    // The chip row's answer, as `pattern` or as the `_pattern` id of its page.
    let chip = input.get("pattern").or_else(|| input.get("answers").and_then(|a| a.get(run::PATTERN_ID)));
    if let Some(p) = chip.and_then(|v| v.as_str()) {
        let p = p.trim();
        let name = p.strip_suffix(run::REC_MARK.trim_start()).map(str::trim_end).unwrap_or(p);
        if name.eq_ignore_ascii_case(run::NO_PATTERN) || name == "none" {
            run.pattern = None;
            run.choose = false;
            if run.model_pick.is_some() {
                run.picked_by = "owner".into();
            }
        } else {
            let pat = set.get(name).ok_or_else(|| Fail::Usage(format!("no pattern `{name}`")))?;
            if run.model_pick.as_deref() != Some(name) {
                run.picked_by = "owner".into();
                // Keep a fill only where the new pattern has the same question and
                // it takes this source and this value (same labels); anything
                // else is dropped, so the question is asked, never left empty.
                let qs = set.questions(pat);
                let mut kept = BTreeMap::new();
                for (id, mut f) in std::mem::take(&mut run.fills) {
                    let q = qs.iter().find(|q| q.id == id && !q.always_ask && q.visible(&run.parts) && q.sources.contains(&f.source));
                    match q.and_then(|q| pass::fit_value(q, &f.value, &run.parts)) {
                        Some(v) => {
                            f.value = v;
                            kept.insert(id, f);
                        }
                        None => run.dropped.push(crate::run::Dropped { q: id, source: f.source, reason: format!("does not fit `{name}`, the pattern picked instead") }),
                    }
                }
                run.fills = kept;
                run.candidates.retain(|id, c| match qs.iter().find(|q| &q.id == id && q.qtype == "text") {
                    Some(q) => {
                        c.truncate((q.suggest as usize).min(3));
                        !c.is_empty()
                    }
                    None => false,
                });
                run.relevant.clear();
            }
            run.pattern = Some(name.to_string());
            run.version = pat.front.version;
            run.ask_first = pat.front.ask_first.clone();
            run.choose = false;
            if name == "bench-session" {
                run.flags.hardware = true;
            }
        }
    }
    if let Some(ans) = input.get("answers").and_then(|v| v.as_object()) {
        let qs = run.pattern(set).map(|p| set.questions(p)).unwrap_or_default();
        for (id, raw) in ans {
            if id == run::REST_ID {
                let s = raw.as_str().unwrap_or_default().to_lowercase();
                // Optional questions only: required and always-asked ones are still asked.
                if s.starts_with("take the rest") {
                    run.rest_optional = true;
                }
                continue;
            }
            if id == run::MEANING_ID {
                if let Some(a) = &run.ambiguous {
                    let m = run::match_meaning(a, raw);
                    run.answers.insert(id.clone(), m);
                }
                continue;
            }
            if id == run::PATTERN_ID {
                continue;
            }
            let q = qs.iter().find(|q| &q.id == id).ok_or_else(|| Fail::Usage(format!("`{id}` is not a question of this run")))?;
            let a = run::match_answer(q, run, raw);
            run.answers.insert(id.clone(), a);
        }
    }
    match input.get("rest").and_then(|v| v.as_str()) {
        None | Some("ask") => {}
        Some("recommended") => run.rest = Some("recommended".into()),
        Some("assume") => run.rest = Some("assume".into()),
        Some(other) => return Err(Fail::Usage(format!("`rest` is ask, recommended or assume, not `{other}`"))),
    }
    Ok(())
}

pub fn next(run_id: &str, input: &Value) -> Result<Value, Fail> {
    let set = Set::load().map_err(Fail::Usage)?;
    let mut run = state::load_run(run_id).map_err(Fail::Usage)?.ok_or_else(|| Fail::Usage(format!("no run `{run_id}`")))?;
    apply(input, &mut run, &set)?;
    state::save_run(&run)?;
    Ok(describe(&run, &set))
}

pub fn render(run_id: &str, input: &Value) -> Result<Value, Fail> {
    let set = Set::load().map_err(Fail::Usage)?;
    let mut run = state::load_run(run_id).map_err(Fail::Usage)?.ok_or_else(|| Fail::Usage(format!("no run `{run_id}`")))?;
    apply(input, &mut run, &set)?;
    if run.choose {
        // Rendering with the chip row still open: the pass's pick stands.
        run.choose = false;
    }
    if run.rest.is_none() && !run::pages(&run, &set).is_empty() {
        // Pages left unanswered: Esc semantics, the rest become assumptions.
        run.rest = Some("assume".into());
    }
    let table = config::table();
    let r = run::render(&run, &set, &config::template_values(&table)).map_err(Fail::Other)?;
    // The prompt is the result: none of the writes below can fail it. A run
    // not saved only means a later render repeats this one's answers.
    let _ = state::save_run(&run);
    let draft = state::write_draft(&run.run, &r.prompt).ok();
    if let (Some(key), Some(p)) = (&run.project_key, run.pattern(&set)) {
        let answers = r.last.iter().map(|(k, (a, t))| (k.clone(), state::LastAnswer { answer: a.clone(), text: t.clone() })).collect();
        let _ = state::save_last(key, p.name(), state::LastRun { date: config::today().to_string(), version: p.front.version, answers });
    }
    let mut out = json!({
        "promptu": 1,
        "run": run.run,
        "pattern": run.pattern,
        "prompt": r.prompt,
        "footer": r.footer,
        "sections": r.sections,
    });
    if let Some(d) = draft {
        out["draft"] = json!(d.display().to_string());
    }
    Ok(out)
}
