# Promptu: design forks for the owner

Choices the approved design does not settle, met while building. **No agent decides one.** An open fork runs on a provisional default, the most reversible one, until the owner picks; the default is marked. Ranked by consequence within each step. Step 1 (memory): F1–F10. Step 2 (the terminal pass): S1–S8, at the end.

**Decided by the owner on 2026-10-29:** F1 (a), F2 (b), F3 (d) in `embarch-umbrella`, F4 (b); each is marked below and recorded as a numbered decision in `embarch-doc/embarch-promptu/decisions.md`. **F5–F10 are open.**

## Decide before block mode is turned on (F1–F4 decided)

### F1. T7 compares fields level-blind and skips Claude Code's own three — decided

**Decided by the owner, 2026-10-29: (a).** Built as such since step 1.


The design's T7 refuses any frontmatter key the write drops. Measured on Claude Code 2.1.287: on a Write into a memory folder the harness moves every top-level key except `name` and `description` under `metadata:`, and stamps `node_type`, `originSessionId` and `modified` afresh. Read literally, T7 then refuses an ordinary rewrite: `kind` "vanishes" from the top level when it sits under `metadata:`, and Claude is told to keep a session id the harness replaces anyway.

- **(a) Level-blind, the stamped three exempt.** `kind` and `metadata.kind` are one field; dropping `type`, `source`, `review-by` or any other field is still refused.
- (b) Literal: every key at every level. Refuses rewrites that lose nothing.

### F2. CLAUDE.md P1 says "at the top level"; the harness moves the field — decided

**Decided by the owner, 2026-10-29: (b).** The owner's `CLAUDE.md` is his to edit and was changed outside this repo; T2's message now says "add `kind:` to the frontmatter" to match.


P1, now live in the global `CLAUDE.md`, says to add `kind:` "at the top level of its frontmatter, beside Claude Code's own `type`". The harness moves it under `metadata:` on the next Write, so a file Claude wrote by the rule does not look like the rule afterwards. Every check reads both levels, so nothing breaks either way. This is the owner's text, so it needs his OK.

- (a) Keep the wording.
- **(b) Reword to "in its frontmatter (Claude Code may move it under `metadata:`)".** The rule then matches what lands on disk, and a reader comparing a file with the rule is not misled.

### F3. Where the plugin is listed — decided

**Decided by the owner, 2026-10-29: (d), in `embarch-umbrella`.** The `embarch` marketplace moves there and lists every suite plugin; the id stays `embarch-promptu@embarch`. **Moved and re-registered 2026-10-29:** `embarch-umbrella/.claude-plugin/marketplace.json` lists this plugin beside `embarch-desk` and `embarch-chat`, each a `git-subdir` source (`url` + `path: plugin`); a `github` source with `path` installed the whole repo instead [measured on Claude Code 2.1.287]. embarch-desk's file is gone.


The design names the plugin `embarch-promptu@embarch`. The `embarch` marketplace lived in embarch-desk's repo (`.claude-plugin/marketplace.json`), which this build may not edit.

- (a) Add an `embarch-promptu` entry to the `embarch` marketplace in embarch-desk's repo, its source pointing at this repo. Keeps the decided id; one marketplace for the suite. Unknown: whether a marketplace entry can point at a subfolder (`plugin/`) of another GitHub repo; check before doing it.
- (b) A marketplace file in this repo named `embarch`: clashes with the desk's when both are added.
- (c) A marketplace here named `embarch-promptu`: the id becomes `embarch-promptu@embarch-promptu`, against the decision.
- **(d) Move the `embarch` marketplace to a neutral repo listing every suite plugin.**
- Until the move, nothing ships a marketplace entry. Tests install from a scratch marketplace under `/tmp` into a scratch `CLAUDE_CONFIG_DIR`.

### F4. I3 on a brand-new index — decided

**Decided by the owner, 2026-10-29: (b), extended to an existing index with no headers yet.** Built: I3 applies only once the index has a header.

I3 refuses a `## ` header the index did not have. A project with no `MEMORY.md` yet has no headers, so in block mode Claude's first index was refused whatever it wrote: a header tripped I3, a line without one tripped I2. An index written before domains existed had the same deadlock.

- (a) As designed: refuse; Claude asks the owner for the project's domains.
- **(b) Headers are free when the index is created, and checked from then on.**
- (c) Seed new indexes from a per-machine default domain list in `config.toml`.

### F5. Writes the hook cannot see

The hook sees Write, Edit and MultiEdit. A shell redirect, `sed -i`, `cp` or `mv` into a memory folder goes unchecked (a rename by `mv` is the case the memory test found).

- (a) Accept it — *provisional default.* `promptu memory check` and the menu's flags still show the result afterwards.
- (b) A PreToolUse on Bash that refuses a command naming a memory folder as a write target. Heuristic: shell syntax is open-ended, and false refusals on reads are likely.
- (c) A PostToolUse after any tool that re-checks memory files changed since the last check. Needs state between calls.

**Recommendation: (a)**, and (c) if the weekly sheet ever shows a bypass.

## Can wait

### F6. Other memory folders

Claude Code also has subagent memory (`agent-memory/`, `agent-memory-local/`, each with its own `MEMORY.md`) and `<config home>/memory/personal/`. None exists on the development machine.

- (a) Auto memory only — *provisional default.*
- (b) Also those folders, with the same checks.
- (c) Any file named `MEMORY.md`, wherever it is.

**Recommendation: (a)** until one of them is in use; (c) would check unrelated files in repos.

### F7. Labels on a body-only edit

T1–T5 run on every write to a topic file, so fixing a typo in an unlabelled file earns a T2. In block mode, an unrelabelled project's next body edit is refused until its labels are added.

- (a) Checked on every write — *provisional default.* Matches the live rule "when you write or update an auto-memory file, add `kind:`": labels arrive as files are touched.
- (b) Only when the write touches the frontmatter.

**Recommendation: (a).** Three projects were not in tonight's relabelling, 18 of their files carry an error today [measured on copies]; (a) relabels them as they are used.

### F8. Telling the owner too

Warnings go to Claude only (`additionalContext`). `systemMessage` would also show the owner one line per warned write.

- (a) Claude only — *provisional default.*
- (b) Both, while warn mode lasts, so the owner sees what block mode would refuse.

**Recommendation: (a).** The hook's job is to get Claude to fix the file; `promptu memory` shows the owner the state whenever he looks.

### F9. A CLAUDE.md in the memory menu

The inventory lists a `CLAUDE.md` as one entry per `## ` section.

- (a) Per section — *provisional default.* The menu opens the file at the section.
- (b) Per top-level bullet. Finer, but well over a hundred entries for a large global file.

**Recommendation: (a).**

### F10. Memory id collisions

Six hex digits are 24 bits. Two topic files in one project share an id with probability about n² / 33.5 million: about 1 in 33,000 at 32 files.

- (a) Accept it — *provisional default.*
- (b) `memory check` warns on a collision, and the footer writer uses eight digits for the colliding pair.

**Recommendation: (a)** at today's folder sizes; (b) is cheap if a folder grows past a few hundred files.

## Step 2: the terminal pass (2026-10-29)

**Decided by the owner on 2026-10-29:** S1, S2 and S3 after the 20-prompt measurement, S4–S7 after the first review round, and S8 after the second; each is a numbered decision in `embarch-doc/embarch-promptu/decisions.md` (22–28, 31). **No step 2 fork is open.**

### S1. The pass's thinking — decided: effort `medium`

**Decided by the owner, 2026-10-29: (b).** Built as such: `pass_effort = "medium"` is the default (contract §7.6); it was `low`, provisional, until the pick.

Thinking cannot be turned off for the decided model through `claude -p`: `--model sonnet` is Claude Sonnet 5.5, and `--max-thinking-tokens 0`, `MAX_THINKING_TOKENS=0` and `alwaysThinkingEnabled: false` all still return a thinking block [measured 2026-10-29, Claude Code 2.1.287]. `--effort` is the only control. Measured on 20 of the owner's real task prompts plus 3 that match no pattern (local results, not in this repo): picks right 16/20 at low, 18/20 at medium, 16/20 at high; the evidence guard kept every fill at all three, and by hand medium's fills were all right while low had 3 wrong of 21; wall time p50 3.6–3.9 s at all three, p95 5.2–6.6 s low, 7.3 s medium, 8.0 s high; the same cost (about $0.02 a run with a new prompt).

- (a) `low`: the closest to the design's "thinking off"; fastest p95.
- **(b) `medium`**: best picks and fills at the same cost; about 1–2 s more at p95.
- (c) `high`, Claude Code's default: no gain.
- (d) Pin Sonnet 4.6, where thinking does turn off: slower (p50 8.5 s), dearer, and fewer right picks (8/11).

### S2. A reply to one PR comment — decided: `address-pr-comments`

**Decided by the owner, 2026-10-29.** A reply to a review comment on a pull request, however small, is `address-pr-comments`; `draft-message` keeps Slack, email and Linear messages. Built: both descriptions and both pick signals reworded so they no longer overlap, `draft-message`'s channels are Linear comment, Linear issue, Slack and email, `address-pr-comments` gains a fourth `which` option, "One comment", and two invented regression examples. The measurement's two misses that recurred at every effort were both of this kind.

### S3. A pasted, already-structured brief — decided: checked for its gaps

**Decided by the owner, 2026-10-29.** The pass runs the pattern of the work the brief describes and asks only the costliest questions it leaves open; the brief stays verbatim and only the answers to those questions are added. The brief's structure is evidence that pre-fills the questions it answers. Built as contract §7.8 and §8.3: "brief mode" asks only the required questions no fill answered and the always-asked ones; the pass flags a brief, and `prep` also detects one without the model; the footer carries `mode=brief`. `handoff`'s description now says it writes a brief and does not run one.

- Rejected: wrapping a brief in its pattern's full template (it rewrites what is already written), and sending it as is (its gaps go unasked).

### S4. A skipped house-rules question — decided: it renders nothing

**Decided by the owner, 2026-10-29: (b), for house rules.** Built: the house-rules fragment's `default` is `[]`, so when the pass judges `rules` not relevant it is skipped and adds nothing; asked or filled, it renders as before. `default` may now be a list for a `multi` question (contract §7.1). Decision 25.

When the pass judged `rules` not relevant, it was skipped and took the including pattern's recommendation: `implement-issue` then added "work in a separate worktree" and "only I open pull requests" without asking.

- (a) As designed: the default is the including pattern's own recommendation.
- **(b) Nothing**: a constraint nobody chose and the pass saw no reason for does not appear.

### S5. The desk work item in the terminal — decided: through the desk's MCP tool

**Decided by the owner, 2026-10-29: (a).** The `/promptu` skill fetches the work item with the desk's `desk_work_item` tool, when the session has the desk, and passes it as `prep --desk <file>`. **The CLI makes no network calls.** Decision 26.

- (a) The skill fetches it through the desk's MCP tool.
- (b) `promptu prep --item <key>` calls the desk's `GET /items/{id}/brief` itself.

### S6. Where last time's answers live — decided: the state file

**Decided by the owner, 2026-10-29: (a).** `<state>/last/<project>.json`, written by `render` (contract §7.5). It records the answers only, not the run: the footer stays the record of what was asked. Decision 27.

- (a) A state file of answers only.
- (b) Derive them from the transcripts' newest rendered prompt: a footer holds only `other`, not the text typed, and an edited draft or a deleted footer loses them.

### S7. A pasted colleague message with no "X asked:" — decided: an invented example

**Decided by the owner, 2026-10-29: (a).** Built: `colleague-relay` gained an invented example of a bare pasted question ("Hey, is the fan connector on these boards keyed, or can it go in backwards?") and its description names the case. The pass reads the examples' rough prompts. Decision 28.

- (a) Teach `colleague-relay` with an invented example.
- (b) Leave it to the chip row.

### S8. Order of the always-asked questions — decided: the bench state first

**Decided by the owner, 2026-10-29: (b).** A bench session asks the bench state first, board, what is plugged in, instruments and the green light, then the session's own questions. Built as a pattern field, `ask_first` (contract §7.1), set on `bench-session` only. Decision 31.

They used to be asked costliest gap first, then in pattern order, so `bench-session` asked "Instruments" before "Board" and "Plugged in", after the required "Pass means".

- (a) By gap, then pattern order.
- **(b) The bench state first**, as one bench check.
