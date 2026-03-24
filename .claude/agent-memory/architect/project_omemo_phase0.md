---
name: OMEMO Phase 0 Foundation
description: Status and key decisions from the OMEMO XEP-0384 Phase 0 foundation work
type: project
---

OMEMO Phase 0 was completed on 2026-03-23 by Agent L (Architect role).

All files are in HEAD (commit ba8b6fc), merged with other changes.

**What was delivered:**
- `docs/OMEMO_ARCHITECTURE.md` — full design document (data flow, DB schema, trust model, stanza structure, 5-phase plan, risks)
- `migrations/0004_omemo_keys.up.sql` — four tables: omemo_identity, omemo_prekeys, omemo_sessions, omemo_devices
- `src/xmpp/modules/omemo/store.rs` — OmemoStore SQLite persistence layer (uses untyped sqlx::query() API — NOT sqlx::query! macro)
- `src/xmpp/modules/omemo/device.rs` — DeviceManager: PEP device list build/parse, 8 unit tests
- `src/xmpp/modules/omemo/mod.rs` — module root
- `src/xmpp/mod.rs` — added XmppEvent::{OmemoDeviceListReceived, OmemoMessageDecrypted, OmemoKeyExchangeNeeded} and XmppCommand::{OmemoEnable, OmemoEncryptMessage, OmemoTrustDevice}
- `Cargo.toml` — added vodozemac = "0.8"

**Key decisions:**
- vodozemac (not libsignal-protocol C bindings) — pure Rust, Matrix-maintained
- Target OMEMO 0.8.3 namespace (urn:xmpp:omemo:2), NOT legacy eu.siacs.conversations.axolotl
- MUC OMEMO out of scope for now — 1:1 only
- TOFU trust model as default

**Next phases:**
- Phase 1: OmemoManager::enable() — key generation + PEP publish
- Phase 2: Outbound encrypt (1:1)
- Phase 3: Inbound decrypt + session update
- Phase 4: Trust UI panel

**Why:** vodozemac has no C FFI risk and is audited; untyped sqlx API avoids DATABASE_URL compile-time requirement.
**How to apply:** When continuing OMEMO work, read docs/OMEMO_ARCHITECTURE.md first. Do NOT touch src/ui/conversation.rs, src/ui/settings.rs, or src/xmpp/engine.rs directly.
