# embarch-promptu

**Promptu keeps Claude Code's memory organised, and from its second build step turns a rough prompt into a good one.** It is part of [EmbArch](https://github.com/gabrieltetar/embarch-doc), works on its own in any Claude Code setup, and ships as one small Rust binary, `promptu`, plus a Claude Code plugin.

## What it does today (build step 1: memory)

Claude Code writes memories on its own: one Markdown file per fact in a project's auto-memory folder, and a `MEMORY.md` index of one-line pointers. Left alone, the index lines grow into paragraphs, facts get "Update:" paragraphs appended instead of being rewritten, and nothing says which memories are standing rules, which are traps, and which are status that will go stale.

Promptu gives every memory two labels and checks every write:

- **A kind**, in the topic file's frontmatter: `preference` · `rule` · `trap` · `reference` · `status`. A `status` carries `review-by:`, at most 90 days ahead; a `reference` carries `source:` (a repo path, an absolute path, a URL or a commit).
- **A domain**: the `## ` header its index line sits under. The headers are the project's short, fixed domain list.

A **PreToolUse hook** looks at every Write, Edit and MultiEdit into an auto-memory folder, works out the file as the write would leave it, and checks only what the write touches: the labels (T1–T5), no appended update paragraphs (T6), no dropped fields (T7), and index lines of at most 150 characters, under an existing domain, one per file (I1–I5). It makes no model call, starts in about a millisecond, and **fails open**: any problem of its own lets the write through. It starts in **warn mode** (the write goes ahead and Claude is told what to fix); **block mode** refuses a write with an error until it is fixed.

```sh
promptu memory              # what loads here: scope → domain → kind, with stale flags
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

## Install

```sh
cargo build --release
install -Dm755 target/release/promptu ~/.local/share/embarch/bin/promptu   # any folder on PATH; -D makes it
```

The plugin is in [`plugin/`](plugin). Where it will be listed is still open ([docs/forks.md](docs/forks.md) F3); to try it in one session without installing anything, run `claude --plugin-dir /path/to/embarch-promptu/plugin`.

**Choose the mode** in `~/.config/embarch-promptu/config.toml`:

```toml
memory_hook = "warn"   # "warn" (default) · "block" · "off"
```

`PROMPTU_MEMORY_HOOK` in the environment overrides it for one session.

## What comes next

Build step 2 adds **prompt patterns**: one Markdown file per kind of task, whose frontmatter is a short quiz and whose body is the prompt's template. `/promptu <rough prompt>` picks the pattern, fills what the prompt, the work item, memory and past answers already say, asks only what is missing, and writes a short headed prompt with your own words kept verbatim. Steps 3–5 bring it into EmbArch's chat panel and desk, add a weekly review of what the patterns got wrong, and extend the chat's reference linker.

## Docs

- [docs/contract.md](docs/contract.md): every interface, the hook's exact I/O and checks, and what is measured vs assumed.
- [docs/forks.md](docs/forks.md): design choices waiting for the owner.
- Design and decisions: [embarch-doc/embarch-promptu](https://github.com/gabrieltetar/embarch-doc/tree/main/embarch-promptu).

MIT.
