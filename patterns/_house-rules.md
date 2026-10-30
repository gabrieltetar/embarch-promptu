---
name: house-rules
kind: fragment
version: 1
description: >-
  Four standing constraints that would otherwise be retyped at the start of
  many tasks.
  Included by task patterns; pre-filled from the prompt, the checkout, memory and
  last time; rendered under Constraints, near the end of the prompt.
questions:
  - id: rules
    header: House rules
    ask: Which of these hold for this task?
    type: multi
    options:
      - label: Use another checkout
        description: The main checkout is busy with another thread.
        value: "The main checkout is in use by another thread: work in a separate worktree or clone and leave the main checkout alone."
      - label: Only I open PRs
        description: Push if needed, then hand me the compare link.
        value: "Only I open pull requests. Push the branch if needed, then give me the compare link and a drafted title and body."
      - label: Public repo
        description: No client names, project names, issue keys or internals.
        value: "This lands in a public repo: no client names, project names, issue keys or client internals in code, docs or commit messages."
      - label: Wait for my green light
        description: Nothing touches the bench or the device until I say go.
        value: "Wait for my green light before touching the bench or the device under test."
    recommended: []
    default: []
    sources: [prompt, context, memory, last]
    gap: G3
    why: "Standing constraints are easy to lose in a long session, so they are stated near the end of the prompt."
---
{{#worktree}}
- You are in the worktree {{worktree}} on {{branch}}. Leave the main checkout alone.
{{/worktree}}
{{#rules}}
- {{.}}
{{/rules}}
