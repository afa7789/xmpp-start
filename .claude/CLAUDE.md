# Claude Code Agent Instructions for xmpp-start

## Task Management

### YAML Task Registry
All tasks are tracked in `tasks/tasks.yaml`. This is the **single source of truth** for automation and parsing.

**Rule: After completing any task, update `tasks/tasks.yaml`:**
1. Move task from `pending` to `completed` section
2. Add completion date: `completed: "YYYY-MM-DD"`
3. Add commit hash if applicable

### Task Categories (from YAML)
- **critical**: Must have before first release (OMEMO, Room creation, Multi-account)
- **high**: Core functionality (Audio, Reactions, @mentions, Moderation)
- **medium**: Important but not blocking (Avatar upload, Link previews, Preferences)
- **low**: Nice to have (About modal, Stickers, Location)

## Development Rules

### Before Committing
1. Run `cargo test --lib` — all 286 tests must pass
2. Run `cargo clippy --all-targets` — no errors allowed
3. Fix all warnings before committing

### Commit Format
```
feat: <task-id> <short description>
feat: M7 About modal
feat: S10 XEP-0004 data forms renderer
```

### File Organization
- UI modules: `src/ui/<feature>.rs`
- Engine modules: `src/xmpp/modules/<feature>.rs`
- Tests: `tests/critical_flows.rs`

## Pending High-Priority Tasks

### High Priority (ordered by complexity)
1. **M6**: Reaction hover bar (hover shows quick-react emojis)
2. **L2**: @mention autocomplete in MUC
3. **L3**: Message moderation by moderators
4. **M4**: Audio recording (voice messages)
5. **K3**: Room invitations (XEP-0249)
6. **K1**: Room creation UI + config modal

### Medium Priority
1. **M1**: System theme sync + 12h/24h toggle
2. **K6**: Chat preferences panel
3. **R2**: Enhanced link previews
4. **H2**: Own avatar upload (XEP-0084 PEP)
5. **K2**: Browse public rooms
6. **J9**: Account registration wizard (XEP-0077)
7. **K7**: Push notifications (XEP-0357)

### Critical (architectural)
1. **MEMO**: OMEMO E2E encryption (XEP-0384) — ~2-3 weeks
2. **MULTI**: Multi-account support — ~1-2 weeks

## XEP Status Reference

See `tasks/tasks.yaml` → `implemented_xeps` for full list.

Core XEPs implemented: XEP-0030, 0115, 0153, 0191, 0198, 0245, 0280, 0313, 0363
Chat XEPs: XEP-0079, 0085, 0184, 0308, 0333, 0393, 0424, 0444, 0461
MUC: XEP-0045 (partial), 0048
Presence: XEP-0178, 0252
Forms: XEP-0004 (renderer)

## Common Commands
```bash
make test       # cargo test --lib
make lint       # cargo clippy --all-targets
make run        # cargo run
```

## Rules
- Update the Yaml, and describe better the roadmap only in the markdown, so we keep track of the tasks in yaml, but have descriptions more data somewhere else so it is easier to track/parse read,update.

## Rules
- Update the Yaml, and describe better the roadmap only in the markdown, so we keep track of the tasks in yaml, but have descriptions more data somewhere else so it is easier to track/parse read,update.
