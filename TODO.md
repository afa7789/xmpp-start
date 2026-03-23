# xmpp-start TODO

## Bugs
- [ ] **BUG-LOGIN-1**: On first login, app enters benchmark screen after ~5 seconds, then logs off and locks on first screen
  - Symptom: Login succeeds, shows "online as...", loads roster (1 contact), MAM catchup (50 messages), bookmarks (2), avatar received, then abruptly switches to Benchmark → back to Login
  - Log: 2026-03-23T02:26 - connects as abeilice@kosmos.org → roster loaded → MAM complete → bookmarks → avatars → **some event triggers benchmark screen**
  - Files to investigate: `src/ui/mod.rs` screen transitions, benchmark screen, event handling
  - Priority: CRITICAL — blocks basic login
- [x] Auto-away not escalating to XA after 15 min (`src/ui/mod.rs`) — FIXED
- [x] Duplicate `PaletteQuery` arm in `App::update` (`src/ui/mod.rs`) — FIXED

## High Priority
- [x] M6: Reaction hover bar
- [ ] L2: @mention autocomplete in MUC
- [ ] L3: Message moderation by moderators
- [ ] M4: Audio recording (voice messages)
- [ ] K3: Room invitations
- [ ] K1: Room creation UI

## Medium Priority
- [ ] K6: Chat preferences panel
- [ ] R2: Enhanced link previews
- [ ] H2: Own avatar upload
- [ ] K2: Browse public rooms
- [ ] J9: Account registration wizard
- [ ] K7: Push notifications

## Critical (architectural)
- [ ] OMEMO E2E encryption
- [ ] Multi-account support

## Recently Completed
- [x] M7: About modal (2026-03-22)
- [x] M1: System theme sync + time format (2026-03-22)
- [x] M2: Delivery/read status UI (2026-03-22)
- [x] M3: Emoji picker (2026-03-22)
- [x] S1: Auto-away (2026-03-22)
- [x] S6: Privacy panel (2026-03-22)
- [x] J10: MAM archiving preferences (2026-03-22)
- [x] S10: Data forms renderer (2026-03-22)
- [x] S3: MUC admin modules (2026-03-22)
