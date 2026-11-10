## Goal
port the three logger tools to the new export format, one tool at a time

## Done when
It builds and the existing tests pass. Show the exact commands and their output, and don't edit existing tests to make them pass.

## Constraints
- Make the smallest change that does the job (a config, an overlay or a small patch) before anything bigger. Report extras instead of doing them.

## Who does it
Read first, then give me a short plan (files, steps, risks) and wait for my OK before editing.
- Context plan, scheduled compactions: Compact after each tool is ported, keeping the format decisions and the open defects. Why: The work stays in this thread but the history grows with every tool.

— promptu implement-issue@1 asked=done:all,goal:all,plan:all skipped=heavy,read,rules way=compactions
