## Goal
use background workers to add JSON export to the three logger tools

## Context
Workers see neither this chat nor my memory: restate in each brief every rule from this session that it must follow.
Each brief gives the worker its objective, what it returns, the tools and sources it may use, and what is off limits.
Write the shared interface (types, routes, file formats) first; workers start only after it is fixed.

## Done when
Each worker returns a short report: what is done with its evidence (commands and output), and what is still open.
A worker stops when its brief's done-check passes, or when it is blocked; either way it reports.

## Constraints
- Each worker has its own worktree.
- Each worker uses its own ports, build and target directories.
- Workers commit locally and push nothing.
- A worker stops only processes it started, by PID; never by name.
- Nothing a worker built is installed, deployed or merged before I review it; land one group at a time.
- Only I open pull requests. Push the branch if needed, then give me the compare link and a drafted title and body.

## Who does it
Propose the split (pieces, which are independent, what each returns) and wait for my OK before starting any worker.

— promptu delegate-agents@1 asked=split:rec,returns:rec,limits:rec,contract:rec,stop:rec,landing:rec,rules:rec
