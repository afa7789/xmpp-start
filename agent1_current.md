# Agent 1 - UI Developer

## Current Task: M6: Reaction hover bar

### Description
Hovering over a message reveals action bar with quick-react emoji (👍 ❤️ 😂) and reply button. Reaction counts shown as pills below bubble. XEP-0444.

### Implementation Plan
1. Add hover state to track which message ID is currently hovered
2. Add Message::SetHoveredMessage variant 
3. Add quick-react emoji buttons (👍 ❤️ 😂) that appear on hover alongside existing buttons (copy, reply, react, edit, retract)
4. The action bar should appear above/below the message when hovered

### Files to modify
- `src/ui/conversation.rs` - Add hover state + view updates

### Requirements
- When hovering over a message, show action bar with quick reactions
- Clicking quick reaction sends reaction to that message
- Keep existing reaction UI (pills below bubble)

### Verify
- Run `cargo test --lib` 
- Run `cargo clippy --all-targets`

### Complete
- Update TODO.md (uncheck M6, add checkmark with date)
- Update tasks/tasks.yaml (move from pending high to completed)
- Commit: `feat: M6 reaction hover bar`

---
## Queue (next tasks after M6)
1. **L2: @mention autocomplete** - In MUC, typing @ shows autocomplete dropdown of room occupants
2. **L3: Message moderation UI** - Moderators can retract any message in room. Show moderator tools on hover.
3. **M4: Audio recording (voice messages)** - Holding mic button records audio; releasing sends as OGG/Opus
4. **K3: Room invitations UI** - Send and receive room invitations
5. **K1: Room creation UI** - Full room creation flow with config