# Agent 2 - Engine Developer

## Current Task: Fix bugs first

### Bug 1: Duplicate PaletteQuery arm in App::update
- File: `src/ui/mod.rs` 
- The `Message::PaletteQuery(String)` is matched twice:
  - Lines 176-179: First match sets self.palette_query
  - Lines 292-295: Duplicate match also sets self.palette_query
- **Fix**: Remove the duplicate (lines 292-295)

### Bug 2: Auto-away not escalating to XA after 15 min
- File: `src/ui/mod.rs`
- Looking at lines 259-290, the IdleTick handler:
  - Active -> AutoAway after 5 min (300s)
  - Active -> AutoXa after 15 min (900s)
  - But there's no transition from AutoAway -> AutoXa!
- **Fix**: Add case for IdleState::AutoAway that escalates to AutoXa after extended idle time

### Complete each bug:
- Fix the code
- Run `cargo test --lib` and `cargo clippy --all-targets`
- Update TODO.md (uncheck bugs, add checkmark with date)  
- Update tasks/tasks.yaml (bugs aren't in tasks.yaml)
- Commit each fix: `fix: <description>`

---
## Queue (after bugs)
1. **K3: Room invitations engine** - Send and receive room invitations
2. **K1: Room creation engine** - Create rooms with MUC config form
3. **J9: Account registration wizard** - In-band registration
4. **K7: Push notifications** - Server-push notifications