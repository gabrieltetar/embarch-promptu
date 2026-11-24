# Promptu: the contract

**Version 1, 2026-10-29.** The interfaces Promptu's five build steps share. Build step 1 (memory) owns §1–§5 and the memory id in §6.3; build step 2 (the terminal pass) owns §6's writer, §7 and §8; build step 3 (the chat and Start work) added `used` to §5, `prep --start` to §7.8 and `promptu patterns` (§7.9). All three are built and tested, and what §11 lists is fixed. **Build step 4 (the weekly pass, §9, §10 and §14)** is specified here before it is built (decision 3): its shapes are fixed from the day this contract says so, and the step that builds them may add to them but not change what §11 fixes. Its first-cut heuristics are provisional in [forks](forks.md) W1–W9.

`[measured]` marks a fact checked against Claude Code 2.1.287 on 2026-10-29, either by running it or by reading its bundle; `[assumed]` marks one that was not. Every example is invented.

## 0. Rules for every interface

- Every JSON document carries `"promptu": 1`. Adding a field keeps 1. Renaming or removing a field, or changing what one means, bumps it. Readers ignore fields they do not know.
- Paths are absolute. A date is `YYYY-MM-DD` in local time; `PROMPTU_TODAY=YYYY-MM-DD` pins "today" for tests.
- A command that can find problems exits 1 when it finds an error, 0 otherwise, and 2 on a usage error (a path that does not exist). Any command but `hook` exits 3 on an internal error, with one `promptu: internal error: <message> (at <file>:<line>:<column>)` line on stderr, never Rust's 101 (`hook` fails open instead, §3.6). A debug build honours `PROMPTU_DEBUG_PANIC` to reach that path in a test; a release build compiles it out.
- No model call and no network in anything step 1 ships. Step 2's one model call is `prep`'s pass (§7.3); `lint`, `next` and `render` are model-free. Step 4's one model call is `promptu week draft` (§14.2); everything else under `week` and `desk` is model-free.

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

- A `## <domain>` header opens each domain. Headers inside a code fence do not count. Once an index has a header, a tool write may neither add a new one nor remove them all (I3).
- An **index line** is a line starting `- [` (after indentation): `- [Title](file.md) — the fact itself`. Its link target is the text between `](` and the next `)`, a leading `./` dropped.
- An index line is at most **150 characters** (Unicode scalar values), sits under a `## ` header, and is the only line linking its file.

## 3. The hook

### 3.1 Registration

`plugin/hooks/hooks.json`, in the plugin `embarch-promptu`:

```json
{ "hooks": {
  "PreToolUse":  [ { "matcher": "Write|Edit|MultiEdit|Bash", "hooks": [ { "type": "command", "command": "promptu hook", "timeout": 5 } ] } ],
  "PostToolUse": [ { "matcher": "Write|Edit|MultiEdit", "hooks": [ { "type": "command", "command": "promptu hook", "timeout": 5 } ] } ]
} }
```

`promptu` is found on `PATH`, as `embarch-desk hook` is; the suite installs binaries to `~/.local/share/embarch/bin/`.

### 3.2 Input

Claude Code's hook JSON on stdin [measured]. The hook reads only these fields and ignores the rest:

| Field | Use |
|---|---|
| `hook_event_name` | `PreToolUse` or `PostToolUse`; anything else is ignored. |
| `tool_name` | `Write`, `Edit` or `MultiEdit`; `Bash` on PreToolUse only (§3.3a); anything else is ignored. |
| `tool_input.file_path` | The target; relative paths resolve against `cwd`. |
| `tool_input.content` | Write. |
| `tool_input.old_string`, `new_string`, `replace_all` | Edit. |
| `tool_input.command` | Bash (§3.3a). |
| `tool_input.edits[]` | MultiEdit: the same three fields per edit, applied in order. |
| `cwd` | For §1.2's overrides and relative paths. |

Input over 16 MiB is not read whole and does not parse, so the write goes ahead unchecked.

### 3.3 What a PreToolUse checks

1. The target must be a memory write (§1.2), or the hook stays silent.
2. **Before:** the file as it is on disk; absent if it does not exist. A target that exists but is not a regular, readable text file: silent.
3. **After:** Write's `content`; or Edit applied to Before (exactly one match of `old_string`, or every match with `replace_all`; an empty `old_string` creates a missing file); or MultiEdit's edits applied in order. An edit that would not apply is one Claude Code refuses itself: silent.
4. `MEMORY.md` gets I1–I5 on After against Before; a topic file gets T1–T7.
5. **Touched lines:** a line of After is untouched when an equal line of Before is left to match it, and touched otherwise. When the file is created, every line is touched.

### 3.3a A shell write to a memory index

The index checks need the file as a write leaves it, which a shell command does not state. So a **PreToolUse on Bash** only warns, and names Edit as the checked route. It speaks when the command (1) names a path ending `MEMORY.md` that §1.2 resolves to a memory folder (`~/`, `$HOME/` and relative paths are expanded; the resolution is `claude::memory_target`, the same as a Write's) and (2) looks like a write: a `>` redirect other than `2>&1` or `> /dev/null`, one of `tee mv cp sed perl truncate install dd ed`, or the text `write`, `open(` or `fs.`. The reply is `additionalContext` in every mode: **a Bash command is never refused**, block mode included. A command that only reads (`cat`, `grep`, `wc`, `head`) gets nothing, as does one naming a `MEMORY.md` outside a memory folder or a topic file. It is a heuristic: a script that builds the path at run time, or a command that never names the index, is not seen, and a read that quotes `>` or `write` costs one warning. PostToolUse on Bash is not registered.

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
| I3 | index | **Once Before has at least one `## ` header**: After has a header Before does not, or After has no header at all ("removes every domain header", no line number). A new index, or one with no headers yet, may add them freely (the owner's answer to [forks](forks.md) F4); emptying the list is refused so that cannot be reached in two writes. | error |
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
{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Promptu checked this write to the memory file lint-script.md (warn mode): it goes ahead, but fix these with your next edit to it:\n- T2: add `kind:` to the frontmatter: preference · rule · trap · reference · status"}}
```

- `hookEventName` is the input's event (`PreToolUse`, or `PostToolUse` for O1); Claude Code rejects a mismatch [measured].
- **No `permissionDecision`, ever.** `allow` would skip the user's own permission prompt; leaving it out keeps Claude Code's permission flow exactly as it would be without the plugin.
- **No `systemMessage`**: that field is shown to the user, not to Claude [measured] ([forks](forks.md) F8).
- The text is a lead line, then one line per finding: `- <id>: <message>` or `- <id> (line <n>): <message>`, line numbers 1-based in After.

**Refusal** (block mode, any error): exit 2, stdout empty, stderr:

```
Promptu refused this write to the memory file lint-script.md. Fix these and write it again:
- T2: add `kind:` to the frontmatter: preference · rule · trap · reference · status
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

`PROMPTU_MEMORY_HOOK` in the environment wins over the file. A missing file, an unreadable one, or an unknown value means `warn`: a typo never turns blocking on. The pass's keys are in §7.6.

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

Exit 1 when any error is found, 0 when there are none (warnings included), 2 when a path does not exist or cannot be read, 3 on an internal error (§0).

```json
{
  "promptu": 1,
  "today": "2026-10-29",
  "files": [
    { "path": "/home/u/.claude/projects/-home-u-work-app/memory/MEMORY.md", "role": "index",
      "findings": [ { "check": "I1", "severity": "error", "line": 1, "message": "this index line is 168 characters; the limit is 150 — keep the fact, cut the words" },
                    { "check": "O1", "severity": "warning", "message": "orphan.md is not linked from the index: delete it or give it an index line" } ] },
    { "path": "/home/u/.claude/projects/-home-u-work-app/memory/lint-script.md", "role": "topic", "findings": [] }
  ],
  "errors": 1,
  "warnings": 1
}
```

Every checked file is listed, clean ones with `findings: []`. `line` is omitted when a finding has none. The text form lists only files with findings, one line per finding, then a count line.

## 5. `promptu memory [--json] [--no-used]`

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
    { "id": "auto",    "label": "Auto memory", "files": ["…/memory/MEMORY.md", "…/memory/soak-test.md"], "domains": ["testing", "tooling"] },
    { "id": "project", "label": "Project",     "files": ["/home/u/work/app/CLAUDE.md"], "domains": ["Build", "Git"] }
  ],
  "entries": [
    { "scope": "auto", "domain": "testing", "kind": "status", "type": "project", "id": "08841b",
      "file": "…/memory/soak-test.md", "title": "Soak test",
      "line": "- [Soak test](soak-test.md) — the nightly soak test is paused until the climate chamber is back from calibration",
      "line_no": 3, "chars": 112, "modified": "2026-09-15", "age_days": 44,
      "review_by": "2026-10-01", "source": [], "flags": ["review_by_past"], "stale": true, "used": [] }
  ],
  "counts": { "entries": 39, "stale": 5, "by_scope": {"auto": 32, "project": 7}, "by_kind": {"rule": 6, "trap": 12}, "by_flag": {"review_by_past": 1} },
  "used_scanned": true,
  "used_claude_md": [ { "pattern": "bench-session", "question": "gate", "date": "2026-10-20", "changed": false } ]
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
  | index points nowhere | `dead_link`: an index line whose target does not exist in the folder (the entry's `file` is where it would be). A URL or `#anchor` target is not listed at all; a target that exists but is not a topic file, such as one in a subfolder, is listed with no `dead_link`. |

- **`used`** lists the footers (§6) of the last 30 days that filled a question from the entry, newest first, one item per fill: `{ "pattern": "bench-session", "question": "flash", "date": "2026-10-20", "changed": false }`, `changed` being the footer's `!` (the owner changed the fill). Only auto entries carry uses; a `memory.claude-md` fill goes to the top-level **`used_claude_md`**, in the same shape, because it names the global `CLAUDE.md` as a file and not one of its sections (§6.3). **`used_scanned`** is `true` when the transcripts were read; `--no-used` skips them, leaving every `used` and `used_claude_md` empty and `used_scanned` `false`.
- **Where a footer is found.** Claude Code's transcripts, `<config home>/projects/*/*.jsonl` (top-level files only: a subagent's own transcript is in a subfolder), each last modified within 30 days. A record counts when its `cwd`'s main checkout (§1.2) is this inventory's `project_root` (a `cwd` that no longer exists counts when it is the main checkout or under it, so a removed worktree's runs are lost), and it is not `isSidechain`. **Only a prompt the owner sent counts** (the owner's pick, [forks](forks.md) P3): a `type: "user"` record that is not `isMeta` and has no `origin.kind` other than `human` — the owner's own message, as the chat sends a rendered prompt — with a line starting `— promptu ` in its content (a string, or its `text` blocks). `promptu render`'s output in a tool result (the terminal skill shows its prompt, it does not send it) and Claude's own text never count. One footer line counts once per transcript. Its date is the record's `timestamp` as a local date, else the file's; one older than 30 days is left out. A footer that does not parse (§6.1) is skipped. Reading is a substring search for `— promptu ` and a JSON parse of only the lines that hold it: **0.21–0.24 s for 349 MB in 60 files** with the page cache warm [measured 2026-10-29, release build, WSL2, a synthetic set; a cold cache not measured]; `--no-used` under 10 ms.

The text form (`promptu memory`) prints the same data: a header, then per scope its files and domains, and for auto memory one line per entry: kind, id, title, age, flags, and `used N×` when it has uses; the User scope's line adds `CLAUDE.md used N×` for `used_claude_md`.

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
                               [ " mode=brief" ] [ " way=" way ] [ " rewritten=yes" ]
pattern  = 1*64( %x61-7A / DIGIT / "-" )          ; a pattern's name
version  = 1*DIGIT                                ; its `version`
ask      = qid ":" ( "rec" / "all" / choice *( "+" choice ) )
choice   = "o" 1*DIGIT / "other"                  ; 1-based option, or free text
fill     = qid ":" source [ "!" ]                 ; "!": the owner changed the fill
source   = "prompt" / "desk" / "last" / "context" / "memory." mem
mem      = 6( DIGIT / %x61-66 ) / "claude-md"
qid      = 1*( %x61-7A / "_" )
way      = "workers" / "handoff" / "compactions" / "decimation"
```

Fields appear in this order; one with nothing is left out. Example:

```
— promptu bench-session@3 asked=board:other,attached:o1+o3,pass:rec,gate:rec filled=flash:memory.de8cea,unblocks:desk! skipped=timebox assume=heavy
```

### 6.2 Meaning

`asked`: questions put to the owner and what he took (`rec` the recommendation, `all` taken by "accept all", `oN` the Nth option, `other` free text). For a `text` question, `rec` is the first option shown (its prompt pre-selection or the pass's first candidate), `oN` counts its static options, then its fallback options (`leave-out`/`not-sure`: the question's own fallback is o(K+1), the other o(K+2), K static options), and another candidate is `other`. `meaning` is the generated ambiguity question (§7.8), `oN` the Nth meaning. `filled`: questions answered without asking, by source. `skipped`: neither asked nor filled; the default was used. `assume`: left open by Esc, rendered as assumptions Claude must state. `pick=owner`: he switched pattern. `mode=brief`, after `pick`: the prompt was a pasted brief, checked for its gaps only (decision 24); `skipped` is then absent, as a brief takes no defaults. `way=<name>`, after `mode`: the pass judged the work would outgrow one context window and named that way in Who does it (§7.10). `rewritten=yes`, last: the prompt above the footer is the rewrite's text, not his words under Goal (decision 42, §8.4); its absence means the structured render, or a brief, left his words as typed. `none`: Promptu ran and no pattern was used. A reader takes the last line of a prompt that starts with `— promptu `, ignores fields it does not know, and treats a deleted footer as no run.

### 6.3 Memory id

**The first six lowercase hex digits of the SHA-1 of the topic file's name**: the name with its `.md`, no directory, as UTF-8. `cache-trap.md` → `de8cea`; `example.md` → `56b97e`. A fill from a `CLAUDE.md` line is `memory.claude-md`, attributed to the file, not the line. An id is unique only within one project's memory folder; two topic files in one folder share an id with probability about n² / 33.5 million (about 1 in 33,000 at 32 files) ([forks](forks.md) F10).

## 7. Prompt patterns and `promptu prep` (build step 2)

### 7.1 Pattern files

- **Where.** `patterns/*.md` in this repo, embedded in the binary at build time (`build.rs`). `PROMPTU_PATTERNS=<folder>` replaces the whole set, for development and lint. One public, generic set; anything specific to a user arrives at run time through memory, the desk and last time's answers.
- **Shape.** A `---` frontmatter block (YAML), then the body: a template (§7.2). `<name>.md` is a task pattern (`kind: task`), `_<name>.md` a fragment (`kind: fragment`) that patterns include and that is never picked.

| Field | Value |
|---|---|
| `name` | `[a-z0-9-]{1,64}`, the file's name (a fragment's without its `_`). The footer's pattern. |
| `description` | 1–1,024 characters: what it is for, and what it is not for. The pass reads it to pick. |
| `version` | A whole number from 1, bumped by every approved change. The footer's version. |
| `kind` | `task` or `fragment`. |
| `signals.phrases`, `signals.not` | Case-insensitive regexes: +1 per phrase hit, −1 per `not` hit: the no-model pre-pick (§7.3's `regex_hint` and the offline fallback). |
| `signals.desk` | `default` · `possible` · `never`: its role under desk Start work, `prep --start` (§7.8): the default pick, one the issue may call for, or never picked. |
| `include` | `[{fragment, recommended?, when?}]`: the fragment's questions join this pattern's, with this pattern's recommendation and condition. |
| `ask_first` | Question ids (its own or its fragments') asked before all others, in this order, when they are asked at all. `bench-session` asks its bench state first: board, what is plugged in, instruments, the gate (decision 31). |
| `questions` | The quiz, below. |
| `examples` | `[{rough, context, answers, note}]`: invented runs. The pass reads their `rough` prompts; lint renders each and the tests pin every render (`tests/golden/`). |

Per question:

| Field | Value |
|---|---|
| `id` | `[a-z][a-z_]*`, unique within the pattern with its fragments. Stable across versions: a change of meaning takes a new id. |
| `header` | 1–12 characters: AskUserQuestion's header. |
| `ask` | The question. |
| `type` | `choice` · `multi` · `text` · `bool`. |
| `options` | `[{label, description, value?}]`: 2–4 for `choice` and `multi`, 0–3 static ones for `text`, 0 or 2 for `bool`. `value` is the instruction rendered into the prompt (the label when absent) and may hold context tokens such as `{{branch}}`. |
| `recommended` | A label (`choice`, `bool`), a list of labels (`multi`, possibly empty). Required on `choice` and `multi`. |
| `default` | A label or a value, or for `multi` a list of labels (`[]`: nothing): used when the question is neither asked nor filled. Absent: the recommendation; for `text`, the fallback. The house-rules fragment's is `[]` (decision 25). |
| `suggest` | 0–3, `text` only: how many candidate answers the pass may propose. |
| `fallback` | `leave-out` · `not-sure`, `text` only: the option added when fewer than two exist. `not-sure` renders "not known yet; ask me before relying on it". |
| `required` | Asked whenever no source fills it. |
| `always_ask` | Asked every run. Only the prompt may pre-select its answer; a fill from any other source becomes a suggestion (a `text` question's candidate) or is dropped. Never `memory` (lint). |
| `sources` | A subset of `prompt`, `desk`, `memory`, `last`, `context`: where a fill may come from. |
| `when` | `flag`, `!flag` (`firmware` · `hardware` · `item`), `qid`, `qid == 'L'`, `qid != 'L'`, `qid in ['A','B']`, `qid has 'L'`; it may test only an earlier question or a flag. False: the question does not apply, is not asked, and renders nothing. |
| `gap` | `G1`…`G16` or `none`: the friction gap it closes. Unanswered questions are asked costliest gap first: G10, G5, G1, G8, G13, G9, G14, G15, G6, G4, G2, G11, G12, G7, G16, G3, then `none`. |
| `why` | One line, generic: the reason the question exists. |
| `requires` | `desk` · `embarch-api` · `item`, on a question or on one option: it shows only when the run has that EmbArch part (§7.6), and for `item` only when the desk is present **and** the run has a work item from it (`--desk`); without it, it is hidden and the generic rest remains (decision 29). An option that reads or writes the work item takes `item`, not `desk`. |

### 7.2 Templates

- **Logic-less Mustache**: `{{x}}` and `{{{x}}}` (the same: the output is Markdown and nothing is escaped), sections `{{#x}}…{{/x}}`, inverted sections `{{^x}}…{{/x}}`, list loops with `{{.}}`, dotted names, partials `{{> <fragment>}}`. A line holding only a section tag leaves no line behind. No helpers and no `$`.
- **Context**: `ask` (the rough prompt, verbatim), `item.key|title|url|id` (when there is a work item; `item` alone is truthy then), `repo`, `branch`, `worktree` (set only in a linked worktree: its path), `today`, `config.<key>` (every string key of `config.toml`, §7.6), the flags `firmware` and `hardware`, and each question id with its value: an option's `value`, the answer's text, or a list for `multi`.
- **The six sections**, in order: `## Goal` · `## Context` · `## Read first` · `## Done when` · `## Constraints` · `## Who does it`. The post-pass drops a heading whose section is empty, collapses runs of blank lines, closes a blank line between two list items, and trims.

### 7.3 The pass

One stripped `claude -p` per `prep`, run in the empty folder `<state>/pass/`:

```
claude -p --model <pass_model> --effort <pass_effort> --no-session-persistence --output-format json \
  --strict-mcp-config --setting-sources "" --tools "" --system-prompt <system>      # message on stdin
```

with `EMBARCH_DESK_INTERNAL=1`, `PROMPTU_INTERNAL=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` in its environment (the last drops a side call Claude Code makes on every `-p`, $0.0033 a run, and leaves caching alone [measured 2026-10-29, 2.1.287]). `PROMPTU_CLAUDE` replaces the `claude` binary (tests use a fake). **Thinking cannot be turned off** on Claude Sonnet 5.5 through 2.1.287: `--max-thinking-tokens 0` still returns a thinking block [measured]. `--effort` is the only control, so it is the setting (decision 22).

- **System prompt**: `src/pass_system.md`, a blank line, then the catalogue generated from the set as it shows with the run's EmbArch parts (hidden questions and options are left out, so the model never sees a hidden label; one cached prompt per combination of parts): per task pattern its `## <name>`, its description, `Typical phrases:`, `Example prompts:`, and one line per question (`- `<id>` (<type>; required; always_ask; suggest N; when …; sources …): <ask> Options: <label> (<description>) | …`). It changes only with the patterns, so the prompt cache holds it.
- **Message**: six tagged blocks, each `(none)` when empty (a seventh, `<context_usage>`, is present only when the run was given the chat's context usage, §7.10): `<prompt>` (verbatim), `<context>` (`repo:`, `branch:`, `linked worktree:`, `firmware:` lines), `<desk>`, `<memory>` (the project's `MEMORY.md`: `## ` headers, and each index line prefixed `[<memory id>] `; then `## standing rules` and the global `CLAUDE.md` bullets prefixed `[claude-md] `, from the sections `memory_claude_md_sections` names, every section by default; 12,000 characters at most, the index first), `<last>` (`<pattern>.<qid>: <answer>` for each question that takes `last`), and `<regex_hint>` (`<pattern> <score>, …`).
- **Reply**: one JSON object, `pattern` first:

  ```json
  {"pattern": "bench-session", "confidence": 0.84, "alt": ["debug-failure"],
   "flags": {"hardware": true, "firmware": true, "brief": false},
   "filled": {"flash": {"value": "EmbArch", "source": "memory", "entry": "de8cea", "evidence": "flash through EmbArch only"}},
   "candidates": {"board": ["Stepper driver rev C, unit 2"]},
   "relevant": {"timebox": false, "unblocks": true},
   "ambiguous": [{"phrase": "the rail", "meanings": ["the 5 V rail", "the 12 V rail"]}],
   "way": null}
  ```

  `way` is `null` for a small task, else `{"name": "workers", "how": "…", "why": "…"}` (§7.10).

- **Falls back to the keyword pick, nothing filled, `offline: true`**, with the reason, when the binary cannot run, the run passes `pass_timeout_s` (15 s; the child is killed), the reply is an error or holds no JSON object, or `--offline` is given.

### 7.4 The guard (no model)

A fill stands only when all of these hold; otherwise it is listed in `dropped` with its reason and the question is asked:

1. The question belongs to the picked pattern (fragments included).
2. The question shows with the run's EmbArch parts, and its `source` is in the question's `sources`; an `always_ask` question takes only `prompt` (a `text` answer from another allowed source becomes a candidate instead).
3. The evidence is at least three words or twelve characters, and, case-, quote-, dash- and whitespace-folded, is found in the source's text on word boundaries (a quote that starts or ends with a letter or digit may not start or end inside a word). A `memory` fill is checked against its `entry`'s line; with no `entry`, the one memory line holding the quote names it.
4. The value fits the type: a label (`choice`), labels (`multi`), non-empty text (`text`), a bool or label (`bool`), counting only the options that show with the run's parts. A `text` value equal to a hidden option's label (any case) is refused, and a fill is only ever matched to an option that shows. A memory entry is six lowercase hex digits or `claude-md`.

Also: a pattern outside the set means no pick; `way` is kept only with a pick, a `name` of the four in §7.10 and a non-empty `how` and `why` (whitespace folded to one line), else dropped; `alt` keeps at most two known patterns; candidates go to `text` questions only, at most their `suggest`; `relevant` counts only optional questions; an `ambiguous` phrase must occur in the prompt and carry 2–3 meanings.

### 7.5 State

Under `PROMPTU_STATE_DIR`, else `$XDG_STATE_HOME/embarch-promptu`, else `~/.local/state/embarch-promptu`:

| Path | What |
|---|---|
| `runs/<run>.json` | One run between `prep` and `render`: the rough prompt, the pick, the guarded fills, candidates, relevance, the answers so far. Deleted 7 days after its last write (each `prep` prunes). |
| `last/<project>.json` | `{"promptu": 1, "patterns": {"<pattern>": {"date", "version", "answers": {"<qid>": {"answer": {…}, "text": "<as it reads>"}}}}}`: each pattern's latest answers in one project, written by `render`, read as the `last` source. `<project>` is the main checkout's memory-folder name (§1.2). |
| `drafts/<run>.md` | The rendered prompt, for "edit first". Pruned with the runs. |
| `log/<run>.json` | One entry per rendered run (decision 42): the rough prompt as typed, what landed in the draft, and what the rewrite cost; shape in §8.4. **Never pruned**: it is the record the chat's card restores from and `promptu stats` measures over. Rendering the same run again overwrites its entry, the original unchanged. |
| `pass/` | The empty folder the model runs in. |
| `week/` | The weekly pass's folder: one folder per ISO week, and the packed and timer records (§14.1). Never pruned. |

A run id is `YYYYMMDD-HHMMSS-<4 hex>`. **The footer is the record of what a run asked** (§6); the run file is scratch and `last/` holds no more than the answers themselves ([forks](forks.md) S6). The log (decision 42) is the second record: the original and the rewrite's cost, not the questions.

### 7.6 Configuration

In `config.toml` (§3.7):

```toml
pass_effort = "medium"     # low · medium · high (decision 22); anything else: medium
pass_model = "sonnet"
pass_timeout_s = 15        # 1–600
voice_skill = "my-voice"   # any string key reaches templates as {{config.<key>}}
memory_claude_md_sections = ["Working rules"]   # which global CLAUDE.md sections join the memory source; absent: all
rewrite = "on"             # on (default) · off: no rewrite after the questions (§8.4)
rewrite_model = "sonnet"   # the rewrite's model: a setting, the owner's to settle (decision 42); any `claude --model` value
rewrite_effort = "low"     # low · medium · high; anything else: low
rewrite_timeout_s = 30     # 1–600

# The weekly pass's keys (week_schedule, week_model, week_effort, week_timeout_s, week_checkout, week_public_check) are in §14.2.

[embarch]                  # the optional EmbArch integration (decision 29)
desk = "auto"              # auto · on · off
embarch-api = "auto"
```

**EmbArch parts.** Promptu works alone. A run's parts are each part forced `on` or `off` here, or, with `auto` (the default, and any other value), present when `prep --have` names it. `item` is not configured or detected: `prep` adds it when the run has `desk` and `--desk` gave it a work item. **A hidden question** (its `requires` is missing) is not asked, not filled and not rendered, and is absent from the footer; a `when` that tests it is false, whatever its operator (`!=` and `!` included). **A hidden option** is not shown, cannot be filled or taken, and is not a recommendation: when a choice's recommended option is hidden, its first option that shows is recommended instead, and a skipped question takes it; a multi keeps only what shows. Footer codes still count every option in the file (`oN`).

### 7.7 `promptu lint [<path>...] [--json] [--show] [--have <parts|none>]`

Checks pattern files without a model. No path: the set this binary carries (or `PROMPTU_PATTERNS`); a folder: that set; files: those files, against their folder's set. `--show` prints every example as rendered; `--have` renders them with those EmbArch parts only (`desk`, `embarch-api`; default: both), an answer to something hidden left out. `item` is never taken from `--have`: as in `prep`, an example has it when the desk is among the parts and its context has a work item. Exit 1 when any error is found, 2 when a path does not exist, 0 otherwise.

| Check | Error when |
|---|---|
| `schema` | The frontmatter is missing or is not the schema (§7.1). |
| `name`, `description`, `version`, `kind` | The name's form, its file name (`_` for a fragment), 1–1,024 characters, a version from 1, `task` or `fragment`. |
| `signals` | A regex does not compile; `desk` is not one of the three. |
| `include`, `ask_first` | The fragment does not exist, or a recommended label is not one of its labels; an `ask_first` id that is not a question of the pattern. |
| `id`, `header`, `ask`, `type` | An id's form or a duplicate; a header of 0 or more than 12 characters; no ask; an unknown type. |
| `options`, `label` | AskUserQuestion's 2–4 options (per type, §7.1); an empty, duplicate or over-60-character label, or one that writes "(Recommended)" itself. A label over 30 characters is a warning. |
| `recommended`, `default`, `suggest`, `fallback` | Not a real label; missing on `choice` or `multi`; `suggest` over 3 or off `text`; `fallback` missing on `text` or present elsewhere. |
| `sources`, `volatile` | Empty, or not among the five; an `always_ask` question that takes `memory`. |
| `when` | Unreadable; testing something that is neither an earlier question nor a flag; a literal that is not a label of the question it tests. |
| `gap`, `why` | Not `G1`–`G16` or `none`; no why. |
| `slot`, `partial` | A tag that is not a question id, a context token or a flag; a partial that is not a fragment. In an option's `value`, which is rendered with the context only: a tag naming anything but a context token or a flag, a partial, or a value that does not render. |
| `example`, `footer` | An answer to an unknown question; a render that fails, leaves a `{{`, or loses the rough prompt; a footer outside §6.1. |
| `requires` | Not `desk`, `embarch-api` or `item`; a `choice` or `multi` question left with fewer than 2 (or more than 4) options when every EmbArch part (and `item`) is missing. |
| `size` (warning) | More than 12 questions: speed, not a cap. |
| `signals` (warning) | An example's keyword pre-pick puts another pattern above its own. |

### 7.8 `promptu prep [--have <parts|none>] [--pattern <id>|none] [--rec] [--desk <file>] [--offline] [--start] [--context-usage <percent>]`

The rough prompt on stdin. Gathers (no model), runs the pass (§7.3), guards it (§7.4), plans the questions, saves the run, prints:

```json
{
  "promptu": 1,
  "run": "20261029-215400-a1b2",
  "pattern": { "id": "bench-session", "version": 1, "picked_by": "model", "confidence": 0.84, "alt": ["debug-failure"] },
  "offline": false, "offline_reason": null,
  "mode": "normal",
  "parts": ["desk", "embarch-api"],
  "flags": { "firmware": true, "hardware": true, "item": false },
  "filled": [ { "q": "flash", "header": "Flash via", "value": "EmbArch", "source": "memory", "entry": "de8cea", "evidence": "flash through EmbArch only" } ],
  "dropped": [ { "q": "pass", "source": "prompt", "reason": "the evidence is not in the source" } ],
  "defaults": [ { "q": "heavy", "header": "How heavy", "value": "Overlay or config first" } ],
  "choose": null,
  "pages": [ { "ids": ["board", "attached", "instruments", "_rest"],
               "questions": [ { "question": "Which board is on the bench, with its revision and any reworks?", "header": "Board", "multiSelect": false,
                                "options": [ { "label": "Stepper driver board (Recommended)", "description": "From your prompt." }, { "label": "Not sure", "description": "The prompt says it is not known yet." } ] } ] },
             { "ids": ["gate", "pass", "unblocks"], "questions": [ … ] } ],
  "done": false,
  "pass": { "ms": 2262, "cost_usd": 0.0157, "effort": "medium" }
}
```

- **`pattern`**: null for no pattern. `picked_by` is `model`, `regex` (offline), `flag` (`--pattern`) or `owner` (switched on the chip row, §8.1). `start_default` (build step 3): true when desk Start work's default replaced the pick (`--start`, below).
- **`--start`** (build step 3): the run is a desk Start work, the chat's or the skill's `--item` (`signals.desk`, §7.1). With no `--pattern`, a pick that is missing, `none`, unknown or a pattern whose `signals.desk` is `never` becomes the set's `signals.desk: default` pattern (the first in name order; `implement-issue` in the shipped set), its fills, candidates and relevance cleared as a forced pick's are, and its confidence null, so the chip row opens with that pattern recommended; offline, the default is taken instead of the keyword top (`picked_by` stays `regex`). Either way `pattern.start_default` is true. A `default` or `possible` pick stands as the model made it. Under `--start`, `alt` and the chip row never hold a `never` pattern; `--pattern` still wins, whatever its `signals.desk`. The output's top-level **`start`** is true for such a run, false otherwise.
- **`--have`**: the EmbArch parts the session has, comma-separated (`desk`, `embarch-api`); `none` or absent: none. With the config they make `parts` (§7.6), plus `item` when the desk is there and `--desk` gave a work item; the output lists them and the run keeps them for `next` and `render`.
- **`--desk <file>`**: the work item as JSON (`key`, `title`, `url`, `id`, `text`) or plain text. It is the `desk` source, sets `item` and the `item.*` tokens.
- **`mode`**: `brief` when the prompt is a pasted, already-structured brief: the pass says so, or, without it, the prompt has at least 150 words and two structure markers (headings, label lines, the usual brief sections, a numbered list of three). Decision 24.
- **`warnings`** (`prep` only): one line for each thing skipped so the run could go on. An unreadable `--desk` file leaves the desk source out. **A run that cannot be saved** (the state folder cannot be written) can have no `next` or `render`, so nothing is asked: every question takes its recommendation, and the output adds `prompt` and `footer`, as `render` would print them, with `pages` empty and `done` true.
- **`pass`** (`prep` only): the call's wall time in ms, its list-price cost as Claude Code reports it, and the effort used; nulls when offline.
- **`filled`**: each fill that stood (§7.4), its question's `header`, `value`, `source`, `entry` and `evidence`. **A fill the dialog can change in place** (build step 3, the owner's [forks](forks.md) P13; every fill but an always-asked question's, which its page asks) also carries `ask`, its question in a page's shape (`question`, `header`, `multiSelect`, `options` as shown, the recommendation marked); `picked`, the labels of `ask.options` it holds now; `other`, any of it that is free text (else null); `changed`, true once an answer differs from the fill; and `original`, `{picked, other}`, what the fill itself holds in the same labels, so an answer equal to it undoes the change (no `!`); free text counts as the same whether the pass gave it or it comes back typed as `other`. Changing it is an answer to its id in `next` or `render` (§8.1), and the footer marks it `!`.
- **`defaults`**: what the prompt will assume without asking: each question that is neither asked nor filled, and the default it takes.
- **`choose`**: the chip row, one AskUserQuestion page with id `_pattern`: when the pick is weak (confidence under 0.6), there is none, or the run is offline. Its options are the pick (recommended) and its alternatives, then the keyword ranking, at most three, and "No pattern, send as is" (recommended when nothing was picked). `pages` is empty until it is answered (§8.1). `--pattern` never opens it, and neither does a run with no pattern to offer (the pass found none and no keyword hit): an AskUserQuestion page needs two options, so that run goes on with no pattern and `done` is true.
- **What is asked**, in order: the pattern's `ask_first` ids that are asked at all, in their order; then required questions no fill answered, costliest gap first; then every `always_ask` question that applies (its prompt fill, if any, pre-selected); then optional questions the pass marked relevant, by gap. **A brief asks only the first two groups**, and its other questions are neither asked nor rendered. Offline, optional questions are not asked. `--rec`: nothing is asked; every question takes its recommendation (`all`).
- **`pages`**: each is one AskUserQuestion call, in its own input shape. At most 4 questions. When an optional question would be left after a page, the page holds 3 plus the `_rest` control (header `Rest`: "Keep going" recommended, or "Take the rest as recommended"); otherwise it holds up to 4. **The Rest control skips optional questions only**: they take their recommendation (`all`), while required and always-asked questions are still asked on the pages after it. Its description lists what the optional ones take and names those still asked. An ambiguous phrase adds `_meaning` (header `Meaning`) first. A question whose `when` tests an answer still to be given waits for a later page; pages after the first are a forecast, re-planned by `next`.
- **Options shown**: the recommended one(s) first, marked " (Recommended)"; for a `text` question, its prompt pre-selection, then the pass's candidates, then its static options, then a fallback while fewer than two; at most 4; labels cut to 80 characters. Claude Code adds "Other" itself.
- **Exit**: 0 with JSON (offline and every warning included); 2 with a message on stderr for no prompt or an unknown `--pattern`; 3 on an internal error (§0). `next` exits 1 when it cannot save the run.

### 7.10 Planning for high context: `--context-usage` and `way` (decision 41)

Before rendering, the pass judges whether the work will outgrow one context window. No code sizes the task: the pass reads the prompt, the desk item, the memory and, when given, the context usage.

- **Input: `prep --context-usage <percent>`**, a whole number 0 to 100: how full the chat's context window already is. **This is the one input `embarch-chat`'s `beginPromptu` passes.** It reaches the pass as a `<context_usage>` block (`62%`) and is saved in the run as `context_usage`. Without it (Start work, the terminal, any caller that does not know) the block is absent and the pass judges from the text alone; a value over 100 exits 2. It is a hint for the judgment, not a threshold in code: the pass's instructions start from `embarch-chat` decision 36's (compact above about 60 %, hand off above about 80 % or for unrelated work, background workers for a long self-contained job).
- **Output: `way`** in the pass's reply (§7.3), kept by the guard (§7.4) and printed by `prep` and `next` as the run's `way` (`null` or `{name, how, why}`). `name` is one of `workers` (parallel background workers, the `delegate-agents` shape), `handoff` (stop at a named boundary and hand off with `embarch-chat`'s `handoff` tool), `compactions` (compact at named points, each with a focus) or `decimation` (split into tasks of one session each, in order).
- **Rendered** as one line added to `## Who does it`, after what the pattern put there (a pasted brief takes it there too, in its added sections): `- Context plan, <way>: <how> Why: <why>`. A small task, with `way` null, gets nothing. The Goal stays verbatim (decision 9).
- **Footer**: `way=<name>`, in the extra fields after `pick` and `mode` (§6.1), only when a way is picked: `— promptu implement-issue@1 asked=… way=handoff`. A reader that does not know it ignores it.

### 7.9 `promptu patterns [--json] [--have <parts|none>]` (build step 3)

The task patterns the set carries (§7.1: the binary's own, or `PROMPTU_PATTERNS`), in name order; fragments are never listed. What the chat's Promptu card offers when the owner forces a pattern for the next prompt. Read-only and model-free; `--have` is accepted as `prep` takes it, and hides nothing (a pattern has no `requires`).

```json
{ "promptu": 1,
  "patterns": [ { "name": "implement-issue", "version": 1, "summary": "Build a feature or a change from an issue or an idea: a new driver, a setting, a fix with…",
                  "description": "<the whole description, whitespace folded>", "desk": "default" } ] }
```

`summary` is the description's first sentence, cut at 120 characters with `…`; `desk` is the pattern's `signals.desk` or null. The text form prints one line per pattern, `<name>@<version>  <summary>`. Exit 0; 2 when the set cannot be loaded (a `PROMPTU_PATTERNS` folder that does not exist).

## 8. `promptu next` and `promptu render` (build step 2)

### 8.1 Input

Both read the same JSON on stdin (empty means `{}`). The input is optional, so neither waits on a caller that sends nothing: a terminal counts as empty, and any other stdin has 2 seconds [chosen] to send its first byte or close, after which the command says so on stderr and goes on with `{}`. Input that has started is read to end of file.

```json
{ "pattern": "debug-failure",
  "answers": { "board": "Same as last time (Recommended)", "attached": "Debug probe, Bench supply", "pass": { "other": "it boots" }, "_rest": "Keep going (Recommended)" },
  "rest": "ask" }
```

- **`pattern`** (or `answers._pattern`): the chip row's answer, a pattern name or "No pattern, send as is" (" (Recommended)" ignored). A switch away from the pass's pick sets `picked_by: owner`. A fill is kept only where the new pattern has a question with the same id that is not asked every run, takes that source, and takes that value (the same labels, §7.4 rule 4); any other fill moves to `dropped`, so its question is asked. Candidates stay for the new pattern's text questions only.
- **`answers`**, by id: a label as returned (" (Recommended)" ignored, case ignored), a list of labels, a comma-joined string of labels (a multi-select), or `{"other": "<text>"}`. Text that matches no label is free text ("Other"). `_rest: "Take the rest as recommended"` gives the optional questions left their recommendation (`all`); required and always-asked ones stay to be asked. (`rest: recommended`, from `--rec` or `render --rest recommended`, takes every question's recommendation.) `_meaning` takes a meaning or free text. An answer to a question that was filled overrides the fill (`!` in the footer). Answers accumulate across calls.
- **`rest`**: `ask` (default), `recommended` (the questions left take their recommendation, `all`), or `assume` (Esc: each question left becomes a stated assumption). `render --rest <r>` sets it from the command line.
- **Exit 2** for an unknown run, stdin that is not JSON, an answer to a question the run does not have, or an unknown `rest`.

### 8.2 `promptu next <run>`

Records the input, re-plans, and prints the same JSON as `prep` (§7.8) for the run as it now stands: `pages` is what is left to ask, `done` is true when nothing is.

### 8.3 `promptu render <run> [--rest recommended|assume] [--text]`

Records the input, then writes the prompt. The structured render (below) is deterministic and model-free; **from decision 42 `render` then makes one model call, the rewrite (§8.4)**, so it takes seconds, not milliseconds, unless the run is a brief, has no pattern, is offline or has `rewrite = "off"`. **Pages left unanswered with no `rest` count as Esc.** Prints `{ "promptu": 1, "run", "pattern", "prompt", "footer", "sections", "rewritten", "draft" }`, or with `--text` the prompt alone; writes `drafts/<run>.md`, `log/<run>.json` and this project's `last/` entry. **None of those writes can fail it**: a draft that cannot be written leaves `draft` out, and an unsaved run, log or last answers change nothing in the output. Rendering again is allowed.

- **A pattern**: its template with the context and the answers (§7.2). An answered `_meaning` adds `By "<phrase>" I mean <meaning>.` to Context. Assumptions add one line to Constraints: `- Not answered; state your assumption before you start: <ask> <ask> …`. A section the prompt lacks is created in its place among the six.
- **A brief** (decision 24): the rough prompt verbatim, then only what it left open, under the six headings: each answer as a bullet (the option's value when it is a sentence, else `<header>: <value>`), plus the meaning and the assumptions. A fill the brief itself gave adds nothing.
- **No pattern**: the rough prompt verbatim, a blank line, `— promptu none`.
- **The value of an answer**: an option's `value` with its context tokens rendered; a candidate's or free text as given; a `not-sure` fallback as "not known yet; ask me before relying on it"; a `leave-out` fallback as nothing.
- **The footer**: §6, fields in pattern order; `rewritten=yes` last when §8.4 rewrote.

### 8.4 The rewrite and the log (decision 42)

After the structured render, `render` rewrites the rough prompt once, unless:

| Skipped when | `rewrite_skipped` in the log |
|---|---|
| the run is a pasted brief (decision 24 stands: his text verbatim, only gaps added) | `brief` |
| no pattern was used | `no-pattern` |
| the run is offline (no model) | `offline` |
| `rewrite = "off"` | `off` |
| the call fails, times out (`rewrite_timeout_s`) or comes back empty: **the structured render stands**, his words verbatim under Goal, and `render` still exits 0 | `failed: <reason>` |

- **The call**: a stripped `claude -p` like the pass (§7.3) with `rewrite_model`, `rewrite_effort` and `src/rewrite_system.md`, in the empty `pass/` folder. The message is three blocks: `<rough>` (as typed), `<answers>` (`question: answer`, one per line, what `last/` records) and `<plan>` (the structured render without its footer, so the six sections and the assumption and context-plan lines). The system prompt keeps every fact and answer, adds none, keeps the six headings, writes in his voice. A whole-reply code fence and any `— promptu ` line the model adds are stripped.
- **What lands**: the rewrite, a blank line, the footer with `rewritten=yes`. The draft file holds the same. `render`'s JSON gains `"rewritten": true|false`.
- **The log entry**, `log/<run>.json`:

```json
{ "promptu": 1, "run": "20261117-142600-ab12", "date": "2026-11-17T14:26:10-06:00",
  "cwd": "/work/repo", "project_key": "-work-repo", "pattern": "bench-session", "version": 3,
  "mode": "prompt",
  "original": "<the rough prompt exactly as typed>",
  "rendered": "<the prompt as it landed in the draft, footer included>",
  "rewritten": true, "rewrite_skipped": null,
  "rewrite_model": "sonnet", "rewrite_effort": "low",
  "rewrite_ms": 4200, "wait_ms": 4300 }
```

`mode` is `brief` or `prompt`. `rewrite_model` and `rewrite_effort` are set whenever a call was tried (also when it failed); `rewrite_ms` only when it succeeded. **`wait_ms` is the owner's wait after the last question**: the `render` command from its start to the prompt ready. **`rendered` is what the weekly loop's edit signal compares the sent prompt against** (decision 14, amended by 42): the rewrite when there is one, the structured render otherwise. A run that was never rendered has no entry.

### 8.5 `promptu original <run> [--json]`

**The read-back verb**, called by `embarch-chat`'s Promptu card to roll the draft back to what the owner typed (EMB-197). Reads `log/<run>.json`.

- **stdout**: the entry's `original`, exactly as typed, no added newline. With `--json`, the whole entry (§8.4) as pretty JSON.
- **Exit 0** printed. **Exit 2**, nothing on stdout and one line on stderr: no log entry for that run (never rendered, or the id is not a run id).
- Model-free and fast; it reads one file. The run id is the one `prep` and `render` print.

### 8.6 `promptu stats [--model <name>] [--json]`

p50 and p95 (nearest rank) of `wait_ms` and of `rewrite_ms` over the logged runs that were rewritten, overall and per `rewrite_model`; `--model` keeps one model's runs. JSON: `{ "promptu": 1, "runs", "rewritten", "wait_ms": { "n", "p50", "p95", "max" }, "rewrite_ms": { … }, "models": { "<model>": { "wait_ms", "rewrite_ms" } } }`; a percentile is `null` with no runs. Exit 0. The owner's measurement of the model and the wait, over at least ten real prompts, is this command's output: it is **owed, owner-side** (embarch-doc `tasks/promptu/007`).

**Speed** [measured 2026-10-29, release build, WSL2]: `render` p50 3.1 ms, p95 3.7 ms; `prep --offline` (everything but the model) p50 7.2 ms. The hook, re-measured with this build's larger binary: p50 0.66 ms, p95 0.81 ms spawned directly, as in §3.8.

## 9. `promptu desk --json` (build step 4)

What the desk's `GET /promptu` serves, which the desk caches for 60 s. Read-only, no model, no network; it reads the newest sheet (§14.1) and writes nothing. **`embarch-desk` owns the endpoint and the Desk section; this command is all they need to ask.**

- Nothing open (no sheet, or every line settled): `{ "promptu": 1, "open": 0 }`. The Desk section stays hidden (decision 14).
- Else: `{ "promptu": 1, "open": 23, "week": "2026-W48", "generated": "2026-11-25T08:03:00-06:00", "groups": [ { "id": "g1", "title": "implement-issue", "open": 7 } ], "feel": null }`.

`open` counts the lines whose `status` is `open` (§10), `groups[].open` the same per group; a group with none is left out. A superseded sheet (decision 46) is never read. Exit 0, also with no sheet; 3 on an internal error. The text form is one line, `promptu: 23 open (2026-W48)` or nothing.

**What the other scopes need, and no change is made to them here.** `embarch-desk`: run `promptu desk --json` for `GET /promptu`, and `promptu week show --json`, `week mark` and `week apply` (§14.4) for the section's list, its ✓/✕ and its apply button; Promptu not installed means no section (`embarch-desk` decision 67's detection). `embarch-chat`'s Promptu card (decision 14, 2026-11-07): `promptu week status --json` for next run and missed run (decision 45) and `promptu desk --json` for the open count. `embarch-ui` has no call of its own: what it shows comes through the desk's endpoint.

## 10. `sheet.json` (build step 4)

One week's ✓/✕ sheet, `week/<week>/sheet.json` (§14.1), written by `promptu week draft` and changed afterwards only by `week mark` and `week apply`:

```json
{ "promptu": 1, "week": "2026-W48", "generated": "2026-11-25T08:03:00-06:00", "feel": null,
  "superseded_by": null, "supersedes": "2026-W47",
  "numbers": [ { "check": "fills_kept", "value": 0.93, "baseline": 0.90, "target": ">= 0.90" } ],
  "groups": [ { "id": "g1", "kind": "pattern", "pattern": "implement-issue", "title": "implement-issue — 7: 4 questions, 2 template, 1 picker", "cut": false,
                "lines": [ { "id": "l1", "kind": "pattern", "verdict": "approve", "note": "",
                             "target": { "kind": "question", "pattern": "implement-issue", "question": "done" },
                             "change": "add option", "text": "add option \"Builds + flash + boot log\"", "count": 2,
                             "events": [ "e14", "e22" ], "carried_from": null,
                             "proposed": "proposed/implement-issue.md",
                             "check": { "lint": "ok", "examples": "ok", "replay": [ { "run": "20251118-091500-3fa1", "questions_before": 5, "questions_after": 5 } ] },
                             "status": "open", "error": null, "commit": null } ] } ] }
```

- **Every pack event is in some line's `events`** (§14.5), the sheet being the account of the week: an event the drafter proposed no change for is a `no-change` line with its reason in `text`. A line's `events` are event ids of the pack; the quotes stay in the pack.
- **`kind`** (line and group): `pattern` · `memory` · `new-pattern` · `external` · `no-change`. `external` is a change outside Promptu's files (a global `CLAUDE.md` rule, another repo's doc): it is shown, never applied, and is `applied` only when the owner marks it so with `week mark`.
- **`target.kind`** names what a `pattern` or `new-pattern` line changes (decision 14): `question` (an existing one's options, wording, default or `when`), `new_question`, `template` (a section's text), `picker` (`signals`, `description`, `desk`, the default pick) or, for `new-pattern`, `pattern` (a whole new file). A `memory` line's target is `{ "kind": "memory", "project": "<§1.2 folder name>", "file": "<topic file>" }`; a specific, stable fact is a memory line and never goes into a pattern, so patterns stay generic (decision 14).
- **`verdict`**: `approve` (the default: a line stands unless cut) or `cut`. A group's `cut: true` cuts every line in it. **`status`**: `open` · `applied` · `failed` · `cut`; `week mark ... cut` sets `cut`, `week apply` sets `applied` or `failed` (`error` says why: a lint failure, a public-safety hit, a failed memory check), and an `applied` pattern line's `commit` is the SHA (`null` with no checkout, §14.2). A `failed` line stays on the sheet, is retried by the next `apply`, and carries into the next sheet like an open one.
- **`check`** is the first cut's test of a proposed pattern change (decision 47): `lint` and `examples` are `promptu lint --show` on the proposed pattern set; `replay` re-runs up to five of the week's real runs of that pattern, taken from the pack, through `prep --offline` with the proposed set, and records the questions before and after ([forks](forks.md) W7). `null` on a `memory`, `external` or `no-change` line.
- **`numbers`** are decision 15's checks as the week measured them: `check` is `friction_per_prompt`, `redirects_per_task`, `fills_kept`, `patterns_kept`, `questions_per_run`; `value`, `baseline` (the previous sheet's value, `null` on the first) and `target` (the owner's, text); a number the pack cannot compute is left out, never zero. **`feel`** is `better`, `same`, `worse` or `null`, set by `week mark --feel`.
- **A `memory` line's change** is a whole topic file plus its index line, shown in `proposed/<file>`; `apply` writes it to the memory folder through the §3 checks and a check error makes the line `failed` ([forks](forks.md) W4). Event detail that quotes the owner's prompts stays local and never reaches a public repository: the sheet, the pack and `proposed/` are under the state folder only (§14.1), and a pattern line's text is generic by the rule above, so a proposed pattern reaches the checkout only after the public-safety command (§14.2).

## 11. What later steps may change

- **Fixed:** §1.2's resolution, §2's labels, the hook's input handling, output shapes, exit codes and fail-open rules (§3), the check ids, the flag vocabulary's existing names, the footer grammar (§6.1) and the memory id (§6.3). From step 2: the pattern file's fields and their meaning (§7.1), the template's tokens and six sections (§7.2), the shapes of `prep`'s, `next`'s and `render`'s JSON (§7.8, §8), the answer forms `next` and `render` accept (§8.1), and the state paths (§7.5).
- **May grow:** new checks with new ids, new flags, new fields in any JSON document, new footer fields placed after `pick` (readers ignore unknown fields), new lint checks, new config keys, new sources.
- **From step 3, fixed:** `used` and `used_claude_md`'s item shape and what counts as a use (§5), `promptu patterns`' JSON (§7.9), and `prep --start` with its output fields `start` and `pattern.start_default` (§7.8).
- **From decision 42, fixed:** `promptu original`'s output and exit codes (§8.5), the log entry's fields (§8.4), the footer's `rewritten=yes`. `stats`' output may grow.
- **From step 4, fixed:** the `week` commands' names, arguments, exit codes and the shape of their JSON (§14.4), `promptu desk --json` (§9), `sheet.json`'s fields, `kind`s, `verdict`s and `status`es (§10), the week folder's layout and each file's name (§14.1), the config keys' names and meaning (§14.2), the pack's record kinds and their fields (§14.5), and the timer units' names (§14.4). The first cut's heuristics, marked provisional in [forks](forks.md) W1–W9, are not fixed: they may change behind these shapes.
- **Open until built:** nothing in §9–§10 and §14; step 4 may add to them within §0's rules.

## 12. `promptu calls <session> [--subagent <id>] [--class <name>] [--json]`

Read-only. `<session>` is a Claude Code session's `.jsonl`, or its id (looked up in `<config home>/projects/*/`). It reads that file (the main thread) and `<session>/subagents/*.jsonl`, each with the `description` of its `*.meta.json`. A `tool_use` is joined to its `tool_result` by id; a result's size is the characters of its text (a call with no result counts 0).

Per thread (`main`, then each subagent by file name; `--subagent` keeps the ids that start with the given one and drops `main`): calls, result characters, and the same by class. Classes, first match wins, defined once in `src/calls.rs` (`classify`):

| Class | Matches |
|---|---|
| `git` | Bash running `git log`, `git diff` or `git show` (global options allowed) |
| `build` | Bash whose first word is `cargo`, `west`, `cmake`, `make`, `ninja`, `pytest`, `npm`, `gradle` or `tsc` |
| `config` | any call whose path or command names `Kconfig*`, `*.dts`, `*.dtsi`, `*.overlay`, `*.conf`, `*.defconfig`, `west.yml`, `CMakeLists.txt` |
| `source` | Read, Grep, Glob; Bash whose first word is `cat`, `grep`, `rg`, `sed`, `head`, `tail`, `ls`, `find`, `wc`, `awk`, `tree`, `nl` |
| `other` | the rest |

`--class <git|build|config|source|other>` lists that class's calls, one per line (`<chars>  <tool>  <command or path>`). `--json` prints an array of threads: `id`, `description`, `total`, `by_class` (`calls`, `result_chars`), and with `--class` a `calls` list. Exit 0; 2 for an unknown session or subagent. May grow: more classes, fields.

## 13. `promptu prior <prompt-file> [--projects <dir>] [--json]`

Read-only: lists what the earlier runs of a prompt asked and were answered, so a rerun of an interview prompt does not ask again. `<prompt-file>` holds the prompt's text, or only its Promptu footer line. `--projects` is a Claude Code transcript folder (`projects/<encoded cwd>/`); the default is this project's folder under the config home (the same folder name `memory` uses). It reads the top-level `*.jsonl` of that one folder (never a subagent's) and writes nothing.

**Match.** A transcript belongs to the prompt when its first user turn (the owner's own, not meta, not a sidechain) is the prompt: the text equal after `\r\n` folding and trimming; or, when the file is only a footer line, a first turn whose last non-empty line is that footer. The footer carries no run id, so the text is the match. A **near-match** (the first turn ends in the same footer under other text: the prompt edited) is not listed; the text view says how many on stderr and `--json` carries `near_matches`.

**Output.** Sessions oldest first by the first turn's `timestamp`. Per session, each `AskUserQuestion` call in order: its questions (`question`, `header`, `options` as `label` and `description`, `multi_select`) and `answer`, the text of the `tool_result` with that call's `tool_use_id` (`null` when the session ended first). No other tool's output is read into it. `--json`: `{"sessions": [{"session", "started", "calls": [{"questions", "answer"}]}], "near_matches": n}`. Exit 0, also with no match; 2 for an unreadable prompt file or folder.

## 14. `promptu week`: the weekly pass (build step 4)

The weekly pass reads a week's runs and sessions, drafts one ✓/✕ sheet of proposed changes (decision 14), and applies what the owner approves (decisions 30, 43). **Step 4's first cut** (decision 47) tests a proposed pattern change by re-rendering its examples and replaying real past prompts, and counts a commit as the only outcome. Everything but `week draft` is model-free; **`draft` is the pass's only model call** (one stripped `claude -p`, §14.2), so `pack`, `status`, `show`, `mark`, `apply`, `timer` and `desk` run offline.

Every command takes `--json` (default is text), prints `"promptu": 1` in it, and exits per §0: 0 done; 1 a drafter run failed or an `apply` left a line `failed`; 2 a usage error (an unknown week or line id, no open sheet where one is needed, a bad `--week` or schedule); 3 an internal error. `--week` is an ISO week `YYYY-Www`, local time, Monday first; it defaults to the week of `PROMPTU_TODAY`/today.

### 14.1 State: the week folder

Beside §7.5's paths, under the same state folder:

| Path | What |
|---|---|
| `week/<week>/pack.json` | The week's events (§14.5), written by `pack`. Quotes the owner's prompts: **local, never in a repository.** Rewritten by a second `pack` for the same week. |
| `week/<week>/run.json` | The run record: `{ "promptu": 1, "week", "kind": "scheduled"\|"manual", "scheduled_for": "<instant>"\|null, "started", "finished", "status": "running"\|"ok"\|"failed", "error": null, "model", "events": 41, "lines": 12 }`. `finished` is `null` and `status` is `running` while the run is in progress. `status` is `failed` when the drafter times out, exits non-zero or returns nothing that parses; the sheet is then not touched. |
| `week/<week>/sheet.json` | The sheet (§10). The record of what the owner decided. |
| `week/<week>/sheet.md` | A read-only Markdown view of the sheet, one `- [x]` line per proposal with its id, for a terminal reader (decision 30); rewritten on every `draft`, `mark` and `apply`. Editing it changes nothing ([forks](forks.md) W5). |
| `week/<week>/proposed/<name>.md` | A proposed pattern file (whole), or a memory line's topic file. |
| `week/<week>/patterns.patch` | With no checkout configured, the approved pattern lines as one patch (decision 43). |
| `week/packed.json` | `{ "promptu": 1, "through": "2026-11-23" }`: the last date a successful `pack` covered. `prep`'s prune keeps a run's draft until `through` passes the run's date, and at most 35 days ([forks](forks.md) W8): a draft is the terminal run's edit baseline (decision 44). |
| `week/timer.json` | `{ "promptu": 1, "installed": "<instant>", "schedule": "wed 08:00" }`, written by `week timer`; `missed` counts only scheduled instants after `installed` (§14.4). |

**The open sheet** is the newest week folder whose `sheet.json` has `superseded_by: null`. A new `draft` sets `superseded_by` on the earlier open sheet and `supersedes` on its own, and re-proposes the earlier sheet's `open` and `failed` lines among its own, with `carried_from: "<week>:<line id>"` and their old events added to the pack's (decision 46); a `cut` line is not carried. A second `draft` for the same week replaces that week's sheet and keeps each line's `verdict` and `note` where the target and change are the same.

### 14.2 Configuration

In `config.toml` (§3.7), beside §7.6's keys; `PROMPTU_WEEK_SCHEDULE` in the environment wins over `week_schedule`, as `PROMPTU_MEMORY_HOOK` does over its key. A bad value falls back to its default and `week status` says so in `warnings`.

```toml
week_schedule = "wed 08:00"   # <mon|tue|wed|thu|fri|sat|sun> <HH:MM>, local time (decision 45)
week_model = "opus"           # the drafter's model: any `claude --model` value (decision 14)
week_effort = "medium"        # low · medium · high; anything else: medium
week_timeout_s = 900          # 60–3600 ([forks](forks.md) W6)
week_checkout = "/path/to/embarch-promptu"   # the checkout a pattern change is committed in (decision 43); absent: a patch
week_public_check = "scripts/check-client-names.py --repo {checkout}"   # run in week_checkout, first; absent: none
```

- **Where an approved pattern change lands** (decision 43). `week_checkout` is a git checkout of the patterns' repo; `apply` creates the branch `promptu/week-<week>` from its current `HEAD` (or checks it out, when the week was applied before), writes each approved `pattern` or `new-pattern` line to `patterns/`, bumps the pattern's `version` by one, runs `promptu lint` on the folder, re-renders the examples, runs `week_public_check` (its non-zero exit fails the line, and the text it prints is the line's `error`), then makes one commit per line (`promptu: <title> (<week> <line id>)`). **Nothing is pushed**, `main` is never touched, and a dirty checkout stops `apply` before it writes (exit 1, nothing changed). With no `week_checkout`, `apply` writes `patterns.patch` and the line's `commit` stays `null`. `{checkout}` in the command is replaced; the command runs without a shell.
- **The drafter** is `claude -p` with no tools, no plugins and no settings, in the empty `pass/` folder, as the §7.3 pass runs, given the pack and the current pattern files on stdin and asked for the sheet's lines as JSON. Its output is guarded like a fill (decision 10): a line's `events` must be pack event ids, a `target` must name a pattern or question that exists (or `new_question` / `pattern`), a `memory` line's file must pass §2 and §3's checks, and a line that fails is dropped and counted in `run.json`. Any pack event no surviving line holds is added as a `no-change` line with its reason `not addressed`, so **every event becomes a proposal** (decision 14).
- **Cost** about $1.5–3 a week [assumed, decision 14]; `run.json` records it only when the CLI reports it.

### 14.3 What a proposal is

Every event (§14.5) ends in one line of the sheet. By event: a run whose questions were skipped, overridden or edited → a `question`, `new_question`, `template` or `picker` change to its pattern; a session that went wrong after a footer-less start (an unmatched task start) → a `new-pattern` or a `picker` change that would have matched it; friction naming a specific, stable fact (a path, a tool's flag, a name) → a `memory` line; anything the drafter judges not worth a change → `no-change`; anything about the owner's own setup → `external`. **A specific, stable fact becomes a memory line, so patterns stay generic** (decision 14): the drafter is told so, and a proposed pattern passes `promptu lint` (§7.7) before it is a line.

### 14.4 The commands

#### `promptu week pack [--week <week>] [--projects <dir>] [--json]`

Reads `log/*.json`, `drafts/`, the transcripts of §5 (`<config home>/projects/*/*.jsonl`, top-level files, last modified within 30 days, never an `isSidechain` record, same reader as `used`; `--projects` replaces the folder), the pattern set, and the open sheet. Writes `week/<week>/pack.json` and `week/packed.json`. **No model, no network.** The week's window is the seven local days before the run's day (for a past `--week`, before that week's Monday plus the schedule's weekday), plus the carried events of §14.1 ([forks](forks.md) W1). Output (`--json`; the text view is the same counts): `{ "promptu": 1, "week", "pack": "<path>", "window": {"from","to"}, "runs": 31, "sessions": 18, "events": { "run": 31, "session": 18, "unmatched": 4, "outcome": 6, "carried": 3 } }`. Exit 0; 2 for a bad `--week` or an unreadable `--projects`. A bad `week_schedule` falls back to `wed 08:00` and is a `warnings` entry. **Built so far: the `run` events** (the first unit, EMB-273); `session`, `unmatched`, `outcome` and `carried` are counted `0` until EMB-274 builds them, and `week/packed.json`'s `through` is the window's last day.

#### `promptu week draft [--week <week>] [--pack <path>] [--json]`

Reads the pack (the week's, or `--pack`; a missing one is built first, as `pack` does), runs the drafter (§14.2), guards its lines, runs the first cut's checks for each pattern line (§10 `check`), writes `sheet.json`, `sheet.md`, `proposed/` and `run.json`, and supersedes the earlier open sheet (§14.1). On a drafter failure it writes `run.json` with `status: "failed"`, leaves every sheet as it was and exits 1. Output: `{ "promptu": 1, "week", "status": "ok", "sheet": "<path>", "lines": 12, "dropped": 1, "supersedes": "2026-W47" }`. A run with no events at all writes a sheet with no lines and exits 0.

#### `promptu week run [--week <week>] [--scheduled] [--json]`

`pack`, then `draft`, as one run: **what the timer's service runs**, with `--scheduled`, which sets `run.json`'s `kind` and `scheduled_for`. Output is `draft`'s with `pack`'s counts added. Exit 1 when either part fails.

#### `promptu week status [--json]`

Read-only; **what the chat asks, so it never asks the scheduler** (decision 45). `{ "promptu": 1, "schedule": "wed 08:00", "timer": { "installed": true, "installed_at": "…" }, "last": { "week", "started", "finished", "status", "error" } | null, "running": false, "next": "2026-12-02T08:00:00-06:00", "missed": false, "missed_since": null, "open": 23, "sheet": { "week", "generated" } | null, "warnings": [] }`. `next` is the first instant after now that is the schedule's weekday and time, local. **`missed`**: let S be the latest scheduled instant at or before now and after `timer.installed`; it is true when now is at least two hours past S ([forks](forks.md) W9) and no `run.json` has `started` at or after S (a `failed` run is not missed: `last.status` says it failed). With no timer installed, `next` is still computed and `missed` is `false`. **`running`** is true when a `run.json` says `status: "running"` with `finished: null`, started less than two hours ago ([forks](forks.md) W12): `week run` writes that record when it starts and rewrites it when it ends. `missed_since` is S when `missed`. `PROMPTU_NOW` (an RFC 3339 instant) pins the clock for tests, as `PROMPTU_TODAY` pins the day. A sheet that cannot be read is a `warnings` entry, not an exit. Exit 0, always.

#### `promptu week show [--week <week>] [--json]`

Read-only. The open sheet by default (or the week's own, which may be superseded or settled): `--json` prints `sheet.json` as it is; the text form is `sheet.md`. Exit 0; 2 for an unknown week or no sheet at all.

#### `promptu week mark <id>... [--verdict approve|cut] [--note <text>] [--feel better|same|worse] [--week <week>] [--json]`

`<id>` is a line id (`l1`) or a group id (`g1`, which sets `cut` on the group and every line in it). `--verdict` defaults to `cut`; `approve` undoes a cut on an `open` line. `--feel` sets the sheet's `feel` and takes no id. A `cut` sets the line's `status` to `cut`; marking a line `applied` or `failed` is refused (exit 2). It rewrites `sheet.json` and `sheet.md`. Output: the changed lines, `{ "promptu": 1, "week", "changed": [ { "id", "verdict", "status" } ], "open": 21 }`. Exit 0; 2 for an unknown id, a settled week, or a superseded sheet.

#### `promptu week apply [--week <week>] [--dry-run] [--json]`

Applies every `open` or `failed` line whose verdict is `approve`, in sheet order, on the open sheet (decisions 30, 43): `pattern` and `new-pattern` lines as §14.2 says, `memory` lines by writing the topic file and its index line into the project's memory folder and running the §4 checks on both (a check error makes the line `failed`), `external` and `no-change` lines left `open` until marked. It sets each line's `status`, `error` and `commit`, rewrites `sheet.json` and `sheet.md`, and **pushes nothing**. `--dry-run` runs the lint, the examples and the public-safety command on a temporary copy and writes nothing. Output: `{ "promptu": 1, "week", "applied": 5, "failed": 1, "skipped": 0, "branch": "promptu/week-2026-W48" | null, "patch": "<path>" | null, "lines": [ { "id", "status", "commit", "error" } ] }`. Exit 0 when no line failed; 1 when one did; 2 for no open sheet.

#### `promptu week timer [--print] [--remove] [--json]`

Writes the systemd user units from `week_schedule` (decision 45): `~/.config/systemd/user/promptu-week.timer` (`OnCalendar=Wed *-*-* 08:00:00`, `Persistent=true`) and `promptu-week.service` (`ExecStart=<this binary> week run --scheduled`, `Type=oneshot`), runs `systemctl --user daemon-reload` and `enable --now promptu-week.timer`, and writes `week/timer.json`. Run again after the schedule changes: it overwrites both units. `--print` prints the two units and writes nothing; `--remove` disables the timer, deletes the units and `timer.json`. Linux with systemd only: elsewhere it exits 2 with one line, and `status` still works. Output: `{ "promptu": 1, "schedule", "timer": "<path>", "service": "<path>", "enabled": true }`. Exit 0; 1 when `systemctl` fails (the units stay written); 2 for a bad schedule or no systemd.

#### `promptu desk --json`

§9.

### 14.5 The pack's records

`pack.json` is `{ "promptu": 1, "week", "window": { "from", "to" }, "built": "<instant>", "events": [ … ] }`, one record per event, each with a pack-unique `id` (`e1`…, in window order) and a `kind`. **Every record is read from the log, the drafts and the transcripts; nothing is judged by a model.** A record's quotes are the owner's own words, so the file stays local (§14.1).

**`run`**: a rendered run from `log/` whose `date` is in the window (decision 44).

```json
{ "id": "e1", "kind": "run", "run": "20251118-091500-3fa1", "date": "2025-11-18", "project": "-home-u-work-app",
  "pattern": "implement-issue", "version": 4, "mode": "prompt", "rewritten": true,
  "footer": "— promptu implement-issue@4 asked=… filled=… skipped=…", "asked": 3, "filled": 2, "skipped": 1, "assumed": 0,
  "skipped_questions": ["timebox"], "assumed_questions": [],
  "not_recommended": [ { "question": "board", "answer": "other" } ], "other_text": { "board": "the second one" },
  "changed_fills": ["flash"], "picked_by_owner": false,
  "sent": { "source": "transcript", "session": "5b1c…", "at": "2025-11-18T09:16:40-06:00", "text": "…" },
  "edit": "edited", "diff": "@@ -3 +3 @@\n-fix it\n+fix it now\n" }
```

`asked`, `filled`, `skipped`, `assumed` are the footer's counts (§6.1); `skipped_questions` and `assumed_questions` name them (an assumption is what Esc leaves); **`not_recommended`** lists each asked question whose code is not `rec` or `all`, with the code (`other`, `o2`, `o1+o3`); `other_text` is the "Other" text the owner typed, by question, read from `runs/<run>.json` and so **present only while that file survives its 7 days** ([forks](forks.md) W10); `changed_fills` the `!` fills (question ids); `picked_by_owner` is `pick=owner`. A rendered prompt with no footer line gets `footer: null` and zero counts.

**`sent`** is how the run's sent prompt was found (decision 44), tried in this order:

1. **The chat.** The first owner message after the log's time, in a session of that project (its main checkout, §1.2), carrying the run's footer line (`source: "transcript"`; `text` is the message). A tie between sessions goes to the text with the fewest changed lines against `rendered`, and the record says `"tie": true`. A message is matched to one run only, oldest run first.
2. **The terminal.** A terminal `/promptu` run is never an owner message: its prompt runs after the `Send as is` / `Edit first` question (`AskUserQuestion`, an option labelled `Send as is…`). The pack finds the transcript tool result that shows the run's id (`render`'s output) and takes the next such question's answer (`source: "terminal"`, `choice`: `send_as_is`, `edit_first` or `cancelled`). `send_as_is` sent `rendered`; `edit_first` sent the draft file `drafts/<run>.md` as it stands (`text: null` and `edit: "unknown"` if it was pruned); `cancelled` sent nothing.
3. **A changed draft** with no transcript evidence at all: `source: "draft"`, `edit: "edited"`. A draft equal to `rendered` says nothing about a send, and the run is `sent: null` ([forks](forks.md) W11).

`edit` is `unchanged` (equal to `rendered` after trimming and folding `\r\n`), `edited`, or `unknown` when `sent` is `null`, the draft is gone or the owner cancelled. **`diff`** is present when `edit` is `edited`: the sent text against `rendered` as a line diff, `-` a line he removed, `+` one he added, one line of context, hunks headed `@@ -<old line> +<new line> @@`. The match is a heuristic on two identical quick runs in one project, and `tie` says when it chose.

**`session`**: a session of the window's projects with a task start.

```json
{ "id": "e2", "kind": "session", "session": "5b1c…", "project": "-home-u-work-app", "started": "2025-11-18T09:16:40-06:00", "ended": "2025-11-18T11:02:10-06:00",
  "owner_messages": 14, "pattern": "implement-issue", "run": "20251118-091500-3fa1", "promptu": true,
  "interrupts": 2,
  "redirects": [ { "at": "2025-11-18T09:31:02-06:00", "after_assistant_messages": 6, "text": "no, the other repo", "signs": ["interrupt", "negation"] } ] }
```

`promptu` is true when the session's task start carried a footer other than `none` (`pattern` and `run` then name it); a session without one is the control group for decision 15. `interrupts` counts owner records that are Claude Code's interrupt marker. **`redirects`** are the model-free candidates for "friction": an owner message after the first that follows an interrupt or opens with a negation or correction phrase (the signs list says which); the drafter judges whether each was real ([forks](forks.md) W2). A session with no owner messages is skipped.

**`unmatched`**: a task start no pattern covered.

```json
{ "id": "e3", "kind": "unmatched", "session": "7d20…", "project": "-home-u-work-app", "at": "2025-11-19T14:02:11-06:00",
  "text": "…", "why": "no_footer", "words": 38 }
```

A session's first owner message (not `isMeta`, not a sidechain) with no `— promptu ` footer line (`why: "no_footer"`) or the footer `none` (`why: "footer_none"`), and over ten words, so a one-line question is not a task start.

**`outcome`**: the first cut's only outcome (decision 47), a commit.

```json
{ "id": "e4", "kind": "outcome", "session": "5b1c…", "run": "20251118-091500-3fa1", "type": "commit",
  "repo": "/home/u/work/app", "commits": 2, "first": "9f3a1c2", "at": "2025-11-18T10:48:00-06:00" }
```

A commit in the session's `cwd` checkout by the owner's `git config user.email`, dated between the session's start and its end plus one hour; one record per session with at least one, `commits` counting them ([forks](forks.md) W3). A session with a pattern and no commit has no outcome record, and the sheet's numbers say how many; that absence is not a failure, since many tasks are not commits. A PR merged or a study passed are later outcomes, each its own issue (decision 47).

**`carried`**: an event from an earlier open sheet, re-proposed (decision 46): `{ "id": "e5", "kind": "carried", "from": "2026-W47:l3", "events": [ <the earlier line's event records, quotes included> ] }`.

The pack's counts feed `numbers` (§10): `fills_kept` is one minus the share of `filled` entries marked `!`; `patterns_kept` is one minus the share of runs with `pick=owner`; `questions_per_run` the mean of `asked`; `friction_per_prompt` the redirects the drafter kept per owner prompt in `promptu` sessions, against the same in the others as the control (decision 15); `redirects_per_task` the redirect count per session.
