## Goal
generate a handoff prompt for a new thread to finish the OTA rollback work

Write a self-contained handoff brief for a fresh chat on the same work item, driven by me. They have not seen this conversation: write for a reader who did not watch, with no working shorthand, and quote my decisions in my own words.

## Context
The brief has these sections, each only if it has content:
1. Context: the goal and why, the state now, and what is done, each with its evidence (commits, commands and their output).
2. Read first, in order: files, issues and notes, by path.
3. Already decided: my words quoted exactly. Also what was tried and ruled out, and why.
4. Hard rules: the rules from this session and its memory that the reader would not otherwise see.
5. Work, in priority order: the first step checks the starting point (directory, git log, a quick build) before anything new.
6. Needs the owner: ask, don't guess.
7. Definition of done: the check and its exact command.

Hand over: everything still open.

## Done when
Their definition of done: the same definition of done this session works to, with its check command.
Save it as a markdown file in the plans folder and give me the one-line prompt that starts it.

## Constraints
- They work on this branch, feat/ota-rollback.

## Who does it
Write the brief yourself, now, from this session's full context; don't start the work.

— promptu handoff@1 asked=reader:rec,their_done:rec,scope:o1,branch:o1,save_to:rec
