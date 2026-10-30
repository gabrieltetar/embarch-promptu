## Goal
Work on ABC-31: debounce the brake input on the motor controller

Why: A noisy brake switch makes the motor stutter on the bench rig; the fix must hold at 20 kHz PWM

## Context
Work item ABC-31, "Debounce the brake input" (https://linear.app/example/issue/ABC-31).

## Read first
Read the work item's description, every comment and any linked PR with the desk's linear_get_issue tool before anything else.

## Done when
It builds, the tests pass, and the board boots with it: flash it and show the boot log.
Hardware: Plan one targeted bench test for this change and wait for my green light before running it.

## Constraints
- Make the smallest change that does the job (a config, an overlay or a small patch) before anything bigger. Report extras instead of doing them.
- You are in the worktree /work/motor-fw-ABC-31 on abc-31-debounce-brake-input. Leave the main checkout alone.
- Only I open pull requests. Push the branch if needed, then give me the compare link and a drafted title and body.

## Who does it
Read first, then give me a short plan (files, steps, risks) and wait for my OK before editing.

— promptu implement-issue@1 asked=done:o2,goal:other,plan:rec,heavy:rec,read:o1,hw_test:o3,rules:o2
