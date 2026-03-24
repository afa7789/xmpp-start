---
name: multi_agent_concurrent_work
description: Multiple agents work concurrently in the same repo — Agent A and Builder work simultaneously on different features
type: project
---

Multiple agents work concurrently in this xmpp-start repo. Agent A works on auth/UI scaffolding, Builder on feature implementations.

**Why:** Tasks are split across agents for parallel progress.

**How to apply:**
- When building, other agents' stashes may be present and get popped automatically
- The `src/ui/mod.rs` and `src/ui/settings.rs` are touched by both agents — check what Agent A has committed before writing to them
- Agent A often pre-writes module declarations (pub mod blocklist, etc.) in advance of Builder creating the files — check `git show HEAD:src/ui/mod.rs` before adding declarations
- Files may be reverted by linters between `git add` and `git commit` — run `git add && git commit` in a single compound command
- Stash pops from other agents can introduce build errors — run `git checkout -- src/ui/chat.rs src/ui/conversation.rs src/ui/sidebar.rs` to restore committed state if needed
