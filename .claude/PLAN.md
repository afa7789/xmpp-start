# ReXisCe — Consolidated Project Status

> **Date:** 2026-03-29 | **Build:** 1026 tests passing, 1 clippy warning | **Dead code:** 95 `#[allow(dead_code)]` annotations across ~30 files

---

## 📊 Overall Progress

| Track | Done | Pending | Blocked |
|---|---|---|---|
| **tasks.yaml** (main) | ~93 | 2 (E2E GUI) | 1 (cleanup-dead-code — replaced by wire tasks) |
| **review-tasks.yaml** (code review) | 10/12 | 0 | 0 |
| **wire-tasks.yaml** (dead code wiring) | 14/14 had been planned, ~6 remain in YAML as Pending | ~6 | 0 |
| **tdd-tasks.yaml** (UI tests) | 0 | 6 | 0 |
| **test-server-tasks.yaml** (Docker Prosody) | 0 | 9 | 0 |

### What Was Done (from RESUME.md — last sprint: 21 tasks)

**Code Review Security Fixes (7 ✅):**
- `sec-cred-permissions` — 0600 on credentials.json
- `sec-insecure-tls-gate` — InsecureTlsConfig gated behind setting
- `fix-carbon-verify-sender` — XEP-0280 §6 verification
- `fix-muc-ignore-jid` — occupant nick check
- `fix-message-dedup-ui` — in-memory HashSet dedup
- `fix-prekey-id-collision` — random u32 pre-key IDs
- `improve-notification-privacy` — "Encrypted message" in notifications

**Wire-Up / Dead Code Activated (14 ✅):**
- `wire-remove-keychain`, `wire-msg-dedup-origin-id`, `wire-unread-badge-count`
- `wire-sidebar-remove-contact`, `wire-composer-drafts`, `wire-settings-tabs`
- `wire-delivery-indicators`, `wire-conversation-archive`, `wire-conversation-mute`
- `wire-multi-account`, `wire-data-forms`, `wire-mam-local-pagination`
- `wire-thumbnail-dimensions`, `improve-avatar-storage`

**Also done (earlier sessions):**
- `sec-omemo-spk` — dedicated OMEMO signed pre-key
- `sec-omemo-tofu` — user confirmation for device trust
- `refactor-engine-context` — SessionContext struct (18-param → struct)
- All bug fixes (MUC sidebar, settings modal, OMEMO activation, presence dot, avatars, broken icons, keychain removal)
- XEP-0077 In-Band Registration wizard
- XEP-0084 User Avatar, XEP-0425 Moderated Retraction
- Audio playback + Opus compression
- Multi-account auto-connect, conversation archive UI, OMEMO persistence

---

## ❌ What Remains

### Priority 1 — E2E GUI Tests (Pending, need Docker Prosody)

| ID | Description |
|---|---|
| `e2e-gui-add-contact-msg` | Add contact → send message → verify via go-sendxmpp |
| `e2e-gui-muc-join-msg` | Join MUC → send message → verify via go-sendxmpp MUC listen |

### Priority 2 — UI TDD Tests (6 task groups, 24 test cases)

| ID | Cases | Files |
|---|---|---|
| `tdd-login-tests` | 4 | src/ui/login.rs |
| `tdd-sidebar-tests` | 6 | src/ui/sidebar.rs |
| `tdd-conversation-tests` | 6 | src/ui/conversation/mod.rs, src/ui/chat.rs |
| `tdd-settings-tests` | 4 | src/ui/settings.rs |
| `tdd-screen-transition-tests` | 4 | src/ui/mod.rs (deferred — needs App test constructor) |
| `bug-presence-status-visual` | — | ✅ Already done |
| `investigate-omemo-send` | — | ✅ Already done |

### Priority 3 — Docker Test Server (9 tasks)

All infrastructure tasks (`ts-scaffold`, `ts-prosody-config`, `ts-setup-script`, `ts-makefile`, `ts-verify-boot`, `ts-client-localhost-compat`, `ts-test-login`, `ts-test-messaging`, `ts-test-muc`, `ts-test-settings-omemo`).

### Priority 4 — Settings Redesign (spec written, not started)

Gajim-style modal with sidebar navigation. Spec in [SETTINGS_REDESIGN.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/SETTINGS_REDESIGN.md). Blocked by: nothing, but low priority.

---

## 🔴 Dead Code Inventory (95 annotations, current state)

### By Area — What's Left

#### UI Layer (30 annotations)

| File | Count | Items | Status |
|---|---|---|---|
| `ui/palette.rs` | 1 | `#![allow(dead_code)]` (module-level) — color constants | **In use by settings, leave or audit** |
| `ui/styles.rs` | 1 | `#![allow(dead_code)]` (module-level) — LINK_COLOR, link_style(), cancel_btn_style() | **Scaffolding for future features** |
| `ui/audio_player.rs` | 1 | `#![allow(dead_code)]` (module-level) — AudioPlayer, is_audio_url() | **Voice playback, marked done but annotations remain** |
| `ui/about.rs` | 1 | line 24 | Minor |
| `ui/account_state.rs` | 3 | lines 34, 38, 44 — PresenceStatus variants | **Partially wired (wire-multi-account done?)** |
| `ui/sidebar.rs` | 2 | lines 87, 184 — Action enum, method | **wire-sidebar-remove-contact done?** |
| `ui/muc_panel.rs` | 3 | lines 41, 70, 88 | Future MUC panel features |
| `ui/mod.rs` | 1 | line 44 | Minor |
| `ui/omemo_trust.rs` | 4 | lines 37, 42, 153, 159 — OwnDeviceInfo, methods | UI wired but annotations left |
| `ui/account_switcher.rs` | 1 | line 26 | Multi-account UI |
| `ui/settings.rs` | 3 | lines 236, 238, 240 — SettingsTab variants | **wire-settings-tabs done, annotations may remain** |
| `ui/data_forms.rs` | 2 | lines 43, 74 — DataForm, FormField | **wire-data-forms done** |
| `ui/chat.rs` | 1 | line 1075 — draft_for() | **wire-composer-drafts done** |
| `ui/conversation/mod.rs` | 1 | line 172 — MessageState | **wire-delivery-indicators done** |

#### Config Layer (3 annotations)

| File | Count | Items |
|---|---|---|
| `config/mod.rs` | 3 | line 67 (mam field), 401/413 (validate funcs) |

#### XMPP Layer (42 annotations)

| File | Count | Items |
|---|---|---|
| `xmpp/multi_engine.rs` | 6 | lines 46, 178, 195, 204, 222, 229 — full multi-engine state machine |
| `xmpp/connection/proxy.rs` | 1 | `#![allow(dead_code)]` (module-level) — entire proxy module |
| `xmpp/connection/dns.rs` | 2 | lines 20, 32 |
| `xmpp/connection/mod.rs` | 3 | lines 66, 80, 107, 121 |
| `xmpp/connection/insecure_tls.rs` | 2 | lines 144, 163 |
| `xmpp/connection/sasl.rs` | 2 | lines 9, 19 |
| `xmpp/mod.rs` | 2 | lines 73 (is_trusted), 299 |
| `xmpp/modules/avatar.rs` | 2 | lines 434, 440 |
| `xmpp/modules/disco.rs` | 3 | lines 174, 309, 318 |
| `xmpp/modules/xmpp_uri.rs` | 1 | line 103 |
| `xmpp/modules/sync.rs` | 2 | lines 135, 144 |
| `xmpp/modules/registration.rs` | 1 | line 30 |
| `xmpp/modules/catchup.rs` | 1 | line 28 |
| `xmpp/modules/console.rs` | 5 | lines 21, 66, 72, 78, 84 — debug console |
| `xmpp/modules/muc.rs` | 3 | lines 66, 78, 118 |
| `xmpp/modules/push.rs` | 6 | lines 130, 157, 170, 191, 199, 205 — PushManager methods |
| `xmpp/modules/entity_time.rs` | 4 | lines 49, 71, 100, 142 |
| `xmpp/modules/file_upload.rs` | 1 | line 209 — on_slot_error |
| `xmpp/modules/muc_config.rs` | 2 | lines 18, 25 |
| `xmpp/modules/bob.rs` | 1 | line 46 |
| `xmpp/modules/bookmarks.rs` | 2 | lines 55, 83 |
| `xmpp/modules/omemo/message.rs` | 2 | lines 94, 146 |
| `xmpp/modules/omemo/session.rs` | 1 | line 37 |
| `xmpp/modules/omemo/store.rs` | 1 | line 41 |
| `xmpp/modules/mam.rs` | 3 | lines 74, 393, 399 |
| `xmpp/modules/muc_admin.rs` | 1 | line 15 |
| `xmpp/modules/stickers.rs` | 2 | lines 64, 92 |

#### Store Layer (4 annotations)

| File | Count | Items |
|---|---|---|
| `store/message_repo.rs` | 4 | lines 66, 77, 104, 152 — find_by_origin_id, find_before, count_unread |

---

## 🟡 Observations

1. **Tasks marked "Done" in tasks.yaml still have `#[allow(dead_code)]` annotations in source.** The wire-up tasks (wire-delivery-indicators, wire-composer-drafts, etc.) were marked done, but 95 annotations remain. Either the annotations weren't cleaned up, or the "wiring" added callers but left the suppress annotation in place.

2. **Biggest dead code clusters** that could be cleaned up most impactfully:
   - `xmpp/modules/push.rs` (6 annotations) — was marked done
   - `xmpp/multi_engine.rs` (6 annotations) — was marked done
   - `xmpp/modules/console.rs` (5 annotations) — debug console, probably leave
   - `xmpp/modules/entity_time.rs` (4 annotations) — utility, partially used
   - `xmpp/connection/` (10 annotations total) — proxy/dns/sasl infrastructure

3. **Wire tasks that are "Done" but still show dead_code** — need an audit pass to just remove the `#[allow(dead_code)]` annotations where the code IS now used.

---

## 📋 XEP Compliance Summary

- **29 XEPs fully implemented**, 7 partial, 1 stub (stickers)
- **Missing (6):** XEP-0066 (OOB), XEP-0352 (CSI), XEP-0392 (Colors), XEP-0393 (Message Styling), XEP-0402 (PEP Bookmarks), Jingle (Voice/Video)
- **Beyond compliance suite:** XEP-0050 (Ad-Hoc), XEP-0080 (GeoLoc), XEP-0202 (Entity Time), XEP-0231 (BoB), XEP-0334 (Hints), XEP-0377 (Spam), XEP-0449 (Stickers stub)

---

## 🎯 Recommended Next Steps

| Priority | Action | Effort |
|---|---|---|
| **1** | **Dead code annotation sweep** — Remove `#[allow(dead_code)]` from items that ARE now used (post-wire tasks). Just remove annotations, no logic changes. Should eliminate ~30-40 annotations. | Small (1-2h) |
| **2** | **TDD tests** — Add the 20 UI state machine tests (login, sidebar, conversation, settings). No deps, pure logic. | Medium (2-3h) |
| **3** | **Docker test server** — Create test-server/ directory with full Prosody setup. All specs are written. | Medium (1h) |
| **4** | **True dead code removal** — Delete items that will never be used (leftover scaffolding, debug console, stubs). | Small (1h) |
| **5** | **Settings redesign** — Gajim-style modal (spec ready). | Large (4-6h) |

---

## 📁 Source Documents

| File | Size | Purpose |
|---|---|---|
| [RESUME.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/RESUME.md) | 2.4K | Session resume with 21 completed tasks |
| [TASK_PRIORITIES.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/TASK_PRIORITIES.md) | 6.6K | Dependency graph & execution waves |
| [DEAD_CODE_PLAN.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/DEAD_CODE_PLAN.md) | 5.4K | Original 31-item dead code audit |
| [CODE_REVIEW.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/CODE_REVIEW.md) | 3.5K | Security review with B1-B4 blockers (all fixed) |
| [XEP_COMPLIANCE.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/XEP_COMPLIANCE.md) | 2.7K | 29 XEPs implemented, 6 missing |
| [SETTINGS_REDESIGN.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/SETTINGS_REDESIGN.md) | 4.9K | Gajim-style settings spec |
| [TERMINAL_CLIENTS.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/TERMINAL_CLIENTS.md) | 8.6K | profanity + go-sendxmpp testing guide |
| [UI_TDD_PLAN.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/UI_TDD_PLAN.md) | 19.9K | 24 test cases with examples |
| [TEST_SERVER_PLAN.md](file:///Users/afa/Developer/arthur/xmpp-start/.claude/TEST_SERVER_PLAN.md) | 27.5K | Docker Prosody setup + 14 test scenarios |
| [tasks.yaml](file:///Users/afa/Developer/arthur/xmpp-start/.claude/tasks.yaml) | 79.9K | Main tracker (~93 done, 2 pending) |
| [wire-tasks.yaml](file:///Users/afa/Developer/arthur/xmpp-start/.claude/wire-tasks.yaml) | 17.0K | Dead code wire-up tasks |
| [review-tasks.yaml](file:///Users/afa/Developer/arthur/xmpp-start/.claude/review-tasks.yaml) | 4.3K | Code review fix tasks |
| [tdd-tasks.yaml](file:///Users/afa/Developer/arthur/xmpp-start/.claude/tdd-tasks.yaml) | 8.8K | UI state machine test tasks |
| [test-server-tasks.yaml](file:///Users/afa/Developer/arthur/xmpp-start/.claude/test-server-tasks.yaml) | 8.1K | Docker infra tasks |
