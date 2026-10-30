# Promptu: design forks for the owner

Choices the approved design does not settle, met while building. **No agent decides one.** An open fork runs on a provisional default, the most reversible one, until the owner picks; the default is marked. Ranked by consequence within each step. Step 1 (memory): F1–F10. Step 2 (the terminal pass): S1–S8. Step 3 (the chat, desk Start work, the Memory menu): P1–P15, at the end.

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

## Step 3: the chat, desk Start work, the Memory menu (2026-10-30)

**Decided by the owner on 2026-10-30:** P1–P11, P13 and P14, each marked below and recorded as a numbered decision in `embarch-doc/embarch-promptu/decisions.md` (33 amended, 34–40). **P12 and P15 stay open** on their provisional defaults, and **P16–P17**, met building the Usage icon the owner added the same day (embarch-chat decision 99), are open. Built in `embarch-promptu` (decisions 32–33), `embarch-chat` (its decisions 94–99, 11 reversed) and `embarch-desk` (its decision 67). Ranked by consequence.

### P1. Start work with no Promptu prompt: send the desk's starter, or leave it in the draft — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such: with Promptu off or not installed, Start work's starter waits in the draft, unsent. P1 and P2 differ on purpose. Decision 34.

When the chat takes a Start work entry and Promptu is off on its card or not installed, there is no pass. Chat decision 11 sent the read-and-summarize starter; the owner reversed 11 to "the prompt lands in the draft, not sent" for the Promptu case.

- **(a)** The starter waits in the draft, unsent. One Enter sends it; nothing acts unasked. Changes what Start work does today for a chat with Promptu off.
- (b) Send it, as 11 did. Today's behaviour when Promptu is off.

**Recommendation: (a)**, the same "nothing is sent" rule whether or not Promptu runs.

### P2. Start work when the pass fails or is skipped — decided: (b)

**Decided by the owner, 2026-10-30: (b).** Built: with Promptu on, a pass that fails or is skipped sends the starter, as chat decision 11 did; it was (a), provisional, until the pick. Decision 34.

The pass ran and gave no prompt (a non-zero exit, a timeout, Esc while it read the issue).

- (a) The starter waits in the draft, with the one-line reason.
- **(b)** Send the starter, as 11 did.
- (c) Leave the draft empty.

**Recommendation: (a).**

### P3. What counts as a use in the Memory menu's "where it's used" — decided: (b)

**Decided by the owner, 2026-10-30: (b).** Built: only a prompt the owner sent counts; `promptu render`'s output in a tool result does not (it also closed review finding F3). Decision 33, amended.

Decision 33. Footers sit in the owner's sent messages (the chat's flow), but the terminal skill shows its prompt and never sends it as a message.

- (a) Sent messages, plus `promptu render`'s JSON in tool results, once per footer per transcript. A terminal run cancelled at "send as is" also counts.
- **(b)** Sent messages only: the terminal's runs never count.
- (c) Also any tool output line starting `— promptu `: a run may count twice.

**Recommendation: (a).**

### P4. A pattern forced on the card, mid-task — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 35.

The owner decided that a reply inside a running task never triggers the pass. The card's picker sets a pattern "for the next prompt", and the next prompt may be a reply.

- **(a)** A forced pattern lights the icon for the next prompt, a reply included, once. An explicit pick is not the automatic trigger.
- (b) Only at a task start: the pick waits for the next chat.

**Recommendation: (a)**: otherwise a pick made mid-task does nothing visible.

### P5. How the desk knows Promptu is installed (desk decision 67) — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 36, with embarch-desk decision 67.

- **(a)** The `promptu` binary, executable, in `~/.local/share/embarch/bin/` or on the desk service's `PATH`. Known gap: a binary without the plugin types a `/promptu` Claude Code does not know.
- (b) Also read Claude Code's installed-plugin list: another tool's file, no stable format.
- (c) The desk's `promptu` setting only, no detection.

**Recommendation: (a).**

### P6. How the chat knows the bench tools are there (`--have embarch-api`) — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 36.

A chat's first prompt goes before its `claude` starts, so its MCP list is not known yet.

- **(a)** The chat's CLI's MCP list once it has one (an `embarch-api` server not failed or disabled), else the `embarch-api` binary on `PATH`.
- (b) The binary on `PATH` only.
- (c) Read Claude Code's MCP configuration.
- (d) A chat setting.

**Recommendation: (a).**

### P7. Start work's chip row when the default replaced the pick (decision 32) — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 37.

When the model picks a `never` pattern or none, or the pass is offline, `--start` takes the `default` pattern and sets confidence to null, so the chip row opens first in the dialog, the default recommended.

- **(a)** The chip row opens; one Enter takes the default. The only way to switch pattern in the automatic Start work dialog.
- (b) No chip row: the default is taken silently.

**Recommendation: (a).**

### P8. What Esc keeps of the page on screen — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 38.

The design: "Esc keeps the partial answers". A terminal Esc drops the page on screen.

- **(a)** The pages already answered, plus the questions the owner touched on the page on screen. Untouched pre-selections become assumptions.
- (b) The pages already answered only, as the terminal does.
- (c) Everything on screen, pre-selections included (then Esc is "Done").

**Recommendation: (a).**

### P9. The handoff card's dialog and the "Fresh chat with this handoff" chip — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 39.

Chat decision 36's chip appears after a turn that answered a handoff request. The handoff pattern's own `save_to` may write the brief to a file.

- **(a)** The handoff pattern's prompt still marks the message as a handoff request, so the chip appears. With no fenced brief, the chip carries Claude's whole reply.
- (b) No chip after Promptu's handoff: `save_to` decides where the brief goes.

**Recommendation: (a)** until the weekly loop shows how the brief is used.

### P10. A key for the pass on any draft — decided: (a)

**Decided by the owner, 2026-10-30: (a).** No pass key: the indicator and its card instead. Decision 35.

SYNTHESIS §12 seeded "Ctrl+Shift+Enter runs the pass", to check against VS Code's own bindings. Not built: the pass runs at a task start or with a forced pattern.

- **(a)** No key.
- (b) Ctrl+Shift+Enter runs the pass on whatever the draft holds, a reply included.

**Recommendation: (a)** for now; (b) once the forced pattern shows it is wanted.

### P11. Where the card's switches live — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 35.

- **(a)** The extension's own state, for every window. Never the owner's settings, as chat decision 82 keeps effort out of them.
- (b) VS Code settings (`embarchChat.promptu.enabled`, `.quick`), syncable and visible in the settings UI.
- (c) Per chat.

**Recommendation: (a).**

### P12. One pass per task start

After a pass ran, failed or was skipped, the next Enter sends, even if the owner clears the draft and types a new first prompt.

- **(a) One pass per task start; a forced pattern runs another** — *provisional default.*
- (b) Light again whenever the draft holds no footer and differs from the last rough prompt.

**Recommendation: (a).**

### P13. Overriding a pre-filled answer in the dialog — decided: (b)

**Decided by the owner, 2026-10-30: (b).** Built now: a pre-filled answer is editable in place in the dialog; a change is an answer, `!` in the footer (contract §7.8 `filled`). It was (a), read-only, provisional. Decision 38.

SYNTHESIS §2.4 has a "Pre-filled (n)" strip whose values can be changed (recorded as `!` in the footer). Built read-only: the fold lists what the sources answered and the defaults.

- (a) Read-only.
- **(b)** Each pre-filled answer editable in place; a change becomes an answer, `!` in the footer.

**Recommendation: (b) later**, once the weekly loop reads overrides.

### P14. The rail's icons and their place — decided: (a)

**Decided by the owner, 2026-10-30: (a).** Built as such. Decision 40.

- **(a)** 🧠 Memory, ✨ Promptu, 📚, 💡, at the bottom of the rail beside the input.
- (b) At the top, as VS Code's activity bar has them.
- (c) Codicons instead of emoji.

**Recommendation: (a)**: the 💡 and Promptu work on the draft, so they sit by it.

### P15. Smaller choices made while building

- `--pattern` with `--start`: the pattern wins, even a `never` one.
- A transcript whose folder no longer exists (a removed worktree) counts only when its path is under the main checkout, so its uses are lost.
- `start_default` is true on every offline Start work run.
- A `memory.claude-md` fill counts for the global `CLAUDE.md` as a whole, never a section; records whose `origin.kind` is not `human` are skipped.
- The desk's Start work answer carries `starter` only beside `claude_uri`.
- Quick mode applies to the handoff card's dialog too.
- "Accept all recommendations" keeps what the page on screen holds, changed answers included.

### P16. Where the Usage icon sits in the rail

The owner asked for a Usage icon in the rail and did not say where. Built at the top of the rail's stack, above Memory, as it is about the account rather than the draft.

- **(a) Top of the stack: 📊 Usage, 🧠 Memory, ✨ Promptu, 📚, 💡** — *provisional default.*
- (b) Bottom, under the 💡.
- (c) Apart from the others, at the rail's top edge, as VS Code keeps its account icon away from the views.

**Recommendation: (a).**

### P17. A usage reading before any chat of the window has had a reply

The Usage icon reads the one source the chat has, the `rate_limit_event` a chat's CLI sends with a turn (what decision 67's banner read). It carries both windows and their resets, as `unifiedWindows` — an `@internal` field of Claude Code 2.1.287 [measured from its bundle]; without it the icon falls back to the one limit the event names. But nothing comes before a chat of this window has had a reply, so a fresh window shows "no reading yet".

- **(a) No reading until then** — *provisional default.* Nothing invented, nothing stored.
- (b) Keep the last reading in the extension's own state across windows and reloads, shown with its age.
- (c) Ask Claude Code for it directly, outside a chat (a separate call; no documented route found).

**Recommendation: (b)**: a reading an hour old, marked as such, beats none for an account-wide number; it stores no more than the decision-82 kind of state the card's switches use.

