---
name: lint-script
description: Run the lint script before every commit
metadata:
  node_type: memory
  kind: rule
  source: tools/lint.sh
  type: feedback
  originSessionId: 00000000-0000-4000-8000-000000000001
  modified: 2026-10-20T15:00:00.000Z
---

Run the lint script before every commit; it also checks license headers. (This
file has the shape Claude Code leaves after a Write: every key but name and
description moved under metadata, and its own three fields stamped.)
