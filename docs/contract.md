# Promptu: the contract

**Version 1, 2026-10-29.** The interfaces Promptu's five build steps share. Build step 1 (memory) owns §1–§5 and the memory id in §6.3: they are built and tested, and they are fixed. §6 is decided and written from step 2. §7–§10 are sketches: the step that builds each one fills it in and may add to it, but must not change anything §1–§6 fix (§11).

`[measured]` marks a fact checked against Claude Code 2.1.287 on 2026-10-29, either by running it or by reading its bundle; `[assumed]` marks one that was not. Every example is invented.

## 0. Rules for every interface

- Every JSON document carries `"promptu": 1`. Adding a field keeps 1. Renaming or removing a field, or changing what one means, bumps it. Readers ignore fields they do not know.
- Paths are absolute. A date is `YYYY-MM-DD` in local time; `PROMPTU_TODAY=YYYY-MM-DD` pins "today" for tests.
- A command that can find problems exits 1 when it finds an error, 0 otherwise, and 2 on a usage error (a path that does not exist).
- No model call and no network in anything step 1 ships.

## 1. Where memory lives

### 1.1 Terms

| Term | Meaning |
|---|---|
| **Memory folder** | A project's Claude Code auto-memory directory. |
| **Topic file** | A `.md` file directly inside a memory folder, other than `MEMORY.md`. One memory each. |
| **Index** | `MEMORY.md` in a memory folder: one line per topic file, grouped under `## <domain>` headers. |
| **Kind** | What a memory is: `preference` · `rule` · `trap` · `reference` · `status`. |
| **Domain** | A `## ` header of the index. The headers are the project's domain list. |
| **Memory id** | Six hex digits naming a topic file in a footer (§6.3). |
| **Main checkout** | The path a memory folder is keyed on (§1.2). |

### 1.2 Where Claude Code keeps a memory folder [measured: read from the 2.1.287 bundle]

1. **Config home:** `CLAUDE_CONFIG_DIR`, else `~/.claude`.
2. **Memory base:** `CLAUDE_CODE_REMOTE_MEMORY_DIR`, else the config home.
3. **Main checkout:** take the directory the session started in, walk up to the nearest directory holding a `.git` (file or folder), and if its `.git` is a file naming `<common>/worktrees/<name>` whose `commondir` leads back to the common `.git` and whose `gitdir` points back at this worktree, take the checkout that `.git` belongs to (for a bare repository, the bare folder). Anything else, a submodule included, is its own root. Outside git, the starting directory itself. So a linked worktree and every subfolder share the main checkout's memory.
4. **Folder name:** every UTF-16 unit of the main checkout's path that is not an ASCII letter or digit becomes `-` (a character outside the BMP becomes two). A name over 200 characters is cut to 200, then `-` and the base-36 absolute value of the path's Java-style hash (`h = (h << 5) - h + unit`, 32-bit) are appended. With `CLAUDE_CONFIG_DIR` set and the memory base equal to the config home, `CLAUDE_CODE_PROJECT_DIR_NAME` replaces the name when it matches `^[A-Za-z0-9_-]{1,64}$` and is not a reserved device name.
5. **Default folder:** `<memory base>/projects/<folder name>/memory/`.
6. **Overrides, first match wins:** the `CLAUDE_COWORK_MEMORY_PATH_OVERRIDE` environment variable (no `~` expansion); then an `autoMemoryDirectory` setting (`~/` expanded) in managed settings (`/etc/claude-code/managed-settings.json` [assumed path]), the project's `.claude/settings.local.json`, and the user's `<config home>/settings.json`. A checked-in `.claude/settings.json` is ignored, as Claude Code ignores it.

**What the hook treats as a memory write:** a `.md` file directly inside `<memory base>/projects/<any folder>/memory/`, or directly inside the folder §1.2 resolves for the hook's `cwd`. A file in a subfolder of a memory folder is not checked. Subagent memory (`agent-memory/`, `agent-memory-local/`) and `<config home>/memory/personal/` are not covered ([forks](forks.md) F6).

**Where a `source:` path resolves:** against the main checkout the folder's name encodes, found on disk by matching each directory's own encoded name against the folder name (the name alone is ambiguous: `/a/b-c` and `/a/b/c` encode alike). A cut (hashed) name, an overridden folder, or a name nothing on disk matches falls back to the main checkout of the current directory.

## 2. The labels

### 2.1 A topic file's frontmatter

The file opens with a `---` block. It is read the way Claude Code reads it [measured]: the block is everything up to the next `---`; it is YAML; when that fails, each top-level `key: value` whose value holds one of ``{}[]*&#!|>%@` `` or `": "` is double-quoted, leading tabs become two spaces each, and it is parsed once more. A block that fails both passes does not parse.

| Field | Required | Value |
|---|---|---|
| `name` | yes | Non-empty. |
| `description` | yes | Non-empty. |
| `kind` | yes | One of the five kinds. |
| `type` | Claude Code's own | When present, it matches the kind: preference → `user`, rule → `feedback`, trap → `feedback`, reference → `reference`, status → `project`. |
| `review-by` | on `status` | `YYYY-MM-DD`, after today and at most 90 days ahead. Only the frontmatter field counts, never a body line. |
| `source` | on `reference`; on `trap` when there is one | One value or a YAML list. Each is a URL (`scheme://…`), a commit (7–40 hex digits), an absolute path (`/…`, `~/…`, or a drive), or a repo-relative path. A path is one token, or holds a `/` if it holds a space, so a sentence is refused. A trailing `:<line>` or `#anchor` names a place in the file. |

**Every field is read at either level: the top level or under `metadata:`.** On a Write into a memory folder, Claude Code moves every top-level key except `name` and `description` under `metadata:`, and stamps `metadata.node_type`, `metadata.originSessionId` and `metadata.modified` [measured, on a create and on an overwrite]. So a file written with `kind:` at the top level is stored with `metadata.kind:`, and both shapes must pass every check. A key at both levels with different values is an error (T2 for `kind`).

### 2.2 The index

- A `## <domain>` header opens each domain. Headers inside a code fence do not count.
- An **index line** is a line starting `- [` (after indentation): `- [Title](file.md) — the fact itself`. Its link target is the text between `](` and the next `)`, a leading `./` dropped.
- An index line is at most **150 characters** (Unicode scalar values), sits under a `## ` header, and is the only line linking its file.

## 3. The hook

### 3.1 Registration

`plugin/hooks/hooks.json`, in the plugin `embarch-promptu`:

```json
{ "hooks": {
  "PreToolUse":  [ { "matcher": "Write|Edit|MultiEdit", "hooks": [ { "type": "command", "command": "promptu hook", "timeout": 5 } ] } ],
  "PostToolUse": [ { "matcher": "Write|Edit|MultiEdit", "hooks": [ { "type": "command", "command": "promptu hook", "timeout": 5 } ] } ]
} }
```

`promptu` is found on `PATH`, as `embarch-desk hook` is; the suite installs binaries to `~/.local/share/embarch/bin/`.

### 3.2 Input

Claude Code's hook JSON on stdin [measured]. The hook reads only these fields and ignores the rest:

| Field | Use |
|---|---|
| `hook_event_name` | `PreToolUse` or `PostToolUse`; anything else is ignored. |
| `tool_name` | `Write`, `Edit` or `MultiEdit`; anything else is ignored. |
| `tool_input.file_path` | The target; relative paths resolve against `cwd`. |
| `tool_input.content` | Write. |
| `tool_input.old_string`, `new_string`, `replace_all` | Edit. |
| `tool_input.edits[]` | MultiEdit: the same three fields per edit, applied in order. |
| `cwd` | For §1.2's overrides and relative paths. |

Input over 16 MiB is not read whole and does not parse, so the write goes ahead unchecked.

### 3.3 What a PreToolUse checks

1. The target must be a memory write (§1.2), or the hook stays silent.
2. **Before:** the file as it is on disk; absent if it does not exist. A target that exists but is not a regular, readable text file: silent.
3. **After:** Write's `content`; or Edit applied to Before (exactly one match of `old_string`, or every match with `replace_all`; an empty `old_string` creates a missing file); or MultiEdit's edits applied in order. An edit that would not apply is one Claude Code refuses itself: silent.
4. `MEMORY.md` gets I1–I5 on After against Before; a topic file gets T1–T7.
5. **Touched lines:** a line of After is untouched when an equal line of Before is left to match it, and touched otherwise. When the file is created, every line is touched.

### 3.4 The checks

Check ids are permanent: a check is never renumbered, and a new one takes a new id.

| Id | On | Fails when | Severity |
|---|---|---|---|
| T1 | topic | The frontmatter is missing, does not parse (§2.1), or lacks a non-empty `name` or `description`. | error |
| T2 | topic | `kind` is missing, not one of the five, or set to different values at the two levels. | error |
| T3 | topic | `type` or `metadata.type` is present and does not match the kind. | error |
| T4 | topic | A `status` has no `review-by`, or it is not a date, not after today, or more than 90 days ahead. | error |
| T5 | topic | A `reference` has no `source`; any `source` value is malformed. A `trap` with no `source` is a **warning**. | error / warning |
| T6 | topic | A touched line opens an update paragraph: `Update:`, `Update <date>:`, `Update (<date>):`, a bold `**Update…`, or `Updated <date>` (list, quote and heading markers ignored). | error |
| T7 | topic | A field in Before's frontmatter is missing from After's, the level ignored. `node_type`, `originSessionId` and `modified` are exempt: Claude Code stamps them on every write. | error |
| I1 | index | A touched index line is over 150 characters. | error |
| I2 | index | A touched index line has no `## ` header above it. | error |
| I3 | index | After has a `## ` header Before does not, **once Before has at least one**. A new index, or one with no headers yet, may add them freely (the owner's answer to [forks](forks.md) F4). | error |
| I4 | index | A touched index line links a file another index line also links. | error |
| I5 | index | A touched index line links a file not in the memory folder (URLs and `#` anchors skipped). | warning |
| O1 | index, PostToolUse | A topic file in the folder is linked from no index line. | warning |

T1–T5 run on every write to a topic file, whatever it touches: a write brings its file's labels up to date ([forks](forks.md) F7). An untouched legacy index line is never checked by the hook.

### 3.5 Output

| Mode | No findings | Warnings only | Any error |
|---|---|---|---|
| `off` | exit 0, nothing | exit 0, nothing | exit 0, nothing |
| `warn` (default) | exit 0, nothing | exit 0, context | exit 0, context |
| `block` | exit 0, nothing | exit 0, context | **exit 2**, reason on stderr |

**Context** is one line of JSON on stdout, and nothing else:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Promptu checked this write to the memory file flash-tool.md (warn mode): it goes ahead, but fix these with your next edit to it:\n- T2: add `kind:` at the top level of the frontmatter: preference · rule · trap · reference · status"}}
```

- `hookEventName` is the input's event (`PreToolUse`, or `PostToolUse` for O1); Claude Code rejects a mismatch [measured].
- **No `permissionDecision`, ever.** `allow` would skip the user's own permission prompt; leaving it out keeps Claude Code's permission flow exactly as it would be without the plugin.
- **No `systemMessage`**: that field is shown to the user, not to Claude [measured] ([forks](forks.md) F8).
- The text is a lead line, then one line per finding: `- <id>: <message>` or `- <id> (line <n>): <message>`, line numbers 1-based in After.

**Refusal** (block mode, any error): exit 2, stdout empty, stderr:

```
Promptu refused this write to the memory file flash-tool.md. Fix these and write it again:
- T2: add `kind:` at the top level of the frontmatter: preference · rule · trap · reference · status
Also worth fixing (not blocking):
- T5: a trap memory takes `source:` when there is one — a repo path, an absolute path, a URL or a commit
```

The write does not happen and Claude reads the text as the tool's error [measured: `PreToolUse:Write hook error: [promptu hook]: <stderr>`].

**PostToolUse** after a write to `MEMORY.md`: when O1 finds orphans, context with `hookEventName: "PostToolUse"` naming them; otherwise nothing. Exit 0 always. A PostToolUse on a topic file says nothing.

### 3.6 Failing open

Any of these exits 0 with nothing printed, so the write goes ahead as it would without the plugin: mode `off`; empty, oversized or malformed input; an event or tool not listed in §3.2; a target that is not a memory write; a Before that cannot be read; an edit that would not apply; a panic (caught, silenced). A missing `promptu` binary makes the shell exit 127, which Claude Code shows as a non-blocking hook error and lets the write through [assumed from the bundle's exit-code handling]. A run past the 5-second timeout is non-blocking too [assumed, same source].

### 3.7 Configuration

`~/.config/embarch-promptu/config.toml` (`$XDG_CONFIG_HOME` respected; `PROMPTU_CONFIG_DIR` replaces the folder):

```toml
memory_hook = "warn"   # "warn" · "block" · "off"
```

`PROMPTU_MEMORY_HOOK` in the environment wins over the file. A missing file, an unreadable one, or an unknown value means `warn`: a typo never turns blocking on. Other keys belong to later steps and are ignored here.

### 3.8 Speed [measured 2026-10-29, release build, WSL2]

Over 300 calls each against copies of real memory: **p50 0.5–0.6 ms, p95 under 0.9 ms** per call with the binary spawned directly, and **p50 0.9–1.0 ms, p95 under 1.4 ms** through `sh -c "promptu hook"` as Claude Code runs it. Spawning `/bin/true` the same way costs 0.3 ms. A debug build is about 1 ms direct.

### 3.9 What was confirmed, and what is still unknown

Confirmed by running real headless sessions (`claude -p --plugin-dir plugin/`, Haiku, 2026-10-29): a plugin hook command finds `promptu` on `PATH`; warn mode's `additionalContext` reaches the model verbatim and the write proceeds under the normal permission flow; block mode's exit 2 refuses the write and the model reads the stderr text; Claude Code's harness moves and stamps frontmatter keys (§2.1). Confirmed from the bundle: the input fields; the `hookSpecificOutput` shape (`permissionDecision` of `allow`·`deny`·`ask`·`defer`, `permissionDecisionReason`, `updatedInput`, `additionalContext`); `systemMessage` going to the user; PostToolUse accepting `additionalContext`.

Unknown, and not guessed:

1. **PostToolUse O1 end to end.** The schema accepts it; no session has run it.
2. **Whether Edit and MultiEdit get the same move-and-stamp as Write.** Only Write was run. The checks read both levels either way.
3. **Whether Claude Code loosens `old_string` matching** (quote styles, whitespace) before applying an Edit. If it does, the hook's own match fails and that edit goes unchecked (fail open), never wrongly refused.
4. **A subagent's write.** Hooks are assumed to fire for subagents' tool calls; not run.
5. **Writes that are not Write, Edit or MultiEdit**: a shell redirect, `sed -i`, `cp` or `mv` into a memory folder is not seen ([forks](forks.md) F5).
6. **`autoMemoryDirectory` precedence.** The bundle reads project and local settings only under a condition it does not name; `--settings` on the command line is invisible to a hook; the managed-settings path is assumed. With no override set, as on the development machine, none of this applies.
7. **Exit 127 and timeouts** behave as §3.6 says per the bundle, not per a run.
8. **Native Windows paths** are untested; Linux and WSL only.
9. **Path safety checks** Claude Code applies while resolving a worktree, and its NFC normalisation of paths, are not mirrored.

## 4. `promptu memory check [<path>...] [--json]`

Runs the T and I checks on memory files **as they stand**: there is no write, so every line counts as touched, and the two checks that compare with Before (T7, I3) are skipped. A path may be a file (`MEMORY.md` gets I1, I2, I4, I5; any other `.md` gets T1–T6) or a folder (its `.md` files, the index first, plus O1 for the folder). With no path, the current directory's memory folder (§1.2); if it does not exist yet, a note on stderr and exit 0.

Exit 1 when any error is found, 0 when there are none (warnings included), 2 when a path does not exist or cannot be read.

```json
{
  "promptu": 1,
  "today": "2026-10-29",
  "files": [
    { "path": "/home/u/.claude/projects/-home-u-work-app/memory/MEMORY.md", "role": "index",
      "findings": [ { "check": "I1", "severity": "error", "line": 1, "message": "this index line is 168 characters; the limit is 150 — keep the fact, cut the words" },
                    { "check": "O1", "severity": "warning", "message": "orphan.md is not linked from the index: delete it or give it an index line" } ] },
    { "path": "/home/u/.claude/projects/-home-u-work-app/memory/flash-tool.md", "role": "topic", "findings": [] }
  ],
  "errors": 1,
  "warnings": 1
}
```

Every checked file is listed, clean ones with `findings: []`. `line` is omitted when a finding has none. The text form lists only files with findings, one line per finding, then a count line.

## 5. `promptu memory [--json]`

The memory that loads in the current directory, for the memory menu (terminal now, the chat's rail from step 3). Read-only.

```json
{
  "promptu": 1,
  "cwd": "/home/u/work/app/src",
  "project_root": "/home/u/work/app",
  "today": "2026-10-29",
  "hook": "warn",
  "memory": {
    "path": "/home/u/.claude/projects/-home-u-work-app/memory",
    "chosen_by": "default",
    "exists": true,
    "index": "/home/u/.claude/projects/-home-u-work-app/memory/MEMORY.md",
    "source_root": "/home/u/work/app"
  },
  "scopes": [
    { "id": "user",    "label": "User",        "files": ["/home/u/.claude/CLAUDE.md"], "domains": ["Response style", "Working rules"] },
    { "id": "auto",    "label": "Auto memory", "files": ["…/memory/MEMORY.md", "…/memory/bench-board.md"], "domains": ["bench", "tooling"] },
    { "id": "project", "label": "Project",     "files": ["/home/u/work/app/CLAUDE.md"], "domains": ["Build", "Git"] }
  ],
  "entries": [
    { "scope": "auto", "domain": "bench", "kind": "status", "type": "project", "id": "09a770",
      "file": "…/memory/bench-board.md", "title": "Bench board",
      "line": "- [Bench board](bench-board.md) — the bench runs a dev kit; its link is the higher serial interface",
      "line_no": 3, "chars": 99, "modified": "2026-09-15", "age_days": 44,
      "review_by": "2026-10-01", "source": [], "flags": ["review_by_past"], "stale": true, "used": [] }
  ],
  "counts": { "entries": 39, "stale": 5, "by_scope": {"auto": 32, "project": 7}, "by_kind": {"rule": 6, "trap": 12}, "by_flag": {"review_by_past": 1} },
  "used_scanned": false
}
```

- **`project_root`** is the main checkout (§1.2). **`memory.chosen_by`** is `default`, `env:<VAR>`, `setting:<file>` or `none`. **`memory.source_root`** is where relative `source:` paths resolve (§1.2).
- **Scopes**, always in this order: `user` (the config home's `CLAUDE.md` and `rules/**/*.md`), `auto` (the memory folder), `project` (each ancestor's `CLAUDE.md`, `CLAUDE.local.md` and `.claude/CLAUDE.md`, outermost first, then the checkout's `.claude/rules/**/*.md`; `@` imports are not followed). A scope with nothing is still listed.
- **Entries**, in order: by scope; in `auto`, by domain in header order (no domain last), then kind in the order preference · rule · trap · reference · status (none last), then index line. A `CLAUDE.md` contributes one entry per `## ` section (plus one for text above the first) with `id: "claude-md"`, `domain` the section's heading, `kind` and `type` null, `line` the section's first line, and no flags ([forks](forks.md) F9).
- **`id`**: the memory id (§6.3) for a topic file or a dead link's target; `claude-md` for a `CLAUDE.md` section. **`line`** and **`line_no`** are the index line; `chars` its length. **`modified`**: the frontmatter's `modified` (either level) as a local date, else the file's mtime. **`age_days`** is never negative.
- **`flags`**, a fixed vocabulary that only grows; `stale` is true when any is set. Grouped as the design's three stale signs:

  | Design's sign | Flags |
  |---|---|
  | review-by past due | `review_by_past` (on or before today) |
  | source path missing | `source_missing`: a path `source` (not a URL, not a commit) that does not exist |
  | label missing | `bad_frontmatter`, `no_kind`, `no_review_by` (a status without one), `no_source` (a reference without one), `no_domain` (index line under no header), `unindexed` (no index line links it) |
  | index points nowhere | `dead_link`: an index line whose file is gone (the entry's `file` is where it would be) |

- **`used`** lists the footers (§6) of the last 30 days naming the entry, newest first. Step 1 does not scan transcripts: `used` is always `[]` and `used_scanned` is `false`. The step that fills it sets `used_scanned: true` and gives each item at least `{ "pattern": "<id>", "question": "<qid>", "date": "YYYY-MM-DD" }`.

The text form (`promptu memory`) prints the same data: a header, then per scope its files and domains, and for auto memory one line per entry: kind, id, title, age, flags.

## 6. The footer

Decided by the owner; written by `promptu render` from step 2, read by the memory menu (§5 `used`) and the weekly loop.

### 6.1 Grammar

The last line of a rendered prompt, after a blank line. `—` is U+2014.

```
footer   = "— promptu " ( "none" / run )
run      = pattern "@" version [ " asked=" ask *( "," ask ) ]
                               [ " filled=" fill *( "," fill ) ]
                               [ " skipped=" qid *( "," qid ) ]
                               [ " assume=" qid *( "," qid ) ]
                               [ " pick=owner" ]
pattern  = 1*64( %x61-7A / DIGIT / "-" )          ; a pattern's name
version  = 1*DIGIT                                ; its `version`
ask      = qid ":" ( "rec" / "all" / choice *( "+" choice ) )
choice   = "o" 1*DIGIT / "other"                  ; 1-based option, or free text
fill     = qid ":" source [ "!" ]                 ; "!": the owner changed the fill
source   = "prompt" / "desk" / "last" / "context" / "memory." mem
mem      = 6( DIGIT / %x61-66 ) / "claude-md"
qid      = 1*( %x61-7A / "_" )
```

Fields appear in this order; one with nothing is left out. Example:

```
— promptu bench-session@3 asked=board:other,attached:o1+o3,pass:rec,gate:rec filled=flash:memory.5e5077,unblocks:desk! skipped=timebox assume=heavy
```

### 6.2 Meaning

`asked`: questions put to the owner and what he took (`rec` the recommendation, `all` taken by "accept all", `oN` the Nth option, `other` free text). `filled`: questions answered without asking, by source. `skipped`: neither asked nor filled; the default was used. `assume`: left open by Esc, rendered as assumptions Claude must state. `pick=owner`: he switched pattern. `none`: Promptu ran and no pattern was used. A reader takes the last line of a prompt that starts with `— promptu `, ignores fields it does not know, and treats a deleted footer as no run.

### 6.3 Memory id

**The first six lowercase hex digits of the SHA-1 of the topic file's name**: the name with its `.md`, no directory, as UTF-8. `flash-tool-trap.md` → `5e5077`; `example.md` → `56b97e`. A fill from a `CLAUDE.md` line is `memory.claude-md`, attributed to the file, not the line. An id is unique only within one project's memory folder; two topic files in one folder share an id with probability about n² / 33.5 million (about 1 in 33,000 at 32 files) ([forks](forks.md) F10).

## 7. `promptu prep` (sketch; build step 2)

```
promptu prep [--session <id>] [--item <work item>] [--pattern <id>] [--rec]     # rough prompt on stdin
```

Gathers (no model), runs the one Sonnet pass, guards its fills, and plans the pages. Prints:

```json
{
  "promptu": 1,
  "run": "<opaque; render takes it back>",
  "pattern": { "id": "bench-session", "version": 3, "picked_by": "model", "confidence": 0.84, "alt": ["debug-failure"] },
  "offline": false,
  "flags": { "firmware": true, "hardware": true, "item": true },
  "filled": [ { "q": "flash", "value": "<the option's value>", "source": "memory", "entry": "5e5077", "evidence": "<quote found in the source>" } ],
  "pages": [ { "ids": ["board", "attached", "pass", "_rest"],
               "questions": [ { "question": "Which board is on the bench?", "header": "Board", "multiSelect": false,
                                "options": [ { "label": "Same as last time (Recommended)", "description": "…" }, { "label": "…", "description": "…" } ] } ] } ]
}
```

- `pages[].questions` is AskUserQuestion's own input shape: at most 4 questions a page, 2–4 options each, a header of at most 12 characters, the recommended option first with " (Recommended)". `ids` maps each question to its question id; `_rest` is the paging control.
- `picked_by`: `model`, `regex` (offline fallback), or `flag` (`--pattern`). `entry` is set only for a `memory` fill.
- Where a run's state is kept between `prep` and `render` is step 2's choice.

## 8. `promptu render` (sketch; build step 2)

Input on stdin:

```json
{ "promptu": 1, "run": "<from prep>",
  "answers": { "board": { "other": "the rev C unit" }, "attached": ["Probe", "Logic analyzer"], "pass": "Each listed check passes" },
  "rest": "ask" }
```

An answer is a label, a list of labels (multi), or `{ "other": "<text>" }`. `rest` is `ask` (every page was answered), `recommended` ("take the rest as recommended"), or `assume` (Esc or a dismissed page: the rest become stated assumptions). Prints `{ "promptu": 1, "prompt": "<markdown, footer included>", "footer": "— promptu …", "sections": ["Goal", "Context", "Done when", "Constraints"] }`; `--text` prints the prompt alone. Rendering is deterministic and model-free.

## 9. `promptu desk --json` (sketch; build step 4)

What the desk's `GET /promptu` serves, cached 60 s: `{ "promptu": 1, "open": 0 }` when nothing is open (the Desk section stays hidden), else `{ "promptu": 1, "open": 23, "week": "2026-W45", "groups": [ { "id": "g1", "title": "implement-issue", "open": 7 } ], "feel": null }`.

## 10. `sheet.json` (sketch; build step 4)

One week's ✓/✕ sheet under the local learning-data folder:

```json
{ "promptu": 1, "week": "2026-W45", "generated": "2026-11-04T08:03:00-06:00", "feel": null,
  "numbers": [ { "check": "friction_per_prompt", "value": 0.12, "baseline": 0.14, "target": "<= 0.10" } ],
  "groups": [ { "id": "g1", "kind": "pattern", "pattern": "implement-issue", "title": "implement-issue — 7: 4 questions, 2 template, 1 picker", "cut": false,
                "lines": [ { "id": "l1", "verdict": "approve", "note": "", "target": { "pattern": "implement-issue", "question": "done" },
                             "change": "add option", "text": "add option \"Builds + flash + boot log\"", "count": 2,
                             "events": [ { "kind": "friction", "gap": "G10", "cost_msgs": 38 } ],
                             "proposed": "proposed/implement-issue.md", "status": "open", "error": null } ] } ] }
```

`kind`: `pattern` · `memory` · `new-pattern` · `external` · `no-change`. `verdict`: `approve` (the default: a line stands unless cut) or `cut`. `status`: `open` · `applied` · `failed` (`error` says why, such as a denylist hit). A `memory` line's change is a whole topic file plus its index line, applied through the §3 checks. Event detail that quotes the owner's prompts stays local and never reaches a public repository.

## 11. What later steps may change

- **Fixed:** §1.2's resolution, §2's labels, the hook's input handling, output shapes, exit codes and fail-open rules (§3), the check ids, the flag vocabulary's existing names, the footer grammar (§6.1) and the memory id (§6.3).
- **May grow:** new checks with new ids, new flags, new fields in any JSON document, new footer fields placed after `pick` (readers ignore unknown fields).
- **Open until built:** §7–§10, within §0's rules.
