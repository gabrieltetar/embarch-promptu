---
name: research-explain
description: >-
  Find out or explain something: sizing and feasibility, options for a part or
  a tool, how a mechanism works, what a spec says. The outcome is an answer, not
  a change. Not for a short question in the middle of a task.
version: 1
kind: task
signals:
  phrases: ["research", "is there a way", "should we", "feasib", "options for", "candidates", "deep research", "find (a |the )?(good )?source", "explain (to me )?(how|why|what)", "how does .* work", "what would be missing"]
  not: ["\\bfix\\b", "implement"]
  desk: possible
questions:
  - id: use
    header: Answer for
    ask: What will you do with the answer?
    type: choice
    options:
      - label: Pick between options
        description: A decision to make.
        value: "I need to pick between options."
      - label: Go or no-go
        description: Is it feasible at all?
        value: "I need a go or no-go on feasibility."
      - label: Understand it
        description: For me.
        value: "I want to understand it myself."
      - label: Explain it to someone
        description: A colleague will read it.
        value: "I will explain it to a colleague, so it must stand on its own."
    recommended: Understand it
    required: true
    sources: [prompt, desk]
    gap: G9
    why: "What the answer is for sets its form."
  - id: web
    header: Web
    ask: May Claude search and read the web?
    type: choice
    options:
      - label: Yes, search freely
        description: Web search and fetch as needed.
        value: "You may search and read the web for this; this counts as my yes for every search it needs, subagents included."
      - label: Local files only
        description: Nothing from the web.
        value: "No web: answer from files on this machine and what you know, and say which is which."
    recommended: Yes, search freely
    required: true
    sources: [prompt, last]
    gap: G13
    why: "Asking for research and then being asked again for web permission costs a round trip."
  - id: depth
    header: Depth
    ask: How deep?
    type: choice
    options:
      - label: Short answer
        description: A few sources, a few minutes.
        value: "Keep it short: the answer and the two or three sources that decide it."
      - label: Thorough
        description: Every relevant source, compared.
        value: "Be thorough: every relevant source, compared, with what each one says."
      - label: Deep research
        description: Parallel searches, many sources.
        value: "Do deep research: parallel searches, many sources, primary sources first."
    recommended: Short answer
    sources: [prompt, last]
    gap: G16
    why: "Depth sets the time and token cost; budget for breadth is granted explicitly."
  - id: start_from
    header: Start from
    ask: What should it start from, and what does not exist?
    type: text
    suggest: 2
    fallback: leave-out
    options:
      - label: What's in the repo
        description: Local docs and datasheets.
        value: "the docs and datasheets already in the repo"
    sources: [prompt, desk, memory]
    gap: G13
    why: "Pointing at the sources that exist, and saying which do not, saves a search for them."
  - id: answer_as
    header: Answer as
    ask: What form should the answer take?
    type: choice
    options:
      - label: Verdict and evidence
        description: One line, then why.
        value: "Lead with a one-line verdict, then the evidence."
      - label: Ranked options
        description: 2–4, with trade-offs and a recommendation.
        value: "Give 2–4 ranked options with their trade-offs, your recommendation first."
      - label: Step by step
        description: An explanation, in order.
        value: "Explain it step by step, defining each term the first time."
      - label: A notes file
        description: Written to disk, link in the reply.
        value: "Write it to a notes file and give me the path."
    recommended: Verdict and evidence
    sources: [prompt, last]
    gap: G12
    why: "The form of the answer decides whether it can be used straight away."
  - id: criteria
    header: Criteria
    ask: What matters most in the comparison?
    type: multi
    when: "use == 'Pick between options'"
    options:
      - label: Cost
        description: Parts or running cost.
        value: "cost"
      - label: Size and power
        description: Flash, RAM, current.
        value: "size and power"
      - label: Time to ship
        description: Available soon.
        value: "time to ship"
      - label: Simplicity
        description: Fewer moving parts.
        value: "simplicity"
    recommended: [Simplicity]
    sources: [prompt, last]
    gap: G14
    why: "Named criteria make options comparable."
examples:
  - rough: "Should the data logger keep a week of samples in RAM, or write them to the SD card every hour?"
    context: {firmware: true}
    answers:
      use: Pick between options
      web: Yes, search freely
      depth: Thorough
      start_from: What's in the repo
      answer_as: Ranked options
      criteria: [Size and power, Simplicity]
---
## Goal
{{{ask}}}
{{#use}}

{{use}}
{{/use}}

## Context
{{#criteria}}
- Compare on {{.}}.
{{/criteria}}

## Read first
{{#start_from}}
Start from {{start_from}}.
{{/start_from}}

## Done when
{{answer_as}}
{{#depth}}
{{depth}}
{{/depth}}

## Constraints
- {{web}}
- Cite each claim: a URL with the date read, or file:line. For versions and current specifics, check a source instead of recalling.
- Change nothing; this is research.
