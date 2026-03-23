# xmpp-start Orchestration TODO

## Completed
- ✅ **B1**: rustls CryptoProvider — wired in main.rs
- ✅ **B2**: i18n module — stub exists as src/i18n/mod.rs
- ✅ **B3+A4**: Presence indicator — sidebar.rs shows ●/○, mod.rs calls on_presence
- ✅ **A1**: SQLite DB at startup — wired in main.rs + App struct
- ✅ **A2**: Persist incoming messages to SQLite (2026-03-22)
- ✅ **A3**: Persist roster to SQLite on RosterReceived (2026-03-22)
- ✅ **A5**: Notifications — wired in ui/mod.rs
- ✅ **C6**: XEP-0280 Carbons — incoming stanza wiring (2026-03-22)
- ✅ **E3**: Emoji reactions (XEP-0444)
- ✅ **E5**: Link previews (2026-03-22)
- ✅ **F1**: XMPP debug console
- ✅ **F2**: Command palette (Cmd+K)
- ✅ **G6**: Draft auto-save per conversation
- ✅ **J2**: Custom status message
- ✅ **J4**: Sound notifications

## Phase B — Storage Layer
- [x] ✅ **B4**: Load message history on conversation open (50 most recent) — (2026-03-22)
- [x] ✅ **B5**: Unread badge count in sidebar — (2026-03-22)
- [x] ✅ **B6** — (2026-03-22)

## Phase C — XMPP Engine Wiring
- [x] ✅ **C1**: Wire StreamMgmt into engine loop (already wired in engine.rs)
- [x] ✅ **C2**: Wire PresenceMachine into engine + SetPresence command + UI picker (2026-03-22)
- [x] ✅ **C3**: MAM post-connect history sync (2026-03-22) — catchup query on Online, MAM result/fin handling, CatchupFinished event
- [x] ✅ **C4**: Wire BlockingManager into engine (2026-03-22) — fetch on Online, result/push handling, message filter
- [x] ✅ **C5**: Wire DiscoManager caps into presence (2026-03-22) — caps in presence, disco#info get response added

## Phase D — UI Panels
- [ ] **D1**: Render OccupantPanel in MUC conversations
- [x] ✅ **D2** — (2026-03-22)
- [ ] **D3**: MUC join/leave UI flow
- [ ] **D4**: Bookmarks autojoin on connect

## Phase E — Rich Features
- [x] ✅ **E1**: Message corrections (XEP-0308) (2026-03-22) — SendCorrection command wired in engine, make_correction_message builder
- [x] ✅ **E2**: Message retractions (XEP-0424) (2026-03-22) — SendRetraction command wired in engine, make_retraction_message builder
- [ ] **E4**: File upload (XEP-0363)

## Phase F — Polish
- [x] ✅ **F3**: Settings panel (font size, timestamps, theme toggle) — already implemented (2026-03-22)
- [ ] **F4**: Reconnect logic with backoff
- [ ] **F5**: Avatar fetching (XEP-0084 + vCard fallback)

## Phase G — Conversation UX
- [x] ✅ **G1**: Close/remove conversation — (2026-03-22)
- [x] ✅ **G2**: Typing indicators (XEP-0085) — (2026-03-22)
- [x] ✅ **G3**: Message replies (XEP-0461) — already implemented (2026-03-22)
- [x] ✅ **G4**: /me action messages (XEP-0245) — already implemented (2026-03-22)
- [x] ✅ **G5** — (2026-03-22)
- [x] ✅ **G7**: Copy message to clipboard — (2026-03-22)
- [ ] **G8**: MAM lazy-load (scroll up for older history)
- [x] ✅ **G9**: Message search within conversation — (2026-03-22)

## Phase H — Avatars & Contact Management
- [ ] **H1**: Show user avatars (XEP-0084 + XEP-0153)
- [ ] **H2**: Own avatar upload (XEP-0084)
- [ ] **H3**: Add/remove/rename contacts
- [ ] **H4**: Contact profile popover (vCard)
- [x] ✅ **H5**: Consistent avatar colors (XEP-0392) — already implemented (2026-03-22)

## Phase I — File & Media
- [ ] **I1**: Paste image from clipboard
- [ ] **I2**: Drag & drop files onto composer
- [ ] **I3**: File picker + multiple attachments + upload progress
- [ ] **I4**: Attachment preview in received messages

---
## Orchestration Notes
- NO worktree isolation — agents work directly in main repo on non-overlapping files
- Agent A (Storage/UI): B4 → B5 → B6 → D2 → G-phase (touches ui/, store/)
- Agent B (Engine/XMPP): C1 → C2 → C3 → C4 → C5 → D3 (touches xmpp/engine.rs)
- Always run `cargo test && cargo clippy` before marking complete
- Commit after each completed task, never push

## Known Bugs (fix before release)
- [ ] **BUG-1**: MAM historical messages trigger desktop notifications + sounds — `IncomingMessage` needs `is_historical: bool` flag; set `true` for MAM-sourced messages in engine.rs; skip notifications/sound in ui/mod.rs when `is_historical`
- [ ] **BUG-2**: Finder/permission modals open on connect — notify-rust fires for every MAM message on startup; root cause is BUG-1
- [x] ✅ **BUG-3**: MAM `fetched` count fixed — now uses `mam_result.messages.len()` instead of `rsm.count` (2026-03-22)
