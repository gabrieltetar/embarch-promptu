## Goal
Review the current branch, it adds the CAN bus watchdog

## Context
Run an independent review loop on the current branch against its base: each round a fresh reviewer that did not write the code reads one context pack, and a findings ledger carries over between rounds (use the review-loop skill if it is installed).
- Look for correctness: bugs, edge cases and error paths.
- Look for concurrency: races, locking and what runs in interrupt context.
Skip style and naming unless they hide a bug.

## Done when
Stop when a round finds nothing of high or medium severity. List the low ones; don't fix them.
Each finding: severity, file:line, why it matters, the fix. Say so when a round finds nothing.

## Constraints
- Commit each round's fixes locally, one commit per round; push nothing.
- Only I open pull requests. Push the branch if needed, then give me the compare link and a drafted title and body.

## Who does it
Fix the findings yourself, except design questions: bring those to me with your recommendation and wait for my pick.

— promptu review-loop@1 asked=stop:rec,target:rec,lens:o1+o3,fixes:rec,commits:rec,budget:rec,rules:rec
