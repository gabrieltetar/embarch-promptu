---
name: promptu
description: Turn a rough task prompt into a good one before working on it. Promptu picks the kind of task, fills in what the prompt, the work item, memory and last time's answers already say, asks only what is missing (recommended answers first), writes a short headed prompt with the user's own words kept verbatim, and asks "send as is / edit" before anything runs. Only when the user types /promptu.
argument-hint: "[pattern=<id>] [--rec] <rough prompt>"
disable-model-invocation: true
allowed-tools: Bash(promptu prep:*), Bash(promptu next:*), Bash(promptu render:*)
---

# /promptu

The user typed `/promptu` with a rough prompt for a task. Promptu turns it into a better prompt through a short question round; then, once the user says so, you work on that prompt. Follow these steps exactly. You run three `promptu` commands and show dialogs; you do not reword, summarise or improve anything yourself.

**Promptu never blocks the task.** If `promptu` is not found, or any `promptu prep`, `next` or `render` exits non-zero, say so in one line (the command and its first stderr line, plus "the `promptu` binary must be on `PATH`" when it is missing), then work on the rough prompt **exactly as the user typed it**, as if they had sent it without `/promptu`. Do not retry, and do not stop.

The arguments, verbatim:

```
$ARGUMENTS
```

## 1. Read the arguments

- A leading `pattern=<id>` becomes `--pattern <id>` (`pattern=none` means "no pattern").
- A leading `--rec` becomes `--rec`: every question takes its recommended answer and nothing is asked.
- Everything else is the **rough prompt**. Keep it exactly as typed, line breaks included.
- No rough prompt at all: ask the user for it in one short line and stop until they answer.

## 2. Prep

Promptu works with or without EmbArch. Check which EmbArch parts your tools include, and pass them as `--have`:

- `desk` when you have the EmbArch desk's `desk_work_item` tool;
- `embarch-api` when you have EmbArch's bench tools (`build_and_flash`, `run_study`, or other `embarch-api` tools).

Pass only those present, comma-separated (`--have desk,embarch-api`), or `--have none`. What needs a missing part is hidden and the generic rest stays.

If you have `desk` and this session has a desk work item, call `desk_work_item` once, write `{"key": …, "title": …, "url": …, "text": …}` (text: its description and the newest comment heads, at most 2,000 characters) to a JSON file in your scratchpad or `/tmp`, and add `--desk <that file>`. Otherwise skip this.

Run, with the rough prompt in a quoted heredoc (never on the command line, never through `$…` substitution):

```bash
promptu prep --have <parts|none> [--pattern <id>] [--rec] [--desk <file>] <<'PROMPTU_ROUGH_PROMPT'
<the rough prompt, exactly>
PROMPTU_ROUGH_PROMPT
```

It prints JSON. Keep `run`. If `offline` is true, say so in one line with `offline_reason`; say each of `warnings` in one line too; and carry on. **If the output already has `prompt`** (the run could not be saved), nothing can be asked: go straight to step 5 with that `prompt`.

## 3. Ask

Each page in the JSON is **one AskUserQuestion call**: pass its `questions` exactly as given (same text, headers, options and order; do not add, drop or reword anything). `ids[i]` is the id of `questions[i]`.

1. If `choose` is not null, ask `choose.questions` first. Then run `promptu next <run>` with `{"pattern": "<the label the user picked>"}` on stdin.
2. While `pages` is not empty, ask `pages[0]`. Then run `promptu next <run>` with the answers on stdin, keyed by id:

   ```bash
   promptu next <run> <<'PROMPTU_ANSWERS'
   {"answers": {"<id>": "<the answer as returned>", "<id>": ["<label>", "<label>"], "<id>": {"other": "<text the user typed>"}}}
   PROMPTU_ANSWERS
   ```

   Pass each answer as AskUserQuestion returned it, "(Recommended)" and all; a multi-select may be a list or the returned comma-joined string. Each call prints the pages still to ask: always ask `pages[0]` of the **latest** output, since an answer can change what comes next, and "Take the rest as recommended" still leaves the required questions to ask.
3. **If the user dismisses a dialog** (Esc, or the call returns no answers), stop asking and go to step 4 with `--rest assume`: the questions left become assumptions the prompt tells you to state.

With `--rec`, or when `done` is true, there is nothing to ask.

## 4. Render

```bash
promptu render <run> [--rest assume]
```

It prints JSON with `prompt` (Markdown, ending in a `— promptu …` footer line) and `draft` (the same text in a file).

## 5. Show it, then wait

Show `prompt` to the user in full, exactly as printed, inside a fenced block. Then ask one AskUserQuestion:

- question: "Run this prompt?", header: `Draft`
- `Send as is (Recommended)`: "Work on it now, as written."
- `Edit first`: "Edit the draft file, then say go."
- `Cancel`: "Do nothing with it."

Then:

- **Send as is**: work on the prompt now, exactly as if the user had typed it as their message. It has had its question round: do not re-ask what it answers; ask only what it leaves open, and state each assumption it lists before you start. Its last line is bookkeeping for Promptu: ignore it.
- **Edit first**: give the user the `draft` path (and run `code <draft>` if `code` is on `PATH`), then wait. When the user says go, read the file and work on what it says now. With no `draft` in the output, ask the user to paste the edited prompt instead, and work on what they paste.
- **Cancel**: stop.

Never start the task before the user picks, and never change the prompt yourself.
