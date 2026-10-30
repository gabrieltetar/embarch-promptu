# embarch-promptu

**Promptu turns a rough prompt into a good one, and keeps Claude Code's memory organised.** It works on its own in any Claude Code setup: one small Rust binary, `promptu`, plus a Claude Code plugin. Nothing else is needed. It also fits into [EmbArch](https://github.com/gabrieltetar/embarch-doc) when that is there (see [With EmbArch](#with-embarch)).

## Install

1. **The binary**, anywhere on `PATH`:

   ```sh
   cargo install --git https://github.com/gabrieltetar/embarch-promptu    # into ~/.cargo/bin
   # or, from a checkout:
   cargo build --release && install -Dm755 target/release/promptu ~/.local/bin/promptu
   ```

2. **The plugin** (the memory hooks and the `/promptu` skill), from [`plugin/`](plugin): add the `embarch` marketplace (`gabrieltetar/embarch-umbrella`) and install `embarch-promptu@embarch`, or try it in one session with `claude --plugin-dir /path/to/embarch-promptu/plugin`.

`/promptu` needs Claude Code logged in: its one model call runs through your own `claude` login, with no API key.

## Prompts

Type what you want the way you would anyway, after `/promptu`:

```
/promptu review the branch, it adds the CAN watchdog
```

1. **One quick model call** (Claude Sonnet through your own Claude Code login, no tools, nothing saved, about 4 seconds) picks the kind of task from a small set of patterns, and pre-fills what your prompt, your project's memory and last time's answers already say, each with a quote as evidence. A fill whose quote is not really in its source is thrown away.
2. **It asks only what is missing**, costliest gap first ("when is it done?" comes before "how should fixes be committed?"), in Claude Code's own question dialogs, with a recommended answer first. One option takes the rest as recommended; Esc turns what is left into assumptions the prompt tells Claude to state.
3. **It writes a short headed prompt**: Goal · Context · Read first · Done when · Constraints · Who does it, only the sections with content, **your own words kept verbatim under Goal**. A brief you paste that is already structured stays word for word; only its gaps are added.
4. **It shows you the prompt and asks "send as is / edit"** before Claude acts on it.

The last line of every prompt is a footer such as `— promptu review-loop@1 asked=stop:rec,lens:o1+o3 filled=target:prompt`: what was asked, filled and assumed. It is the only record a run leaves, and what a later weekly review learns from.

The eleven patterns are in [`patterns/`](patterns): implement an issue, a review loop, draft a message, relay a colleague's ask, shape a design, address PR comments, research and explain, debug a failure, write a handoff, a bench session, delegate to background agents; plus two fragments (house rules, bench state). Each is one Markdown file: the frontmatter is the quiz, the body the template. Everything specific to you comes from your memory at run time, never from a pattern file.

```sh
promptu prep < rough.txt      # pick, fill, plan the questions (JSON)
promptu next <run>            # a page of answers in (JSON), what is left out
promptu render <run>          # the prompt and its footer
promptu patterns [--json]     # the patterns, for a picker
promptu lint [--show]         # check pattern files; --show renders every example
```

The `/promptu` skill runs these for you. Settings, in `~/.config/embarch-promptu/config.toml`:

```toml
pass_effort = "medium"        # the model call's effort: low · medium · high
voice_skill = "my-voice"      # the skill that writes in your voice, for drafts
```

## Memory

Claude Code writes memories on its own: one Markdown file per fact in a project's auto-memory folder, and a `MEMORY.md` index of one-line pointers. Left alone, the index lines grow into paragraphs, facts get "Update:" paragraphs appended instead of being rewritten, and nothing says which memories are standing rules, which are traps, and which are status that will go stale.

Promptu gives every memory two labels and checks every write:

- **A kind**, in the topic file's frontmatter: `preference` · `rule` · `trap` · `reference` · `status`. A `status` carries `review-by:`, at most 90 days ahead; a `reference` carries `source:` (a repo path, an absolute path, a URL or a commit).
- **A domain**: the `## ` header its index line sits under. The headers are the project's short, fixed domain list.

A **PreToolUse hook** looks at every Write, Edit and MultiEdit into an auto-memory folder, works out the file as the write would leave it, and checks only what the write touches: the labels (T1–T5), no appended update paragraphs (T6), no dropped fields (T7), and index lines of at most 150 characters, under an existing domain, one per file (I1–I5). It makes no model call, starts in about a millisecond, and **fails open**: any problem of its own lets the write through. It starts in **warn mode** (the write goes ahead and Claude is told what to fix); **block mode** refuses a write with an error until it is fixed.

```sh
promptu memory              # what loads here: scope → domain → kind, with stale flags and where each is used
promptu memory --json       # the same, for a UI
promptu memory check        # run the checks on this project's memory as it stands
promptu memory check PATH   # ...or on given files or folders
```

The rule Claude follows when it writes a memory goes in your `~/.claude/CLAUDE.md`, for example:

```markdown
## Memory

When you write or update an auto-memory file:

- Add `kind:` to its frontmatter: `preference`, `rule`, `trap`, `reference` or `status`.
- A `status` memory also gets `review-by: YYYY-MM-DD`, at most 90 days ahead. A `reference` gets `source:`.
- Put its line in `MEMORY.md` under the `## <domain>` header it belongs to, in 150 characters or less, stating the fact itself.
- When a fact changes, rewrite the memory so it says what is true now; never append an "Update:" paragraph.
```

**Choose the hook's mode** in `~/.config/embarch-promptu/config.toml`:

```toml
memory_hook = "warn"   # "warn" (default) · "block" · "off"
```

`PROMPTU_MEMORY_HOOK` in the environment overrides it for one session.

## With EmbArch

[EmbArch](https://github.com/gabrieltetar/embarch-doc) is an optional integration. Promptu looks for two of its parts in the session and uses them when they are there:

- **The desk** (`desk`): `/promptu` reads the session's work item (title, description, comments) as one more source, and a few options appear, such as reading the issue with the desk's tools or leaving a handoff note on the work item. The desk's **Start work** types `/promptu --item <id> Work on KEY — title` for you, and Promptu then takes implementing the issue as the default kind of task (`prep --start`).
- **The bench tools** (`embarch-api`): bench sessions offer to build, flash and run through EmbArch's tools.

Without them those options are hidden and the generic ones remain, for example "flash with the project's own tooling". The `/promptu` skill detects the parts from the tools the session has; to force one either way:

```toml
[embarch]
desk = "auto"          # auto (default) · on · off
embarch-api = "auto"
```

Inside EmbArch the binary is installed with the suite's others, in `~/.local/share/embarch/bin/`, and the plugin comes from the suite's `embarch` marketplace in [embarch-umbrella](https://github.com/gabrieltetar/embarch-umbrella).

## What comes next

A weekly review of what the patterns got wrong: a ✓/✕ sheet as a Markdown file, applied with `promptu week apply` or reviewed with a `/promptu:review` skill, and shown in EmbArch's Desk when that is there. Then more reference kinds for EmbArch chat's linker. (The pass inside EmbArch's chat panel, with its Memory and Promptu menus, is EmbArch chat's own; it runs this CLI.)

## Docs

- [docs/contract.md](docs/contract.md): every interface: the hook's exact I/O and checks, the pattern format, `prep`/`next`/`render`'s JSON, the footer, and what is measured vs assumed.
- [docs/forks.md](docs/forks.md): design choices waiting for the owner.
- Design and decisions: [embarch-doc/embarch-promptu](https://github.com/gabrieltetar/embarch-doc/tree/main/embarch-promptu).

MIT.
