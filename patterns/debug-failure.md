---
name: debug-failure
description: >-
  Investigate a failure whose cause is not known yet: something stays off,
  crashes, resets, hangs, returns the wrong value or stopped working. The first
  deliverable is a diagnosis with evidence, not a fix.
version: 1
kind: task
signals:
  phrases: ["investigate", "\\bdebug", "why (does|is|do|did)", "doesn'?t (work|open|start|boot)", "\\bfails?\\b", "stays off", "\\bcrash", "\\bhangs?\\b", "regression", "not working", "broke"]
  not: ["review comments"]
  desk: possible
include:
  - fragment: bench-state
    when: hardware
  - fragment: house-rules
    recommended: [Wait for my green light]
questions:
  - id: symptom
    header: Symptom
    ask: What did you expect, and what happens instead?
    type: text
    suggest: 1
    fallback: not-sure
    required: true
    sources: [prompt, desk]
    gap: G9
    why: "An incomplete description of expected versus actual behaviour hurts correctness most."
  - id: first
    header: First step
    ask: What comes before any fix?
    type: choice
    options:
      - label: Ranked hypotheses, stop
        description: Evidence for each, then I pick.
        value: "Before any fix, give me ranked hypotheses, each with the evidence for and against it, then stop and wait for my pick."
      - label: Test my hypothesis
        description: Mine is in the ask; check it first.
        value: "Test the hypothesis in my ask first and report the evidence, then stop."
      - label: Hypotheses, fix the top
        description: Fully scoped; fix if the evidence is clear.
        value: "Rank the hypotheses with evidence; if the top one is clearly supported, fix it and show the evidence; otherwise stop and ask."
    recommended: "Ranked hypotheses, stop"
    required: true
    sources: [prompt, last]
    gap: G4
    why: "Following one plausible theory can be a long detour; competing hypotheses with their evidence come first."
  - id: repro
    header: Reproduces
    ask: How often does it happen, and how do you trigger it?
    type: choice
    options:
      - label: Every time
        description: Put the steps in Other if not in the ask.
        value: "It happens every time."
      - label: Intermittently
        description: Some runs only.
        value: "It happens intermittently, not on every run."
      - label: Seen once
        description: Not reproduced yet.
        value: "It was seen once and is not reproduced yet."
    recommended: Every time
    sources: [prompt]
    gap: G13
    why: "The reproduction and whether it is intermittent decide the whole approach."
  - id: ruled_out
    header: Ruled out
    ask: What have you already tried or ruled out?
    type: text
    suggest: 1
    fallback: leave-out
    options:
      - label: Nothing yet
        description: Start from scratch.
        value: "nothing yet"
    sources: [prompt, desk]
    gap: G13
    why: "Saying what was ruled out stops dead ends from being walked again."
  - id: fixed
    header: Fixed means
    ask: When it comes to a fix, what proves it is fixed?
    type: choice
    options:
      - label: Gone in ten tries
        description: The trigger no longer reproduces it.
        value: "The trigger no longer reproduces it in ten tries."
      - label: A test fails, then passes
        description: Reproduce it in a test first.
        value: "A test reproduces the failure first, then passes with the fix."
      - label: I confirm on the bench
        description: My observation decides.
        value: "I confirm it on the bench."
    recommended: Gone in ten tries
    sources: [prompt, last]
    gap: G10
    why: "A fix proven only by a build can address the wrong cause; the proof is the symptom going away."
examples:
  - rough: "After an OTA update the sensor node sometimes boots the old image. investigate"
    context: {firmware: true, hardware: true}
    answers:
      symptom: "Expected: the new image runs after the update. Actual: about 1 update in 5 falls back to the old image."
      first: "Ranked hypotheses, stop"
      repro: Intermittently
      ruled_out: "Not a signature failure: the image verifies on the host"
      fixed: Gone in ten tries
      board: "Sensor node rev B, no reworks"
      attached: [Debug probe, Bench supply]
      rules: [Wait for my green light]
---
## Goal
{{{ask}}}

## Context
Expected versus actual: {{symptom}}
{{#repro}}
{{repro}}
{{/repro}}
{{#ruled_out}}
Already ruled out: {{ruled_out}}.
{{/ruled_out}}
{{#hardware}}
Bench right now (as of this prompt):
{{> bench-state}}
{{/hardware}}

## Done when
The diagnosis comes with its evidence (logs, measurements, file:line).
{{#fixed}}
For a fix: {{fixed}}
{{/fixed}}

## Constraints
- Run a command that changes state (flash, erase, reset, power) only when the evidence calls for it.
{{> house-rules}}

## Who does it
{{first}}
