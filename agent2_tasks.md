# Agent 2 - Engine Developer Tasks

Your priority order:

**Bugs to fix:**
1. **Auto-away not escalating to XA after 15 min** - File: `src/ui/mod.rs`
2. **Duplicate PaletteQuery arm in App::update** - File: `src/ui/mod.rs`

**Engine tasks:**
3. **K3: Room invitations engine** - Send and receive room invitations. Mediated (through room) and direct invites. XEP-0045 + XEP-0249.
4. **K1: Room creation engine** - Create rooms with MUC config form.
5. **J9: Account registration wizard** - In-band registration. Register new account from within app. XEP-0077.
6. **K7: Push notifications** - Server-push notifications when app is backgrounded. VAPID registration. XEP-0357.

Files to work with:
- `src/xmpp/engine.rs` - Main XMPP engine
- `src/xmpp/mod.rs` - XMPP module definitions
- `src/xmpp/modules/muc.rs` - MUC functionality
- `src/xmpp/modules/muc_config.rs` - Room config
- `src/xmpp/modules/account.rs` - Account handling
- `src/ui/mod.rs` - App state and update

Process:
1. Fix bugs first
2. Then pick highest engine task
3. Read relevant code to understand structure
4. Implement
5. Run `cargo test --lib` and `cargo clippy --all-targets`
6. Fix any errors
7. Update TODO.md (check off task)
8. Update tasks/tasks.yaml (move from pending to completed)
9. Commit with message: `fix: <task-id> <description>` for bugs, `feat: <task-id> <description>` for features
10. Move to next task

Start with the two bugs first!