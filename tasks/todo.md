# xmpp-start Orchestration TODO

## Wave 5 (2026-03-24) — OMEMO + Multi-account

### Completed in Wave 5
- ✅ **OMEMO-foundation**: OmemoStore, DeviceManager, OmemoSessionManager, OmemoBundle helpers, message.rs stanza builder/parser, omemo_trust.rs UI — 2026-03-24
- ✅ **OMEMO-engine**: engine wiring for OmemoEnable, OmemoEncryptMessage, OmemoTrustDevice, omemo_try_decrypt, handle_client_event plumbed with omemo_mgr — 2026-03-24
- ✅ **MULTI-foundation**: MultiEngineManager, AccountState, AccountStateManager, account switcher UI (account_switcher.rs, account_state.rs), sidebar indicator bar — 2026-03-24
- ✅ **DC-6**: MUC admin + voice command wiring finalized (Kick/Ban/Affiliation/Voice enum + engine handler signature alignment) — 2026-03-24

### Remaining Work (not yet complete)

#### OMEMO
- [ ] PEP bundle auto-publish on XMPP connect (currently only published on explicit OmemoEnable command)
- [ ] Key exchange flow: fetch peer device list + bundles, create outbound Olm sessions automatically before first encrypted message
- [ ] UI: show OMEMO lock icon on encrypted messages, show trust fingerprint dialog
- [ ] E2E test: enable OMEMO, exchange messages between two accounts in Docker

#### Multi-account
- [ ] Replace single `xmpp_tx: mpsc::Sender<XmppCommand>` in App with `MultiEngineManager`
- [ ] Route all `XmppEvent` to correct per-account `AccountState` (messages, roster, presence, avatars)
- [ ] Full account switcher: switch conversation lists + sidebars per account
- [ ] Store XMPP credentials per-account in OS keychain
- [ ] E2E test: add second account, switch between them, verify independent events

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
- ✅ **K1** (PLAN.md): Room creation UI + config modal (2026-03-23)
- ✅ **L2** (PLAN.md): @mention autocomplete in MUC (2026-03-23) — dropdown above composer, amber highlight for mentioned messages

- ✅ **BUG-5**: Fix duplicate PaletteQuery match arm (2026-03-23)
- ✅ **BUG-4**: Auto-away escalation to XA (2026-03-23)
- ✅ **BUG-6**: Voice message composer fix (2026-03-23)
- ✅ **AUTH-1**: Auto-login / Remember me + auto-connect (2026-03-23)
- ✅ **AUTH-2**: Logout button in settings (2026-03-23)
- ✅ **M7**: About modal (2026-03-23)
- ✅ **M3**: Blocklist search + add JID UI (2026-03-23)
- ✅ **M4**: Account details panel (2026-03-23)
- ✅ **M6**: Data & storage settings (2026-03-23)
- ✅ **K6**: Chat preferences panel (2026-03-23)
- ✅ **M1**: System theme sync + 12h/24h (2026-03-23)
- ✅ **R1**: Reaction tooltips + quick emoji bar + toggle on re-click (2026-03-23)
- ✅ **R2**: Enhanced link previews + OGP image dimensions (2026-03-23)
- ✅ **R3**: Composer markdown shortcuts + paste image (2026-03-23)
- ✅ **K2**: Browse public rooms / vCard editing (2026-03-23)
- ✅ **K3**: Room invitations (XEP-0249) (2026-03-23)
- ✅ **L4**: Ad-hoc commands UI (XEP-0050) (2026-03-23)
- ✅ **H2**: Own avatar upload (XEP-0084) (2026-03-23)
- ✅ **F5**: Avatar fetching (XEP-0084 + vCard fallback) (2026-03-23)
- ✅ **J9**: Account registration wizard (XEP-0077) (2026-03-23)

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
- [x] ✅ **D1**: Render OccupantPanel in MUC conversations — (2026-03-22) muc_panels + muc_jids + muc_occupants in ChatScreen, row![sidebar, main_area, panel] when active JID is a MUC room
- [x] ✅ **D2** — (2026-03-22)
- [x] ✅ **D3**: MUC join/leave UI flow — (2026-03-22) JoinRoom/LeaveRoom commands, MucManager wired into engine, join-room input row in sidebar
- [x] ✅ **D4**: Bookmarks autojoin on connect — (2026-03-22) private XML get on Online, IQ result parse via BookmarkManager, BookmarksReceived event, JoinRoom commands for autojoin rooms

## Phase E — Rich Features
- [x] ✅ **E1**: Message corrections (XEP-0308) — (2026-03-22) UI: edit button, edit-mode strip, apply_correction, SendCorrection XmppCommand
- [x] ✅ **E2**: Message retractions (XEP-0424) — (2026-03-22) UI: retract button, tombstone rendering, apply_retraction, SendRetraction XmppCommand
- [x] ✅ **E4**: File upload UI (XEP-0363) — (2026-03-22) paperclip button, rfd file picker, pending attachments strip with progress, RequestUploadSlot command, HTTP PUT + send get_url on UploadSlotReceived

## Phase F — Polish
- [x] ✅ **F3**: Settings panel (font size, timestamps, theme toggle) — already implemented (2026-03-22)
- [x] ✅ **F4**: Reconnect logic with backoff — (2026-03-22) reconnect_attempt state, 2^n backoff capped at 64s, banner overlay in view()
- [x] ✅ **F5**: Avatar fetching (XEP-0084 + vCard fallback) (2026-03-23)

## Phase G — Conversation UX
- [x] ✅ **G1**: Close/remove conversation — (2026-03-22)
- [x] ✅ **G2**: Typing indicators (XEP-0085) — (2026-03-22)
- [x] ✅ **G3**: Message replies (XEP-0461) — already implemented (2026-03-22)
- [x] ✅ **G4**: /me action messages (XEP-0245) — already implemented (2026-03-22)
- [x] ✅ **G5** — (2026-03-22)
- [x] ✅ **G7**: Copy message to clipboard — (2026-03-22)
- [x] ✅ **G8**: MAM lazy-load (scroll up for older history) — (2026-03-22)
- [x] ✅ **G9**: Message search within conversation — (2026-03-22)

## Phase H — Avatars & Contact Management
- [x] ✅ **H1**: Show user avatars (XEP-0084 + XEP-0153) — (2026-03-22) avatar_cache in ChatScreen, on_avatar_received, FetchAvatar on RosterReceived, PNG handle in conversation view with initials fallback
- [x] ✅ **H2**: Own avatar upload (XEP-0084) (2026-03-23)
- [x] ✅ **H3**: Add/remove/rename contacts — (2026-03-22)
- [x] ✅ **H4**: Contact profile popover (vCard) — (2026-03-22)
- [x] ✅ **H5**: Consistent avatar colors (XEP-0392) — already implemented (2026-03-22)

## Phase I — File & Media
- [x] ✅ **I1**: Paste image from clipboard — (2026-03-22) Cmd+V subscription, arboard clipboard read, PNG encode via image crate, staged as temp file attachment
- [x] ✅ **I2**: Drag & drop files onto composer — (2026-03-22) iced event::listen_with FileDropped, routes to active conversation
- [x] ✅ **I3**: File picker + multiple attachments + upload progress — (2026-03-22) Attachment struct, rfd AsyncFileDialog, progress bar per attachment, upload slot flow
- [x] ✅ **I4**: Attachment preview in received messages — (2026-03-22)

---
## Orchestration Notes
- NO worktree isolation — agents work directly in main repo on non-overlapping files
- Agent A (Storage/UI): B4 → B5 → B6 → D2 → G-phase (touches ui/, store/)
- Agent B (Engine/XMPP): C1 → C2 → C3 → C4 → C5 → D3 (touches xmpp/engine.rs)
- Always run `cargo test && cargo clippy` before marking complete
- Commit after each completed task, never push

## Known Bugs (fix before release)
- [x] ✅ **BUG-1**: MAM historical messages trigger desktop notifications + sounds — added `is_historical: bool` to `IncomingMessage`; set `true` for MAM-sourced messages in engine.rs; skip notifications in ui/mod.rs when `is_historical` (2026-03-22)
- [x] ✅ **BUG-2**: Finder/permission modals open on connect — fixed by BUG-1 (2026-03-22)
- [x] ✅ **BUG-3**: MAM `fetched` count fixed — now uses `mam_result.messages.len()` instead of `rsm.count` (2026-03-22)
- [x] ✅ **BUG-4**: Auto-away does not escalate to extended away (XA) — fixed (2026-03-23)
- [x] ✅ **BUG-5**: Duplicate `Message::PaletteQuery(q)` match arm — fixed (2026-03-23)
- [x] ✅ **BUG-6**: Voice message composer fix — fixed (2026-03-23)

## Phase J — High Priority (from gap analysis)
- [ ] **J5**: OMEMO end-to-end encryption (XEP-0384) — Critical (in progress by OMEMO agent)
- [x] ✅ **J6** engine side — wire AvatarManager into engine (XEP-0084) — (2026-03-22)
- [ ] **J7**: File upload full UI flow (XEP-0363) — picker + paste + drag-drop + progress
- [ ] **J8**: Multi-account support — account switcher, per-account state (in progress)
- [x] ✅ **J9**: Account registration wizard (XEP-0077) (2026-03-23)
- [x] ✅ **J10** MAM preferences get/set — (2026-03-22)

## Phase K — Medium Priority (from gap analysis)
- [ ] **K1**: Proxy settings per-account (SOCKS5 + HTTP)
- [x] ✅ **K2**: vCard editing + browse public rooms (2026-03-23)
- [x] ✅ **K3**: Room invitations (XEP-0249) (2026-03-23)
- [x] ✅ **K4** Delivery receipts (XEP-0184) — (2026-03-22)
- [x] ✅ **K5** Read markers / displayed (XEP-0333) — (2026-03-22)
- [x] ✅ **K6**: Chat preferences panel — join/leave notifications, contact sorting (2026-03-23)
- [ ] **K7**: Push notifications (XEP-0357)

## Phase L — Low Priority (from gap analysis)
- [ ] **L1**: Voice messaging — record and send voice notes
- [ ] **L2**: Sticker packs support
- [ ] **L3**: Location sharing (XEP-0080)
- [x] ✅ **L4**: Ad-hoc commands UI (XEP-0050) (2026-03-23)
- [ ] **L5**: Spam reporting

## Phase K — Security & Encryption (gap analysis)
- [ ] **K1**: OMEMO end-to-end encryption (XEP-0384) — Critical; libsignal + device trust UI (in progress)
- [ ] **K2**: Device identity management + trust fingerprint verification UI

## Phase L — Account Management (gap analysis)
- [ ] **L1**: Multi-account support — scope DB + engine per JID; account switcher (in progress)
- [x] ✅ **L2**: Account registration wizard (XEP-0077 In-Band Registration) (2026-03-23)

## Phase M — Preferences & Settings gaps (gap analysis)
- [x] ✅ **M1**: System theme sync + 12h/24h time format + compact mode (2026-03-23)
- [ ] **M2**: Per-room notification mute/mentions-only; DND suppresses notifications
- [x] ✅ **M3**: Blocklist search + add JID UI (2026-03-23)
- [x] ✅ **M4**: Account details panel (JID, resources, connection method, auth, server caps) (2026-03-23)
- [ ] **M5**: Network settings: proxy SOCKS5/HTTP, manual SRV, TLS toggle
- [x] ✅ **M6**: Data & storage: MAM fetch limit, clear history, export conversations (2026-03-23)
- [x] ✅ **M7**: About modal: version, XEP count, license, GitHub link (2026-03-23)

## Phase N — Delivery & Read Markers (gap analysis)
- [x] ✅ **N1**: Delivery receipts (XEP-0184) — ✓/✓✓ status indicators on sent messages (2026-03-22)
- [x] ✅ **N2**: Read markers (XEP-0333) — displayed double-check indicator (2026-03-22)

## Phase O — Push Notifications (gap analysis)
- [ ] **O1**: XEP-0357 push notifications + VAPID registration
- [ ] **O2**: DND presence suppresses desktop notifications

## Phase P — Admin & Moderation (gap analysis)
- [x] ✅ **P1**: Ad-Hoc Commands UI (XEP-0050 + XEP-0004 dynamic forms) (2026-03-23)
- [ ] **P2**: Moderator retract button in MUC + reason in tombstone

## Phase Q — Other XEPs (gap analysis)
- [ ] **Q1**: Sticker packs
- [ ] **Q2**: Bits of Binary (XEP-0231)

## Phase R — UI/UX Polish (gap analysis)
- [x] ✅ **R1**: Reaction tooltips (who reacted), quick emoji bar, toggle on re-click (2026-03-23)
- [x] ✅ **R2**: Enhanced link previews + OGP image dimensions (2026-03-23)
- [x] ✅ **R3**: Composer markdown shortcuts (Ctrl+B/I), auto-grow, paste image (2026-03-23)

## UNICODE — Emoji & Unicode Rendering (High Priority)
- [x] ✅ **UNICODE**: Unicode & Emoji Rendering Support — Shaping::Advanced applied to most text widgets (2026-03-23)
  - Partially complete: conversation.rs pending Agent H (Shaping::Advanced not yet applied there)
  - **Checklist**:
    - [x] Font configuration (system fallback fonts used)
    - [x] `.shaping(Shaping::Advanced)` applied to sidebar, composer, chat widgets
    - [ ] Apply `.shaping(Shaping::Advanced)` to conversation.rs text widgets (pending Agent H)
    - [ ] Test emoji combinations: simple (😀), skin tones (👋🏽), ZWJ sequences (👨‍👩‍👧‍👦)

## Quick Wins — Auth UX
- [x] ✅ **AUTH-1**: "Remember me" / auto-login — auto-connect if credentials in keychain (2026-03-23)
- [x] ✅ **AUTH-2**: Logout button — in settings, clears session and returns to login screen (2026-03-23)
