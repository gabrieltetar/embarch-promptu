---
name: draft-message
description: >-
  Draft text that goes out under my name: a Slack message, an email, a Linear
  issue or comment. Nothing is posted until I say "post". Not for a reply to a
  review comment on a pull request (address-pr-comments), nor for answering a
  pasted message from someone else (colleague-relay).
version: 1
kind: task
signals:
  phrases: ["\\bdraft\\b", "write (a|an|the) (message|comment|issue|reply|answer|email)", "create a linear issue", "message (to|for) ", "slack (message|dm)", "\\bemail\\b"]
  not: ["\\w+ (said|asked)\\s*:", "comments? (on|in) (the )?(pr|pull request|branch|feature/)", "\\bpr\\s*#?\\d+", "review comment"]
  desk: never
questions:
  - id: channel
    header: Channel
    ask: Where does it go?
    type: choice
    options:
      - label: Linear comment
        description: On an existing issue.
        value: "a Linear comment"
      - label: Linear issue
        description: A new issue.
        value: "a new Linear issue"
      - label: Slack
        description: A DM or a channel.
        value: "a Slack message"
      - label: Email
        description: An email I send.
        value: "an email"
    recommended: Linear comment
    required: true
    sources: [prompt, desk]
    gap: G12
    why: "Each channel has its own form; the voice skill keeps per-channel rules."
  - id: audience
    header: Reader
    ask: Who reads it, and what do they need from it?
    type: text
    suggest: 2
    fallback: not-sure
    required: true
    sources: [prompt, desk]
    gap: G9
    why: "Depth follows the reader's role: impact for a manager, nets for an electrical engineer, files for firmware."
  - id: lead
    header: Lead with
    ask: What must the reader take away first?
    type: text
    suggest: 2
    fallback: leave-out
    sources: [prompt]
    gap: G12
    why: "The opening line is what the reader takes away, and the part most often reworked."
  - id: tone
    header: Tone
    ask: How sure is the message?
    type: choice
    options:
      - label: A finding to confirm
        description: Humble; what it rests on.
        value: "Present it as a finding to confirm: what it is based on, and what was not checked or measured."
      - label: A decision
        description: Settled; say what changes.
        value: "Present it as a decision: what is decided and what changes."
      - label: A question
        description: I need their answer.
        value: "Present it as a question to them; end on the question."
    recommended: A finding to confirm
    sources: [prompt, last]
    gap: G12
    why: "When not everything was checked, a draft reads better as a finding to confirm than as a verdict."
  - id: tag
    header: Tag
    ask: Who should it tag?
    type: text
    suggest: 2
    fallback: leave-out
    options:
      - label: Nobody
        description: No mention.
        value: "nobody"
    sources: [prompt]
    gap: G9
    why: "Who to tag is the sender's call; the voice skill asks when it is not named."
examples:
  - rough: "draft a comment for ABC-58 that the fan connector footprint is mirrored on rev B"
    context: {item: {key: ABC-58, title: Enclosure fan header, url: "https://linear.app/example/issue/ABC-58"}}
    answers:
      channel: Linear comment
      audience: "The board designer; needs the net names and the sheet"
      lead: "The footprint is mirrored, so pin 1 lands on GND"
      tone: A finding to confirm
      tag: "the board designer"
---
## Goal
{{{ask}}}

## Context
It goes to {{channel}}{{#item.key}} on {{item.key}}{{/item.key}}. The reader: {{audience}}.
{{#lead}}
Open with: {{lead}}.
{{/lead}}
{{#tag}}
Tag {{tag}}.
{{/tag}}

## Done when
A draft file I can edit, with what you corrected in the fact check and what you could not check listed under it.

## Constraints
- {{tone}}
{{#config.voice_skill}}
- Write it with the {{config.voice_skill}} skill.
{{/config.voice_skill}}
- Nothing is posted until I say "post".
