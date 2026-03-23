# Agent 1 - UI Developer Tasks

Your priority order:
1. **M6: Reaction hover bar** - Hovering over a message reveals action bar with quick-react emoji (👍 ❤️ 😂) and reply button. Reaction counts shown as pills below bubble. XEP-0444.
2. **L2: @mention autocomplete** - In MUC, typing @ shows autocomplete dropdown of room occupants. Highlighted messages when mentioned. XEP-0372.
3. **L3: Message moderation UI** - Moderators can retract any message in room. Show moderator tools on hover. XEP-0425.
4. **M4: Audio recording (voice messages)** - Holding mic button records audio; releasing sends as OGG/Opus via HTTP Upload. Show waveform/duration preview while recording.
5. **K3: Room invitations UI** - Send and receive room invitations. Mediated (through room) and direct invites.
6. **K1: Room creation UI** - Full room creation flow: room name, MUC service selection, initial config (public/private, persistent, members-only), instant join after creation.

Files to work with:
- `src/ui/conversation.rs` - Main chat UI
- `src/ui/mod.rs` - App state and update
- `src/ui/settings.rs` - Settings panel
- `src/xmpp/modules/muc.rs` - MUC functionality
- `src/xmpp/modules/muc_config.rs` - Room configuration

Process:
1. Pick highest priority task
2. Read relevant code to understand structure
3. Implement
4. Run `cargo test --lib` and `cargo clippy --all-targets`
5. Fix any errors
6. Update TODO.md (check off task)
7. Update tasks/tasks.yaml (move from pending to completed)
8. Commit with message: `feat: <task-id> <description>`
9. Move to next task

Start with M6: Reaction hover bar