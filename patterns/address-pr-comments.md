---
name: address-pr-comments
description: >-
  Work through comments human reviewers left on a pull request, from a whole
  review round down to one comment: change the code where needed, and draft
  (never post) the replies. Use for "address the comments", "peer review
  round", "git switch to X and address all comments", "draft a reply to X's
  comment on the PR". Not for a message outside a pull request (draft-message).
version: 1
kind: task
signals:
  phrases: ["address (all )?(the )?(new )?comments", "review comments", "peer review round", "comments? on (the )?(pr|pull request)", "(reply|answer|respond) (to|for) .*comment", "(draft|write) (a |an )?(reply|answer) .*comment", "comment (on|in) (the )?(pr|pull request|branch|feature/)", "\\bpr\\s*#?\\d+", "git switch"]
  not: ["review the (current )?branch", "reviewer loop"]
  desk: possible
include:
  - fragment: house-rules
    recommended: [Only I open PRs, Use another checkout]
questions:
  - id: pr
    header: Which PR
    ask: Which pull request?
    type: text
    suggest: 3
    fallback: not-sure
    required: true
    sources: [prompt, desk, context]
    gap: G6
    why: "Naming the pull request removes a guess about which one is meant."
  - id: replies
    header: Replies
    ask: What happens to replies to the reviewers?
    type: choice
    options:
      - label: Draft them, I post
        description: In my voice; nothing is posted.
        value: "Draft a reply to each comment in my voice and post nothing: I post them myself."
      - label: No replies needed
        description: Code changes only.
        value: "No replies: change the code only."
    recommended: Draft them, I post
    required: true
    sources: [prompt, memory, last]
    gap: G3
    why: "Whether replies are posted or only drafted is easy to get wrong, so it is restated at the end of the prompt."
  - id: hw_test
    header: Hardware
    ask: Do these changes need a hardware test?
    type: choice
    when: firmware
    options:
      - label: No hardware test
        description: Review changes are small.
        value: "No hardware test for these changes."
      - label: Boot check only
        description: Flash and confirm it boots.
        value: "Flash it and check that it boots; nothing more on the bench."
      - label: Targeted test
        description: One test for a risky change, on my go.
        value: "Plan one targeted bench test for the riskiest change and wait for my green light."
    recommended: No hardware test
    sources: [prompt, last]
    gap: G2
    why: "Review changes are usually small; a hardware test nobody asked for costs bench time."
  - id: which
    header: Which ones
    ask: Which comments?
    type: choice
    options:
      - label: All unresolved
        description: Every open thread on the PR.
        value: "every unresolved comment thread on the PR"
      - label: One reviewer's
        description: Name the reviewer in Other.
        value: "only the comments of the reviewer I name"
      - label: The latest round
        description: Comments since my last push.
        value: "only the comments left since my last push"
      - label: One comment
        description: The single comment the ask points to.
        value: "only the one comment named in the ask"
    recommended: All unresolved
    sources: [prompt]
    gap: G11
    why: "\"All the comments\" can be read too narrowly or too widely; naming the set removes the guess."
  - id: plan
    header: Before edit
    ask: Plan first, or change the code directly?
    type: choice
    options:
      - label: Plan, wait for my OK
        description: One line per comment first.
        value: "Plan first: one line per comment saying what you will change, or why not, and wait for my OK."
      - label: Fix directly
        description: Bring me only design forks.
        value: "Change the code directly; bring me only the design forks, with your recommendation, and wait for my pick."
    recommended: Plan, wait for my OK
    sources: [prompt, last]
    gap: G4
    why: "A one-line plan per comment lets the reviewer's points be checked before any code changes."
  - id: commits
    header: Commits
    ask: How are the changes committed?
    type: choice
    when: "which != 'One comment'"
    options:
      - label: One commit per round
        description: Named after the review round.
        value: "Put all the changes for this review round in one commit, named after the round."
      - label: One per comment
        description: Easy to map back to threads.
        value: "Make one commit per comment thread."
      - label: Don't commit
        description: Leave the changes in the tree.
        value: "Don't commit; leave the changes in the working tree."
    recommended: One commit per round
    sources: [prompt, last]
    gap: G3
    why: "A predictable commit layout keeps the pull request easy to review."
examples:
  - rough: "Let's address the comments on PR 12"
    context: {branch: feat/fan-pwm, firmware: true}
    answers:
      pr: "PR #12 \"Drive the enclosure fan with PWM\""
      replies: Draft them, I post
      hw_test: No hardware test
      which: All unresolved
      plan: Plan, wait for my OK
      commits: One commit per round
      rules: [Only I open PRs, Use another checkout]
  - rough: "Can you draft a reply for Sam's latest comment on feature/fan-pwm (main repo is in use)"
    context: {branch: feature/fan-pwm, firmware: true}
    answers:
      pr: "The open PR for feature/fan-pwm"
      replies: Draft them, I post
      hw_test: No hardware test
      which: One comment
      plan: Plan, wait for my OK
      rules: [Use another checkout, Only I open PRs]
    note: Regression case. A reply to one review comment is this pattern, not draft-message.
  - rough: "Let's look at the latest comment from Lee on PR 7 and draft an answer for him"
    context: {branch: main}
    answers:
      pr: "PR #7"
      replies: Draft them, I post
      which: One comment
      plan: Plan, wait for my OK
      rules: [Only I open PRs]
    note: Regression case, a second wording. Typed with no checkout of the PR yet. One comment, so no commit instruction.
---
## Goal
{{{ask}}}

## Context
Pull request: {{pr}}.
{{#which}}
Address {{which}}.
{{/which}}

## Done when
Every comment in scope is answered by a change or by a drafted reply saying why not.
{{#hw_test}}
Hardware: {{hw_test}}
{{/hw_test}}

## Constraints
{{#commits}}
- {{commits}}
{{/commits}}
{{> house-rules}}
- {{replies}}
{{#config.voice_skill}}
- Write anything that goes out under my name with the {{config.voice_skill}} skill.
{{/config.voice_skill}}

## Who does it
{{plan}}
