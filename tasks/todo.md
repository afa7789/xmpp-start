# xmpp-start Orchestration TODO

## Completed
- ✅ **B1**: rustls CryptoProvider — wired in main.rs
- ✅ **B2**: i18n module — stub exists as src/i18n/mod.rs
- ✅ **B3+A4**: Presence indicator — sidebar.rs shows ●/○, mod.rs calls on_presence
- ✅ **A1**: SQLite DB at startup — wired in main.rs + App struct
- ✅ **A5**: Notifications — wired in ui/mod.rs
- ✅ **E5**: Link previews — implemented (2026-03-22)
- ✅ **J2**: Custom status message
- ✅ **J4**: Sound notifications
- ✅ **E3**: Emoji reactions (XEP-0444)
- ✅ **F1**: XMPP debug console
- ✅ **F2**: Command palette (Cmd+K)

## In Progress (Agent Track)
- [ ] **A2**: Persist incoming messages to SQLite — engine.rs has partial C6 changes staged
- [ ] **A3**: Persist roster to SQLite on RosterReceived
- [ ] **C6**: XEP-0280 Carbons — partial impl in engine.rs (uncommitted), need to complete + commit

## Phase B — Storage Layer (next priority, depends on A2)
- [ ] **B4**: Load message history on conversation open (50 most recent)
- [ ] **B5**: Unread badge count in sidebar
- [ ] **B6**: Mark conversation read, persist last_read_id

## Phase C — XMPP Engine Wiring (parallel with B)
- [ ] **C1**: Wire StreamMgmt into engine loop
- [ ] **C2**: Wire PresenceMachine into engine
- [ ] **C3**: MAM post-connect history sync (depends on A1, A2)
- [ ] **C4**: Wire BlockingManager into engine
- [ ] **C5**: Wire DiscoManager / caps into presence

## Phase D — UI Panels
- [ ] **D1**: Render OccupantPanel in MUC conversations
- [ ] **D2**: XEP-0393 message styling (bold/italic/code in ConversationView)
- [ ] **D3**: MUC join/leave UI flow
- [ ] **D4**: Bookmarks autojoin on connect

## Phase E — Rich Features
- [ ] **E1**: Message corrections (XEP-0308)
- [ ] **E2**: Message retractions (XEP-0424)
- [ ] **E4**: File upload (XEP-0363)

## Phase F — Polish
- [ ] **F3**: Settings panel (font size, timestamps, theme toggle)
- [ ] **F4**: Reconnect logic with backoff
- [ ] **F5**: Avatar fetching (XEP-0084 + vCard fallback)

## Phase G — Conversation UX
- [ ] **G1**: Close/remove conversation
- [ ] **G2**: Typing indicators (XEP-0085)
- [ ] **G3**: Message replies (XEP-0461)
- [ ] **G4**: /me action messages (XEP-0245)
- [ ] **G5**: Message grouping + date separators
- [ ] **G6**: Draft auto-save per conversation
- [ ] **G7**: Copy message to clipboard
- [ ] **G8**: MAM lazy-load (scroll up for older history)
- [ ] **G9**: Message search within conversation

## Phase H — Avatars & Contact Management
- [ ] **H1**: Show user avatars (XEP-0084 + XEP-0153)
- [ ] **H2**: Own avatar upload (XEP-0084)
- [ ] **H3**: Add/remove/rename contacts
- [ ] **H4**: Contact profile popover (vCard)
- [ ] **H5**: Consistent avatar colors (XEP-0392)

## Phase I — File & Media
- [ ] **I1**: Paste image from clipboard
- [ ] **I2**: Drag & drop files onto composer
- [ ] **I3**: File picker + multiple attachments + upload progress
- [ ] **I4**: Attachment preview in received messages

---
## Orchestration Notes
- Agent A (Storage/UI): A2 → A3 → B4 → B5 → B6 → D2 → G-phase
- Agent B (Engine/XMPP): C6 finish → C1 → C2 → C3 → C4 → C5 → D3
- Always run `cargo test && cargo clippy` before marking complete
- Commit after each completed task, never push
