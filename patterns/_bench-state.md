---
name: bench-state
kind: fragment
version: 1
description: >-
  What is on the bench right now. Volatile, so it is asked on every run that
  includes it; only the prompt itself can pre-select an answer, never memory
  or last time.
questions:
  - id: board
    header: Board
    ask: Which board is on the bench, with its revision and any reworks?
    type: text
    suggest: 2
    fallback: not-sure
    always_ask: true
    sources: [prompt, desk, last]
    gap: G1
    why: "What is on the bench changes from day to day, and a plan built on the wrong board or revision wastes the session."
  - id: attached
    header: Plugged in
    ask: What is plugged into it right now?
    type: multi
    options:
      - label: Debug probe
        description: SWD or JTAG probe attached.
        value: "A debug probe (SWD/JTAG) is attached. It can hold the MCU off or add current, so say when it should come off."
      - label: Bench supply
        description: Powered from a lab supply.
        value: "It is powered from a bench supply."
      - label: Battery
        description: Powered from a battery.
        value: "It runs from a battery."
      - label: USB to the PC
        description: A USB cable to this machine.
        value: "A USB cable connects it to this machine."
    recommended: []
    always_ask: true
    sources: [prompt]
    gap: G1
    why: "A probe, a supply or a cable left connected can hold a board off or change what a test measures."
---
{{#board}}
- Board: {{board}}
{{/board}}
{{^board}}
- Board: not stated. Ask me before relying on it.
{{/board}}
{{#attached}}
- {{.}}
{{/attached}}
{{^attached}}
- What is plugged in: not stated. Ask me before relying on it.
{{/attached}}
