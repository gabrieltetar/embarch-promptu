---
name: review-loop
description: >-
  Review a branch, a PR or a set of files with an independent reviewer, round
  after round, until a stop rule is met. Use for "review the current branch",
  "independent reviewer loop", "check this before I open the PR". Not for
  answering comments a human reviewer left (address-pr-comments).
version: 1
kind: task
signals:
  phrases: ["\\breview\\b", "reviewer loop", "independent review", "before (I|we) open the pr", "audit (the|this) (branch|code)"]
  not: ["address (all )?(the )?comments", "reply to"]
  desk: possible
include:
  - fragment: house-rules
    recommended: [Only I open PRs]
questions:
  - id: stop
    header: Stop when
    ask: When does the loop stop?
    type: choice
    options:
      - label: No high or medium left
        description: Low findings are listed, not fixed.
        value: "Stop when a round finds nothing of high or medium severity. List the low ones; don't fix them."
      - label: No findings at all
        description: Every finding fixed.
        value: "Stop when a round finds nothing at all."
      - label: Three rounds at most
        description: A hard cap.
        value: "Stop after three rounds at most, whatever is left; list what is left."
      - label: One round, report only
        description: No loop, no fixes.
        value: "One round only: report the findings and change nothing."
    recommended: No high or medium left
    required: true
    sources: [prompt, last]
    gap: G10
    why: "Without a stop rule a review loop can run until the budget is gone; the stop rule decides when it is done."
  - id: target
    header: Review what
    ask: What is under review?
    type: choice
    options:
      - label: This branch
        description: The current branch against its base.
        value: "the current branch against its base"
      - label: An open PR
        description: Name it in Other if it is not this branch's.
        value: "the open pull request for this branch"
      - label: Named files
        description: List them in Other.
        value: "the files I name"
    recommended: This branch
    required: true
    sources: [prompt, desk, context]
    gap: G6
    why: "The review target is usually known from the checkout; naming it removes a guess."
  - id: lens
    header: Lens
    ask: What should the reviewer look for?
    type: multi
    options:
      - label: Correctness
        description: Bugs, edge cases, error paths.
        value: "correctness: bugs, edge cases and error paths"
      - label: Spec compliance
        description: Name the spec and version in Other.
        value: "compliance with the spec I name, cited by section"
      - label: Concurrency and ISR
        description: Races, locking, interrupt context.
        value: "concurrency: races, locking and what runs in interrupt context"
      - label: Security
        description: Inputs, secrets, permissions.
        value: "security: untrusted input, secrets and permissions"
    recommended: [Correctness]
    required: true
    sources: [prompt, last]
    gap: G10
    why: "A reviewer asked only to find problems always finds something; a named lens keeps findings worth fixing."
  - id: fixes
    header: Findings
    ask: What happens to the findings?
    type: choice
    options:
      - label: Fix; ask me on design
        description: Fix the rest yourself.
        value: "Fix the findings yourself, except design questions: bring those to me with your recommendation and wait for my pick."
      - label: Report only
        description: Change nothing.
        value: "Report the findings and change nothing."
      - label: Fix everything
        description: Fully scoped, no stops.
        value: "This is fully scoped: fix every finding without stopping, and list each design choice you made in the summary."
    recommended: "Fix; ask me on design"
    required: true
    sources: [prompt, last]
    gap: G4
    why: "Design questions need the user's judgement; the rest the loop can fix itself."
  - id: commits
    header: Commits
    ask: How are fixes committed?
    type: choice
    options:
      - label: A local commit per round
        description: Nothing pushed.
        value: "Commit each round's fixes locally, one commit per round; push nothing."
      - label: Commit and push per round
        description: The branch stays current upstream.
        value: "Commit each round's fixes and push the branch."
      - label: No commits
        description: Leave the changes in the tree.
        value: "Don't commit; leave the changes in the working tree."
    recommended: A local commit per round
    sources: [prompt, last]
    gap: G3
    why: "Whether fixes are committed, and pushed, is a question of posting authority; it should be explicit."
  - id: budget
    header: Budget
    ask: How much should the loop spend?
    type: choice
    options:
      - label: Normal
        description: Default effort.
        value: ""
      - label: Go broad
        description: Tokens to spend; aim for thorough.
        value: "There are tokens to spend: go broad and aim for thorough."
      - label: Keep it cheap
        description: Small rounds, fewer reviewers.
        value: "Keep it cheap: one reviewer per round, short reports."
    recommended: Normal
    sources: [prompt]
    gap: G16
    why: "How much a review may spend depends on the moment; saying so sets its breadth."
examples:
  - rough: "Review the current branch, it adds the CAN bus watchdog"
    context: {branch: feat/can-watchdog, firmware: true}
    answers:
      stop: No high or medium left
      target: This branch
      lens: [Correctness, Concurrency and ISR]
      fixes: "Fix; ask me on design"
      commits: A local commit per round
      budget: Normal
      rules: [Only I open PRs]
---
## Goal
{{{ask}}}

## Context
Run an independent review loop on {{target}}: each round a fresh reviewer that did not write the code reads one context pack, and a findings ledger carries over between rounds (use the review-loop skill if it is installed).
{{#lens}}
- Look for {{.}}.
{{/lens}}
Skip style and naming unless they hide a bug.

## Done when
{{stop}}
Each finding: severity, file:line, why it matters, the fix. Say so when a round finds nothing.

## Constraints
{{#commits}}
- {{commits}}
{{/commits}}
{{#budget}}
- {{budget}}
{{/budget}}
{{> house-rules}}

## Who does it
{{fixes}}
