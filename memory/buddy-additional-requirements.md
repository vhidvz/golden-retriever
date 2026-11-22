---
name: buddy-additional-requirements
description: "Standing requirements from Buddy's second requirements document: proactive help from habits, learned workflows, feedback, eye contact, Ollama as an optional peer provider"
metadata:
  node_type: memory
  type: project
  originSessionId: 2544f5d0-0fc6-455b-8724-10d57b4433cb
  modified: 2026-11-22T18:45:01.101Z
---

A second requirements document given 2026-11-12, during Phase 2. It extends, and does not
replace, [[buddy-product-decisions]]. It adds:

1. **Proactive assistance from behavioural patterns:** notice recurring workflows (apps,
   menus, click or shortcut sequences, file operations) and offer help.
2. **Learned workflows:** capture context, preconditions, intended outcome, action
   sequence and variations; prefer accessibility APIs and structured tools over
   coordinate replay; verify outcomes.
3. **Playful invitation, not a notification:** excitement, bouncing, wagging, looking at
   the user, short dog sounds. Accept, decline or ignore, and learn from refusals. Must
   work with no cloud AI.
4. **Feedback with distinct consequences:** correction, disabling help per context or
   task, approval, persistent preference, positive reinforcement. Temporary versus
   permanent and one-time versus ongoing must be distinct.
5. **Error recovery and control:** confidence, confirmation before consequential actions,
   checkpoints, verification, history, rollback, immediate pause/cancel. Feedback must
   never silently widen permissions.
6. **Positive reinforcement:** petting, virtual treats, praise, play affect emotion,
   preferences and learned associations, never verification or permissions.
7. **Natural eye contact** towards the person in front of the monitor without a webcam;
   occasional, varied, never constant staring; an independent animation/personality layer.
8. **Ollama as a first-class local provider**, per-capability model choice, never a
   dependency of the autonomous engine.
9. **First model to evaluate: `gemma4:e4b`.** Measure CPU/GPU/RAM/VRAM, latency, Persian
   and English quality, tool calling, image understanding, and whether audio input works
   through Ollama.
10. **Recommend other models for this hardware**, possibly per capability.
11. **Ollama and Claude Code are peers.** Choose by task, availability, quota, privacy,
    latency and preference.
12. **Priority rule:** finish and verify already-approved work first; these additions come
    after it.

Eye contact was folded into Phase 2 because it lives in the animation layer. The "no
human voice" decision still applies: invitations and replies are dog behaviour only.

The full text and implementation status are recorded in the repository at
`docs/decisions.md`.

**Why:** it widens Phases 3-5: perception must feed pattern discovery, skills need a
workflow recorder and verifier, and the provider layer must treat Ollama and Claude Code
as peers.

**How to apply:** when planning Phase 3 onward, include pattern recognition, workflow
learning, the invitation behaviour and the feedback vocabulary. Related:
[[buddy-product-decisions]].
