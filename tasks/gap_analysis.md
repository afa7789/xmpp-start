# Feature Gap Analysis — xmpp-start vs Fluux + Gajim
Generated: 2026-03-22

## Summary
xmpp-start has solid messaging fundamentals but is missing critical features for production: OMEMO encryption, avatar display/upload, file upload UI flow, and multi-account support. Most infrastructure modules exist in `src/xmpp/modules/` — they just need wiring.

## Critical gaps (blocking production use)
- **OMEMO (XEP-0384)** — E2E encryption. Gajim ✅ Fluux ⚠️. No module exists yet. rust-omemo crate available.
- **Avatar display/upload (XEP-0084 + XEP-0153)** — AvatarManager exists but never called from engine.
- **File upload UI (XEP-0363)** — FileUploadManager exists but no picker/paste/drag-drop UI flow.
- **Multi-account support** — App is hardcoded to single JID.
- **Account registration (XEP-0077)** — Account module exists, no UI wizard.

## Settings gaps vs Gajim preferences
- Chat preferences: show/hide join-leave notifications, contact sorting
- Per-contact notification muting (J3 partial)
- MAM archiving mode (All/Contacts-only/None)
- Proxy settings per-account
- vCard editing (nickname, org, email, photo)
- Certificate pinning / TLS options
- Sound file customization

## Phase J — High Priority (NEW)
- [ ] **J5**: OMEMO end-to-end encryption (XEP-0384) — Critical. rust-omemo crate. PubSub key storage. Trust fingerprints UI.
- [ ] **J6**: Avatar fetch + display (XEP-0084 + XEP-0153) — Wire AvatarManager in engine; show in sidebar + bubbles
- [ ] **J7**: File upload full UI flow (XEP-0363) — Picker + paste + drag-drop + progress bar
- [ ] **J8**: Multi-account support — Account switcher UI, per-account state, config stores multiple JIDs
- [ ] **J9**: Account registration wizard (XEP-0077) — Registration form UI, server check, auto-setup on first launch
- [ ] **J10**: MAM preferences dialog — Archive mode selector (All/Contacts/None) in settings

## Phase K — Medium Priority (NEW)
- [ ] **K1**: Proxy settings per-account (SOCKS5 + HTTP)
- [ ] **K2**: vCard editing (XEP-0054 + XEP-0292) — Edit nickname, org, email, avatar
- [ ] **K3**: Per-contact notification muting refinement (right-click → Mute)
- [ ] **K4**: Delivery receipts (XEP-0184) — Sent/delivered checkmarks
- [ ] **K5**: Read markers / displayed (XEP-0333) — Double checkmark on read
- [ ] **K6**: Chat preferences panel — join/leave notifications, contact sorting options
- [ ] **K7**: Push notifications (XEP-0357) — Background push for mobile-style experience

## Phase L — Low Priority (NEW)
- [ ] **L1**: Voice messaging — Record and send voice notes
- [ ] **L2**: Sticker packs support
- [ ] **L3**: Location sharing (XEP-0080)
- [ ] **L4**: Ad-hoc commands UI (XEP-0050) — Server/bot command execution
- [ ] **L5**: Spam reporting

---
# Deep Gap Analysis — from local Fluux + Gajim source
Generated: 2026-03-22 (local source review)

## Phase K — Security & Encryption
- [ ] **K1**: OMEMO end-to-end encryption (XEP-0384) — Critical; libsignal + device trust UI
- [ ] **K2**: Per-contact device identity management and trust fingerprint verification

## Phase L — Account Management
- [ ] **L1**: Multi-account support — scope DB and engine per JID; account switcher UI
- [ ] **L2**: Account registration wizard (XEP-0077 In-Band Registration)

## Phase M — Preferences & Settings (gaps vs Gajim/Fluux)
- [ ] **M1**: System theme sync + time format (12h/24h/auto) + compact mode
- [ ] **M2**: Per-room notification mute/mentions-only; DND suppresses notifications
- [ ] **M3**: Blocklist search + add JID UI (Fluux has full BlockedUsersSettings)
- [ ] **M4**: Account details panel: JID, resources, connection method, auth, server caps
- [ ] **M5**: Network settings: proxy (SOCKS5/HTTP), manual SRV, TLS verification toggle
- [ ] **M6**: Data & storage: MAM fetch limit setting, clear history, export conversations
- [ ] **M7**: About modal: version, XEPs count, license, GitHub link, auto-update check

## Phase N — Message Delivery & Read Markers
- [ ] **N1**: Delivery receipts (XEP-0184) — ✓/✓✓ status on sent messages
- [ ] **N2**: Chat Markers / Read Markers (XEP-0333) — "read" double-check indicator

## Phase O — Push Notifications
- [ ] **O1**: XEP-0357 Push Notifications + VAPID registration (mobile/web)
- [ ] **O2**: DND presence suppresses desktop notifications

## Phase P — Admin & Moderation
- [ ] **P1**: Ad-Hoc Commands UI (XEP-0050) with XEP-0004 dynamic form rendering
- [ ] **P2**: Moderator retract button in MUC + moderation reason in tombstone

## Phase Q — Other XEPs
- [ ] **Q1**: Sticker packs
- [ ] **Q2**: Bits of Binary (XEP-0231) for embedded images

## Phase R — UI/UX Polish
- [ ] **R1**: Reaction tooltips (who reacted), quick emoji bar, toggle on re-click
- [ ] **R2**: Enhanced link previews with OGP image dimensions (XEP-0264)
- [ ] **R3**: Composer: markdown shortcuts (Ctrl+B/I), auto-grow textarea, paste image

## Priority for MVP+1
Critical: K1 (OMEMO), L1 (multi-account), N1+N2 (delivery/read receipts)
High: L2 (registration), M1-M5 (settings), O1 (push)
Medium: M6-M7, P1-P2, R1-R3
