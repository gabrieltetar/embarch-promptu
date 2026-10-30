---
name: colleague-relay
description: >-
  A colleague's message or question, pasted or quoted, with a short ask: explain
  it, answer it, fact-check it or do what they ask. Use when the prompt carries
  "X said:", "X asked:" or a pasted message from someone else, including a bare
  question pasted with no speaker named. For a message that starts from me, use
  draft-message.
version: 1
kind: task
signals:
  phrases: ["\\b\\w+ (said|asked|says|asks|wrote)\\s*:", "request from", "(he|she|they) (said|asked|want)", "question from"]
  not: []
  desk: never
questions:
  - id: deliverable
    header: Deliverable
    ask: What do you need from it?
    type: multi
    options:
      - label: Explain it to me
        description: What they mean and what is true.
        value: "Explain to me what they are asking and what the true answer is."
      - label: Draft a reply
        description: In my voice; I post it.
        value: "Draft my reply to them. Post nothing; I send it."
      - label: Do what they ask
        description: The fix, the file or the build they need.
        value: "Do what they ask for."
      - label: Fact-check it
        description: Is their claim right?
        value: "Check whether their claim is right, with the evidence."
    recommended: [Explain it to me]
    required: true
    sources: [prompt]
    gap: G9
    why: "A relayed message rarely says who the answer is for or what form it should take."
  - id: check
    header: Check with
    ask: What must claims be checked against?
    type: multi
    options:
      - label: The code
        description: The current source.
        value: "the current source code"
      - label: Hardware docs
        description: Schematics, datasheets, hardware notes.
        value: "the hardware documentation (schematics, datasheets and any hardware notes)"
      - label: A bench run
        description: Measure it, on my go.
        value: "a bench measurement, after my green light"
      - label: What we know is enough
        description: No extra checking.
        value: "what is already known in this session"
    recommended: [The code, Hardware docs]
    sources: [prompt, last]
    gap: G13
    why: "Device behaviour inferred from source code is not a fact until it is checked."
  - id: speed
    header: Speed
    ask: How fast does this need to go?
    type: choice
    options:
      - label: Thorough
        description: Test what needs testing.
        value: ""
      - label: Fast, no hardware test
        description: They need it now; it is simple.
        value: "They need this fast and it is simple: no hardware test, deliver as soon as it builds."
    recommended: Thorough
    sources: [prompt]
    gap: G2
    why: "A fast turnaround may waive a hardware test; saying so up front saves a bench session."
  - id: reply_to
    header: Reply where
    ask: Where does the reply go?
    type: choice
    when: "deliverable has 'Draft a reply'"
    options:
      - label: The thread it came from
        description: Same channel, same thread.
        value: "the thread the message came from"
      - label: A Linear comment
        description: On the related issue.
        value: "a comment on the related Linear issue"
      - label: A Slack message
        description: A DM or a channel.
        value: "a Slack message"
    recommended: The thread it came from
    sources: [prompt, last]
    gap: G12
    why: "Channel decides length and form; the voice skill has per-channel rules."
  - id: tag
    header: Tag
    ask: Who should the reply tag?
    type: text
    when: "deliverable has 'Draft a reply'"
    suggest: 2
    fallback: leave-out
    options:
      - label: Nobody
        description: No mention.
        value: "nobody"
    sources: [prompt]
    gap: G9
    why: "Who to tag is the sender's call; without it the draft has to ask."
examples:
  - rough: "The ME asked: can the enclosure fan run from the 5 V rail at full speed, or does it need its own supply? respond"
    context: {firmware: true}
    answers:
      deliverable: [Explain it to me, Draft a reply]
      check: [Hardware docs]
      speed: Thorough
      reply_to: The thread it came from
      tag: "the mechanical engineer who asked"
  - rough: "Hey, is the fan connector on these boards keyed, or can it go in backwards?"
    context: {firmware: true}
    answers:
      deliverable: [Explain it to me, Draft a reply]
      check: [Hardware docs]
      reply_to: The thread it came from
    note: Regression case. A pasted question with no speaker named is still someone else's message.
---
## Goal
{{{ask}}}

## Context
It carries a message from someone else, with my ask around it.

## Read first
{{#check}}
- Check claims against {{.}}.
{{/check}}

## Done when
{{#deliverable}}
- {{.}}
{{/deliverable}}
{{#reply_to}}
The reply goes to {{reply_to}}{{#tag}}, tagging {{tag}}{{/tag}}.
{{/reply_to}}

## Constraints
{{#speed}}
- {{speed}}
{{/speed}}
- State as fact only what you checked; mark the rest as a reading to confirm.
{{#reply_to}}
- Nothing is posted until I say "post".
{{#config.voice_skill}}
- Write the reply with the {{config.voice_skill}} skill.
{{/config.voice_skill}}
{{/reply_to}}
