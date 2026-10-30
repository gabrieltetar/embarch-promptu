---
name: implement-issue
description: >-
  Build a feature or a change from an issue or an idea: a new driver, a setting,
  a fix with a known cause, a list of UI changes. The default when a task
  starts from a work item. Not for investigating an unknown failure (debug-failure) or answering
  review comments (address-pr-comments).
version: 1
kind: task
signals:
  phrases: ["\\bimplement", "let'?s (add|create|start|build|plan for)", "add (the )?(option|support|a setting)", "\\bdriver\\b", "work on [A-Z]{2,6}-\\d+", "feedback to address"]
  not: ["review comments", "investigate", "why does", "explain"]
  desk: default
include:
  - fragment: house-rules
    recommended: [Use another checkout, Only I open PRs]
questions:
  - id: done
    header: Done when
    ask: What does done look like, and how is it checked?
    type: choice
    options:
      - label: Build and tests pass
        description: Show the commands and their output.
        value: "It builds and the existing tests pass. Show the exact commands and their output, and don't edit existing tests to make them pass."
      - label: Plus a boot check
        description: Also flash it and see it boot.
        value: "It builds, the tests pass, and the board boots with it: flash it and show the boot log."
      - label: I'll judge it
        description: Stop when you think it's done and show me the diff.
        value: "Stop when you think it is done and show me the diff; I'll judge."
    recommended: Build and tests pass
    required: true
    sources: [prompt, desk, last]
    gap: G10
    why: "Without a definition of done, work stops too early or runs on too long."
  - id: goal
    header: Goal
    ask: Why are we doing this, and for whom? One line.
    type: text
    suggest: 2
    fallback: leave-out
    required: true
    sources: [prompt, desk]
    gap: G9
    why: "When the goal is missing, trade-offs go the wrong way."
  - id: plan
    header: Before edit
    ask: What happens before any file changes?
    type: choice
    options:
      - label: Plan, wait for my OK
        description: Files, steps and risks first.
        value: "Read first, then give me a short plan (files, steps, risks) and wait for my OK before editing."
      - label: Go, stop at design forks
        description: Start now; bring me each design fork.
        value: "Go ahead. At each design fork, give me the options with your recommendation and wait for my pick."
      - label: Fully scoped, finish it
        description: For background work I scoped completely.
        value: "This is fully scoped: finish without stopping, take your recommendation at small forks inside the scope, list each one in the summary, and stop for anything outside the scope."
    recommended: Plan, wait for my OK
    required: true
    sources: [prompt, last]
    gap: G4
    why: "Who decides, and when to stop, sets how much happens before the user sees it; design forks in interactive work wait for the user."
  - id: heavy
    header: How heavy
    ask: How big a change do you expect?
    type: choice
    options:
      - label: Smallest that works
        description: A config, an overlay or a small patch first.
        value: "Make the smallest change that does the job (a config, an overlay or a small patch) before anything bigger. Report extras instead of doing them."
      - label: Complete
        description: The full version, edge cases included.
        value: "Do the complete version, edge cases included. Report anything outside the ask instead of doing it."
      - label: Both, costs first
        description: Show me light and full, then I pick.
        value: "Before building, show me a light and a full option with what each costs, and wait for my pick."
    recommended: Smallest that works
    sources: [prompt, last]
    gap: G14
    why: "A heavy build can run on before a lighter option is considered, so the size is set up front."
  - id: read
    header: Read first
    ask: What should Claude read before starting, and what does not exist?
    type: text
    suggest: 2
    fallback: leave-out
    options:
      - label: The issue and its PR
        description: The work item's description, comments and linked PR.
        value: "Read the work item's description, every comment and any linked PR with the desk's linear_get_issue tool before anything else."
        requires: item
    sources: [prompt, desk]
    gap: G13
    why: "Pointers to the right sources, and to what does not exist, save a search."
  - id: hw_test
    header: Hardware
    ask: Does this need a test on the hardware?
    type: choice
    when: firmware
    options:
      - label: No hardware test
        description: The change is too small to need one.
        value: "No hardware test for this change."
      - label: Boot check only
        description: Flash and confirm it boots.
        value: "Flash it and check that it boots; nothing more on the bench."
      - label: Targeted test
        description: One test for this change, on my go.
        value: "Plan one targeted bench test for this change and wait for my green light before running it."
      - label: Full bench run
        description: The whole test list, on my go.
        value: "Run the full bench test list after the change, after my green light."
    recommended: Boot check only
    sources: [prompt, last]
    gap: G2
    why: "A hardware test nobody asked for costs bench time; a boot check catches a dead build cheaply."
examples:
  - rough: "Work on ABC-31: debounce the brake input on the motor controller"
    context: {item: {key: ABC-31, title: Debounce the brake input, url: "https://linear.app/example/issue/ABC-31"}, branch: abc-31-debounce-brake-input, worktree: /work/motor-fw-ABC-31, firmware: true}
    answers:
      done: Plus a boot check
      goal: "A noisy brake switch makes the motor stutter on the bench rig; the fix must hold at 20 kHz PWM"
      plan: Plan, wait for my OK
      heavy: Smallest that works
      read: The issue and its PR
      hw_test: Targeted test
      rules: [Only I open PRs]
    note: A run started from a work item; the worktree line replaces "Use another checkout".
  - rough: "Let's add retry with backoff to the provisioning upload"
    context: {branch: main, firmware: false}
    answers:
      done: Build and tests pass
      plan: Go, stop at design forks
      rules: [Use another checkout, Only I open PRs]
    note: Typed in the terminal with no work item; goal left out, the ask carries it.
---
## Goal
{{{ask}}}
{{#goal}}

Why: {{goal}}
{{/goal}}

## Context
{{#item.key}}
Work item {{item.key}}, "{{item.title}}" ({{item.url}}).
{{/item.key}}

## Read first
{{#read}}
{{read}}
{{/read}}

## Done when
{{done}}
{{#hw_test}}
Hardware: {{hw_test}}
{{/hw_test}}

## Constraints
{{#heavy}}
- {{heavy}}
{{/heavy}}
{{> house-rules}}

## Who does it
{{plan}}
