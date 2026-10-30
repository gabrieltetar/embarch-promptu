---
name: bench-session
description: >-
  Work on real hardware: bring-up items, validating a rework, running bench
  tests or a test list, measuring something on the board. Use when the result
  is an observation on the bench, not only a build.
version: 1
kind: task
signals:
  phrases: ["\\bbench\\b", "bring[- ]?up", "\\brework", "e[- ]?load", "hardware test", "test (it )?on the (board|hardware)", "validate", "\\bstudy\\b", "\\bplug(ged)?\\b"]
  not: []
  desk: possible
include:
  - fragment: bench-state
ask_first: [board, attached, instruments, gate]
questions:
  - id: pass
    header: Pass means
    ask: What observation on the bench means it passed?
    type: text
    suggest: 2
    fallback: not-sure
    options:
      - label: The issue's test list
        description: Each listed check passes.
        value: "each check listed in the work item passes"
    required: true
    sources: [prompt, desk, last]
    gap: G10
    why: "A clean build proves little on hardware; the check is an observation on the board."
  - id: instruments
    header: Instruments
    ask: Which instruments are wired up?
    type: multi
    options:
      - label: Scope or logic analyzer
        description: Probes on the signals.
        value: "a scope or logic analyzer is on the signals"
      - label: Electronic load
        description: Connected to the output.
        value: "an electronic load is connected"
      - label: Protocol analyzer
        description: Inline on the bus or port.
        value: "a protocol analyzer is inline"
      - label: None
        description: No instruments.
        value: "no instruments are connected"
    recommended: []
    always_ask: true
    sources: [prompt]
    gap: G1
    why: "Which instruments are connected decides which tests can run at all."
  - id: gate
    header: Bench gate
    ask: Who may act on the bench, and when?
    type: choice
    options:
      - label: My go for each step
        description: You prepare, I say go.
        value: "Wait for my green light before each step that touches the bench or the device."
      - label: Run; ask before power
        description: Ask before power or wiring changes.
        value: "Run the steps yourself; ask me before any change to power, wiring or the battery."
      - label: Fully yours
        description: I'm away; nothing needs my hands.
        value: "The bench is yours for this: run every step, and stop only if something needs my hands."
    recommended: My go for each step
    always_ask: true
    sources: [prompt, last]
    gap: G7
    why: "Whether someone else is using the bench, or wants to act on it themselves, changes from day to day."
  - id: flash
    header: Flash via
    ask: How should firmware get onto the board?
    type: choice
    options:
      - label: EmbArch
        description: build_and_flash and the study tools.
        value: "Build, flash and run through EmbArch's tools."
        requires: embarch-api
      - label: The project's own way
        description: Its build system's flash command or script.
        value: "Flash with the project's own tooling: its build system's flash command or script."
      - label: The vendor tool
        description: The chip vendor's flasher.
        value: "Flash with the chip vendor's own tool."
      - label: Commands for me
        description: I run them myself.
        value: "Give me the exact commands to paste; I run them."
    recommended: EmbArch
    sources: [prompt, memory, last]
    gap: G5
    why: "Flashing the wrong way can cost more time than the test itself; naming the path up front avoids a detour."
  - id: heavy
    header: How heavy
    ask: If firmware has to change for the test, how big a change?
    type: choice
    options:
      - label: Overlay or config first
        description: The lightest change that works.
        value: "If firmware must change for this, use the lightest form (a devicetree overlay or a config) before a new binding or board."
      - label: Full support
        description: A proper binding, driver or board.
        value: "If firmware must change, add proper support (binding, driver or board), not a workaround."
      - label: Show both first
        description: I pick.
        value: "If firmware must change, show me the light and the full way with their cost first, and wait for my pick."
    recommended: Overlay or config first
    sources: [prompt, last]
    gap: G14
    why: "A small overlay or config change is often enough for a test; a full driver or board port can come later."
  - id: timebox
    header: Time box
    ask: Is there a hard stop?
    type: choice
    options:
      - label: No hard stop
        description: Take the time it needs.
        value: ""
      - label: About two hours
        description: Then I leave the bench.
        value: "I leave the bench in about two hours: order the steps so the important ones run first."
      - label: Today only
        description: Finish or hand off by end of day.
        value: "Today only: stop at a point where the bench can be unplugged, and write down where we are."
    recommended: No hard stop
    sources: [prompt]
    gap: G16
    why: "A hard stop orders the work: the important checks run first, and the bench is left in a known state."
  - id: unblocks
    header: Unblocks
    ask: What does the result unblock?
    type: text
    suggest: 2
    fallback: leave-out
    sources: [prompt, desk]
    gap: G9
    why: "Knowing what the result unblocks tells Claude which observation matters most."
examples:
  - rough: "Two reworks on the stepper driver board need validating: a pull-down on the enable line and a 100 nF cap on the fault pin"
    context: {firmware: true}
    answers:
      board: "Stepper driver board rev C, both reworks done on unit 2"
      attached: [Debug probe, Bench supply, USB to the PC]
      pass: "The fault pin stays high through 50 enable toggles, and the motor holds still with enable low"
      instruments: [Scope or logic analyzer]
      gate: My go for each step
      flash: EmbArch
      heavy: Overlay or config first
      timebox: No hard stop
      unblocks: "The reworks go into the next board revision"
---
## Goal
{{{ask}}}
{{#unblocks}}

It unblocks: {{unblocks}}
{{/unblocks}}

## Context
Bench right now (as of this prompt):
{{> bench-state}}
{{#instruments}}
- Instruments: {{.}}.
{{/instruments}}

## Done when
Pass means: {{pass}}. Show the observation itself (log lines, measurements or a capture), not only a clean build.

## Constraints
{{#flash}}
- {{flash}}
{{/flash}}
{{#heavy}}
- {{heavy}}
{{/heavy}}
{{#timebox}}
- {{timebox}}
{{/timebox}}

## Who does it
{{gate}}
