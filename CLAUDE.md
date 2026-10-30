# embarch-promptu

## Docs

**Four files, not one.** Current truth: [spec.md](../embarch-doc/embarch-promptu/spec.md). Why it is that way: [decisions.md](../embarch-doc/embarch-promptu/decisions.md) — a decision number addresses this sub-project, not a file. Unresolved: [open.md](../embarch-doc/embarch-promptu/open.md). Reference: [interfaces.md](../embarch-doc/embarch-promptu/interfaces.md), and in this repo [docs/contract.md](docs/contract.md), the interface every build step shares, and [docs/forks.md](docs/forks.md), the owner's open choices.

Update them proactively per [../embarch-doc/DOC-PROTOCOL.md](../embarch-doc/DOC-PROTOCOL.md) whenever a notable design decision, feature, or status change happens here — §4 says when, §5 says how, and history goes in a `changelog.d/` fragment rather than into a doc. **A change to an interface changes `docs/contract.md` in the same commit**; §11 there says what may never change.

**This repo is public.** Patterns, fixtures and examples are generic and invented: no client or project names, no issue keys, no real prompts or memories. The owner's learning data stays outside every repo, and so does any measurement on his real prompts. **Never test against the real `~/.claude`, `~/.config` or `~/.local/state`**: use a scratch `CLAUDE_CONFIG_DIR`, `PROMPTU_CONFIG_DIR`, `PROMPTU_STATE_DIR` and `PROMPTU_TODAY`, and a fake `claude` through `PROMPTU_CLAUDE`, as `tests/cli.rs` and `tests/prompts.rs` do.

**Patterns** are `patterns/*.md`, embedded in the binary at build time; the format is contract §7.1. A pattern change keeps `promptu lint` clean and updates its golden renders (`PROMPTU_BLESS=1 cargo test --test prompts`, then read the diff). Bump a pattern's `version` with every change once it has shipped.

**Design forks are the owner's.** A choice the design does not settle goes to `docs/forks.md`, ranked, with options and a recommendation, and runs on the most reversible default, marked provisional.

## Git

**Work directly on `main` — no feature branches, no PRs (2026-08-25).** Overrides the general "branch before committing to the default branch" default, for this suite only; ends when the repo owner explicitly says so. See [../embarch-doc/embarch-dev-workflow.md](../embarch-doc/embarch-dev-workflow.md) §6.

## Gate

`cargo build`, `cargo test --all-targets` (it runs `promptu lint` on the shipped patterns and pins every example render) and `cargo clippy --all-targets -- -D warnings`. Before a commit: `python3 ../embarch-fleet/scripts/check-client-names.py --repo .`, and the suite's local public-safety check over this folder, gated on its exit code directly, never through a pipe. Never run `cargo fmt`: the suite is not rustfmt-styled.

**The hook's speed is a gate too**: it runs on every Write, Edit and MultiEdit in every session. Keep it free of model calls, network and async runtimes, and re-measure (docs/contract.md §3.8) after touching its path.
