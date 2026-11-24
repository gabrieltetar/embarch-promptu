---
name: review
description: Walk the weekly Promptu sheet in the terminal, for a user without the EmbArch Desk. Shows each group's lines as ✓ or ✕ questions (a typed note is kept on the line), records each answer with `promptu week mark`, asks how the week felt (better, same, worse), then runs `promptu week apply` and shows what applied and what failed. Only when the user types /embarch-promptu:review.
disable-model-invocation: true
allowed-tools: Bash(promptu week show:*), Bash(promptu week mark:*), Bash(promptu week apply:*)
---

# /embarch-promptu:review

The user typed `/embarch-promptu:review` to go through this week's Promptu sheet: the lines Promptu proposes for its patterns and memory, each one kept (✓) or cut (✕). You run three `promptu week` commands and show dialogs; you do not reword, judge or improve any line yourself.

**This skill never changes anything when it cannot work.** If `promptu` is not found, or `promptu week show` exits non-zero, say so in one line (the command and its first stderr line, plus "the `promptu` binary must be on `PATH`" when it is missing) and stop. If a `promptu week mark` exits non-zero, say so in one line and stop: do not retry and do not run `apply`. Marks already recorded stay; say how many.

## 1. Read the sheet

```bash
promptu week show --json
```

Parse the JSON (`groups[].lines[]`; fields in the contract's §10). No sheet is exit 2: say "no sheet to review" and stop. If `superseded_by` is set, say which sheet replaced it and stop.

The lines to ask about are those whose `status` is `open` or `failed`, and whose `kind` is not `no-change` (those are already cut and only explain events). If there are none, skip to step 3's feel question, then stop after it: do not run `apply`.

## 2. Ask, one group at a time

For each group, in sheet order, make pages of at most four of its lines; each page is **one AskUserQuestion call** with one question per line:

- question: the line's `text`, then its `kind` and `count` in one short clause; for a `failed` line add its `error`; for an `external` line add "outside Promptu's files: shown, never applied". Keep the group's `title` in the first question of the page.
- header: the line id (`l3`).
- options: `Keep ✓ (Recommended)`: "Apply it." and `Cut ✕`: "Leave it out." (a line whose verdict is already `cut` lists `Cut ✕` first, with `(Recommended)`, and `Keep ✓` second). The dialog's free-text answer is **a note**: the line is kept, and the note is recorded on it.

Do not add, drop or reword a line. If the user dismisses a dialog, stop asking: record nothing from that page and go to step 3.

After each page, record its answers (single-quote a note and write each `'` in it as `'\''`):

```bash
promptu week mark l1 l4 --verdict cut
promptu week mark l2 --verdict approve
promptu week mark l3 --verdict approve --note '<the typed note>'
```

One call per kind of answer, ids together; skip a line whose answer matches its current verdict and has no note. A line is never marked `applied` or `failed`.

## 3. The feel line

Ask one AskUserQuestion: question "How did the week feel with Promptu?", header `Feel`, options `Better`, `Same`, `Worse`. Then:

```bash
promptu week mark --feel <better|same|worse>
```

If the user dismisses it, skip it.

## 4. Apply

Ask one AskUserQuestion: question "Apply the kept lines now?", header `Apply`, options `Apply (Recommended)`: "Write the kept lines; nothing is pushed." and `Not now`: "Stop; the marks stay on the sheet." On `Not now`, stop. On Apply:

```bash
promptu week apply --json
```

Exit 1 means a line failed, which is not an error to stop on; exit 2 is a failure to report in one line. Show `applied`, `failed` and `skipped`, then each failed line's id and `error` and, when `branch` or `patch` is not null, where the pattern change landed (it is never pushed). Say that a failed line stays on the sheet and the next apply retries it, and that `external` lines are only done by the user.
