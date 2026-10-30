---
name: delegate-agents
description: >-
  Hand a piece of work to background agents that run unattended: one worker or
  several in parallel, each briefed with its objective, output, tools, limits
  and stop rule, and reviewed before anything is installed or merged.
version: 1
kind: task
signals:
  phrases: ["background (workers?|agents?)", "in parallel", "\\bsubagents?\\b", "spawn", "\\bdelegate", "overnight", "while I('m| am) away"]
  not: []
  desk: never
include:
  - fragment: house-rules
    recommended: [Only I open PRs]
questions:
  - id: split
    header: Split
    ask: How is the work split between workers?
    type: choice
    options:
      - label: Propose it, wait for OK
        description: You split; I check the split.
        value: "Propose the split (pieces, which are independent, what each returns) and wait for my OK before starting any worker."
      - label: One worker
        description: A single background agent.
        value: "Use one background worker."
      - label: The pieces I listed
        description: One worker per piece in the ask.
        value: "One worker per piece listed in the ask."
    recommended: "Propose it, wait for OK"
    required: true
    sources: [prompt]
    gap: G4
    why: "Short, vague delegations make workers misread the task or duplicate each other."
  - id: returns
    header: Returns
    ask: What does each worker hand back?
    type: choice
    options:
      - label: Short report
        description: Done, evidence, still open.
        value: "a short report: what is done with its evidence (commands and output), and what is still open"
      - label: Findings list
        description: One line per finding, by severity.
        value: "a findings list, one line each, by severity, with file:line"
      - label: A file at a path
        description: Its result written to disk.
        value: "its result written to a file at a path named in its brief, plus a two-line summary"
    recommended: Short report
    required: true
    sources: [prompt, last]
    gap: G12
    why: "Reports in a fixed shape keep many results from filling this session."
  - id: limits
    header: Isolation
    ask: Which isolation rules apply to every worker?
    type: multi
    options:
      - label: Own worktree each
        description: No two workers in one checkout.
        value: "Each worker has its own worktree."
      - label: Own ports and build dirs
        description: Nothing shared that can collide.
        value: "Each worker uses its own ports, build and target directories."
      - label: Local commits only
        description: Nothing pushed by a worker.
        value: "Workers commit locally and push nothing."
      - label: Kill only its own PIDs
        description: Never kill processes by name.
        value: "A worker stops only processes it started, by PID; never by name."
    recommended: [Own worktree each, Own ports and build dirs, Local commits only, Kill only its own PIDs]
    sources: [prompt, memory, last]
    gap: G7
    why: "Parallel workers collide on shared checkouts, ports, build folders and processes unless each has its own."
  - id: contract
    header: Contract
    ask: Do the pieces share an interface?
    type: choice
    when: "split != 'One worker'"
    options:
      - label: Contract first
        description: Write the shared interface, then fan out.
        value: "Write the shared interface (types, routes, file formats) first; workers start only after it is fixed."
      - label: Independent pieces
        description: Nothing shared.
        value: ""
    recommended: Contract first
    sources: [prompt]
    gap: G13
    why: "Parallel pieces built against an unwritten interface have to be reconciled by hand."
  - id: stop
    header: Stops when
    ask: What stops a worker?
    type: choice
    options:
      - label: Done or blocked
        description: Report on either; never wait silently.
        value: "A worker stops when its brief's done-check passes, or when it is blocked; either way it reports."
      - label: A time box
        description: Hours, then report.
        value: "A worker stops after two hours of work and reports where it is."
      - label: A budget cap
        description: Spend, then report.
        value: "A worker stops at its share of the budget and reports where it is."
    recommended: Done or blocked
    required: true
    sources: [prompt, last]
    gap: G10
    why: "An unattended run needs to know what done looks like and what stops it."
  - id: landing
    header: Landing
    ask: What happens before a worker's result is installed or merged?
    type: choice
    options:
      - label: I review each first
        description: Nothing lands before my review.
        value: "Nothing a worker built is installed, deployed or merged before I review it; land one group at a time."
      - label: Review loop, then me
        description: An independent reviewer first.
        value: "Run an independent review loop on each result, then bring it to me before it lands; land one group at a time."
      - label: Land as they finish
        description: Fully scoped and low risk.
        value: "This is fully scoped and low risk: land each result as it finishes and its checks pass."
    recommended: I review each first
    required: true
    sources: [prompt, last]
    gap: G3
    why: "Reviewing a worker's result before it is installed or merged catches what the worker could not see."
examples:
  - rough: "use background workers to add JSON export to the three logger tools"
    context: {branch: main}
    answers:
      split: "Propose it, wait for OK"
      returns: Short report
      limits: [Own worktree each, Own ports and build dirs, Local commits only, Kill only its own PIDs]
      contract: Contract first
      stop: Done or blocked
      landing: I review each first
      rules: [Only I open PRs]
---
## Goal
{{{ask}}}

## Context
Workers see neither this chat nor my memory: restate in each brief every rule from this session that it must follow.
Each brief gives the worker its objective, what it returns, the tools and sources it may use, and what is off limits.
{{#contract}}
{{contract}}
{{/contract}}

## Done when
Each worker returns {{returns}}.
{{stop}}

## Constraints
{{#limits}}
- {{.}}
{{/limits}}
- {{landing}}
{{> house-rules}}

## Who does it
{{split}}
