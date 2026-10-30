---
name: handoff
description: >-
  Have this session write a self-contained brief so another session, a
  background worker or a colleague can carry on. Use for "generate a
  handoff prompt", "a prompt for a new thread", "hand this to a worker".
  Not for running a brief that is already written: that takes the pattern of
  the work the brief describes.
version: 1
kind: task
signals:
  phrases: ["hand-?off", "prompt for (a |the |another )?(new )?(thread|worker|session|chat)", "generate a prompt", "fresh (chat|session|thread)", "new thread"]
  not: []
  desk: never
questions:
  - id: reader
    header: Picked up by
    ask: Who picks the work up?
    type: choice
    options:
      - label: A fresh chat, this item
        description: Me, in a new chat on the same work item.
        value: "a fresh chat on the same work item, driven by me"
      - label: A background worker
        description: Its own branch, unattended.
        value: "a background worker on its own branch, unattended"
      - label: A colleague
        description: A person, not an agent.
        value: "a colleague (a person, not an agent)"
    recommended: A fresh chat, this item
    required: true
    sources: [prompt]
    gap: G4
    why: "Who picks the work up decides how much the brief must explain."
  - id: their_done
    header: Their done
    ask: When is the reader done?
    type: choice
    options:
      - label: Same done as ours
        description: The definition this session works to.
        value: "the same definition of done this session works to, with its check command"
      - label: A plan, then my review
        description: They stop after planning.
        value: "a written plan, then stop for my review"
      - label: Review loop is clean
        description: Built, then reviewed to no high or medium.
        value: "built, then an independent review loop until no high or medium finding is left"
    recommended: Same done as ours
    required: true
    sources: [prompt, last]
    gap: G10
    why: "A brief without a definition of done lets a fresh session declare victory early."
  - id: scope
    header: Their scope
    ask: Which part of the work goes over?
    type: text
    suggest: 2
    fallback: leave-out
    options:
      - label: Everything still open
        description: All of it.
        value: "everything still open"
    sources: [prompt]
    gap: G11
    why: "Splitting work between threads needs the boundary named, or both threads do it."
  - id: autonomy
    header: Their forks
    ask: How does the reader handle design forks?
    type: choice
    when: "reader == 'A background worker'"
    options:
      - label: Scoped, take the rec
        description: Inside scope, proceed on the recommendation.
        value: "The work is fully scoped: inside the scope, take your recommendation at each fork and list it in the report; stop for anything outside the scope."
      - label: Stop at each fork
        description: Ask me every time.
        value: "Stop at each design fork and ask me, with your recommendation."
    recommended: "Scoped, take the rec"
    sources: [prompt, last]
    gap: G4
    why: "Interactive work waits for the user's pick at a design fork; fully scoped background work can proceed on the recommendation."
  - id: branch
    header: Branch
    ask: Where does the reader work?
    type: choice
    when: "reader != 'A colleague'"
    options:
      - label: This branch
        description: Same checkout.
        value: "this branch{{#branch}}, {{branch}}{{/branch}}"
      - label: New worktree off this
        description: Its own checkout.
        value: "a new worktree branched off this branch"
      - label: Their choice
        description: Let the brief say what fits.
        value: ""
    recommended: New worktree off this
    sources: [prompt, context]
    gap: G6
    why: "A brief without its branch and checkout leaves the reader to guess where to work."
  - id: save_to
    header: Save to
    ask: Where does the brief go?
    type: choice
    options:
      - label: A plans file
        description: A markdown file plus a one-line start command.
        value: "Save it as a markdown file in the plans folder and give me the one-line prompt that starts it."
      - label: A note on the item
        description: A handoff note on the desk work item.
        value: "Add it to the work item as a handoff note (desk_add_line, kind handoff)."
        requires: item
      - label: In the chat
        description: Nothing written to disk.
        value: "Put it in the chat only."
    recommended: A plans file
    sources: [prompt, last]
    gap: G12
    why: "A brief saved to a file can be read later by path, from any session."
examples:
  - rough: "generate a handoff prompt for a new thread to finish the OTA rollback work"
    context: {branch: feat/ota-rollback}
    answers:
      reader: A fresh chat, this item
      their_done: Same done as ours
      scope: Everything still open
      branch: This branch
      save_to: A plans file
---
## Goal
{{{ask}}}

Write a self-contained handoff brief for {{reader}}. They have not seen this conversation: write for a reader who did not watch, with no working shorthand, and quote my decisions in my own words.

## Context
The brief has these sections, each only if it has content:
1. Context: the goal and why, the state now, and what is done, each with its evidence (commits, commands and their output).
2. Read first, in order: files, issues and notes, by path.
3. Already decided: my words quoted exactly. Also what was tried and ruled out, and why.
4. Hard rules: the rules from this session and its memory that the reader would not otherwise see.
5. Work, in priority order: the first step checks the starting point (directory, git log, a quick build) before anything new.
6. Needs the owner: ask, don't guess.
7. Definition of done: the check and its exact command.
{{#scope}}

Hand over: {{scope}}.
{{/scope}}

## Done when
Their definition of done: {{their_done}}.
{{save_to}}

## Constraints
{{#branch}}
- They work on {{branch}}.
{{/branch}}
{{#autonomy}}
- Tell them: {{autonomy}}
{{/autonomy}}

## Who does it
Write the brief yourself, now, from this session's full context; don't start the work.
