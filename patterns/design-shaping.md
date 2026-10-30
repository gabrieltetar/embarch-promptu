---
name: design-shaping
description: >-
  Explore and shape an idea before anything is built: a new tool, a module, a
  rework of something that exists. The outcome is notes or a proposal, not code.
  Use when the ask is open on purpose ("explore", "shape", "let's think", "ask me
  a lot of questions").
version: 1
kind: task
signals:
  phrases: ["\\bexplore", "\\bshape\\b", "\\bscope\\b", "brainstorm", "i want to (create|build|rework|explore)", "ask me (a lot of |many )?questions", "let'?s think", "\\bidea\\b", "proposal"]
  not: ["review the", "\\bbug\\b", "doesn'?t work"]
  desk: possible
include:
  - fragment: house-rules
    recommended: [Public repo, Use another checkout]
questions:
  - id: goal
    header: Goal
    ask: What is it for, and who will use it? One line.
    type: text
    suggest: 2
    fallback: not-sure
    required: true
    sources: [prompt, desk]
    gap: G9
    why: "An open-ended ask needs its goal most: without it, trade-offs go the wrong way."
  - id: mode
    header: Shape how
    ask: How should we shape it?
    type: choice
    options:
      - label: Interview me in rounds
        description: Questions first, notes after I say go.
        value: "Interview me before proposing anything: rounds of up to 4 questions with AskUserQuestion, 2–4 options each, your recommendation first. Keep going until I say go, then write the notes."
      - label: 2–4 options, then stop
        description: Trade-offs and a recommendation; build nothing.
        value: "Give me 2–4 options with their trade-offs and your recommendation, then stop. Build nothing."
      - label: Research first
        description: Look at what exists, then interview me.
        value: "First research what already exists (tools, papers, prior art), then interview me with what you found."
      - label: Over-generate, I cut
        description: Every angle, each with a ✓/✕ cut point.
        value: "Over-generate: every angle you can find, each item with a ✓/✕ cut point. I'll cut."
    recommended: Interview me in rounds
    required: true
    sources: [prompt, last]
    gap: G4
    why: "Shaping by questions first catches a wrong direction before anything is written."
  - id: web
    header: Web
    ask: May Claude search and read the web for this?
    type: choice
    when: "mode == 'Research first'"
    options:
      - label: Yes, search freely
        description: Web search and fetch as needed.
        value: "You may search and read the web for this; cite each source with its URL."
      - label: Local only
        description: Only files on this machine.
        value: "No web: use only files on this machine."
    recommended: Yes, search freely
    sources: [prompt]
    gap: G13
    why: "Asking for research and then being asked again for web permission costs a round trip."
  - id: decided
    header: Decided
    ask: What is already decided, and what is out of scope?
    type: text
    suggest: 1
    fallback: leave-out
    options:
      - label: Nothing yet
        description: Everything is open.
        value: "Nothing is decided yet."
    sources: [prompt, desk]
    gap: G13
    why: "Decisions made away from the repo are exactly what Claude can't read."
  - id: criteria
    header: Criteria
    ask: What matters most when comparing options?
    type: multi
    options:
      - label: Simplicity
        description: Fewer moving parts.
        value: "simplicity"
      - label: Reuse what exists
        description: Build on tools already in place.
        value: "reuse of what already exists"
      - label: Time to ship
        description: Something usable soon.
        value: "time to ship"
      - label: Running cost
        description: Tokens, services, upkeep.
        value: "running cost"
    recommended: [Simplicity, Reuse what exists]
    sources: [prompt, last]
    gap: G14
    why: "Without named criteria, a heavy design can be built before a light one is considered."
  - id: notes
    header: Notes go
    ask: Where do the notes go?
    type: text
    suggest: 1
    fallback: leave-out
    options:
      - label: A notes folder in docs
        description: Kept beside the repo's docs.
        value: "Keep the notes in a notes folder in the repo's docs, one file per topic."
      - label: Chat only
        description: Nothing written to disk.
        value: "Keep it in the chat; write no files."
    sources: [prompt, last]
    gap: G12
    why: "A named place for the notes outlives the chat."
examples:
  - rough: "I want to explore a small tool that tracks which boards are lent to whom. Let's be very open first."
    context: {branch: main}
    answers:
      goal: "So anyone on the team can see where each dev board is without asking in chat"
      mode: Interview me in rounds
      decided: Nothing yet
      criteria: [Simplicity, Reuse what exists]
      notes: A notes folder in docs
      rules: [Public repo]
---
## Goal
{{{ask}}}
{{#goal}}

For: {{goal}}
{{/goal}}

## Context
{{#decided}}
Decided or out of scope: {{decided}}
{{/decided}}
{{#criteria}}
- Weigh the options by {{.}}.
{{/criteria}}

## Done when
{{#notes}}
{{notes}}
{{/notes}}
The outcome is notes or a proposal, not code.

## Constraints
{{#web}}
- {{web}}
{{/web}}
{{> house-rules}}

## Who does it
{{mode}}
