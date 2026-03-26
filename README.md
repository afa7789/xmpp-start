# ReXisCe — A Rust XMPP Client

<img src="resources/Rexisce_banner.jpg" width="400" alt="ReXisCe Banner">

Native XMPP desktop messenger — pure Rust using [iced](https://github.com/iced-rs/iced).


---

## About the name

**ReXisCe** (pronunciation: /ˈʁe.xiʃ.se/ — "ré-xis-sê")

The name comes from the acronym **RXC** (R → Rust, X → XMPP, C → Client), transformed into a pronounceable word:

- **Re** — "R" (pronunciado "ré" em português)
- **Xis** — "X" (pronunciado "xis" em português)  
- **Ce** — "C" (pronunciado "sê" em português)

This solves a common problem: pure acronyms are hard to speak. ReXisCe turns the abbreviation into a name.

| Item | Name |
|------|------|
| Project | ReXisCe |
| Meaning | Rust XMPP Client |
| CLI binary | `rexisce` |
| Lib crate | `rexisce` |
| Config dir | `~/.config/rexisce/` |

---

## Requirements

- [Rust 1.80+](https://rustup.rs) — `rustup update stable`
- Linux: `sudo apt install libdbus-1-dev pkg-config libssl-dev`
- macOS / Windows: no extra deps

---

## Getting started

```bash
git clone https://github.com/owner/rexisce
cd rexisce
make run
```

That's it. The first run compiles everything and opens the app.

---

## Common commands

```bash
make setup          # update Rust toolchain
make run            # compile (debug) and run
make build          # compile release binary → target/release/rexisce
make run-release    # compile release and run
make test           # run the full test suite
make lint           # clippy (warnings = errors)
make fmt            # auto-format source files
make clean          # delete build artifacts
```

For verbose XMPP logging:

```bash
RUST_LOG=rexisce=debug make run
```

---

## Integration tests

The `tests/critical_flows.rs` file covers end-to-end flows across modules (login → connect, MAM catchup, presence transitions, blocking, etc.):

```bash
make test-integration
```

---

## Stack

| Layer | Crate |
|---|---|
| GUI | iced 0.13 |
| XMPP | tokio-xmpp + xmpp-parsers |
| Async | tokio |
| TLS | rustls |
| Storage | SQLite via sqlx |
| Keychain | keyring |
| i18n | fluent-rs |
| Notifications | notify-rust |

---

## Implemented XEPs

### Core
- [XEP-0030](https://xmpp.org/extensions/xep-0030.html) — Service Discovery
- [XEP-0115](https://xmpp.org/extensions/xep-0115.html) — Entity Capabilities (CAPS)
- [XEP-0153](https://xmpp.org/extensions/xep-0153.html) — vCard-Based Avatar
- [XEP-0191](https://xmpp.org/extensions/xep-0191.html) — Simple Communications Blocking
- [XEP-0198](https://xmpp.org/extensions/xep-0198.html) — Stream Management
- [XEP-0245](https://xmpp.org/extensions/xep-0245.html) — /me Command in Chat Messages
- [XEP-0280](https://xmpp.org/extensions/xep-0280.html) — Message Carbons
- [XEP-0313](https://xmpp.org/extensions/xep-0313.html) — Message Archive Management (MAM)
- [XEP-0363](https://xmpp.org/extensions/xep-0363.html) — HTTP File Upload

### Chat Features
- [XEP-0079](https://xmpp.org/extensions/xep-0079.html) — Advanced Message Processing (AMP)
- [XEP-0085](https://xmpp.org/extensions/xep-0085.html) — Chat State Notifications (typing indicators)
- [XEP-0184](https://xmpp.org/extensions/xep-0184.html) — Message Delivery Receipts
- [XEP-0245](https://xmpp.org/extensions/xep-0245.html) — /me Actions
- [XEP-0308](https://xmpp.org/extensions/xep-0308.html) — Message Correction
- [XEP-0333](https://xmpp.org/extensions/xep-0333.html) — Chat Markers (read receipts)
- [XEP-0424](https://xmpp.org/extensions/xep-0424.html) — Message Retraction
- [XEP-0444](https://xmpp.org/extensions/xep-0444.html) — User Mood
- [XEP-0445](https://xmpp.org/extensions/xep-0445.html) — Message Reactions
- [XEP-0461](https://xmpp.org/extensions/xep-0461.html) — Reply to Messages
- [XEP-0393](https://xmpp.org/extensions/xep-0393.html) — Message Styling

### Presence & Status
- [XEP-0178](https://xmpp.org/extensions/xep-0178.html) — Presence Subscribe (Approved)
- [XEP-0252](https://xmpp.org/extensions/xep-0252.html) — Client State Indication (auto-away)
- Custom Status Message

### MUC (Group Chat)
- [XEP-0045](https://xmpp.org/extensions/xep-0045.html) — Multi-User Chat
- [XEP-0048](https://xmpp.org/extensions/xep-0048.html) — Bookmarks

### Privacy
- Delivery Receipts toggle
- Typing Indicators toggle
- Read Markers toggle

### Other
- [XEP-0050](https://xmpp.org/extensions/xep-0050.html) — Ad-Hoc Commands (partial)
- [XEP-0065](https://xmpp.org/extensions/xep-0065.html) — SOCKS5 Bytestreams (for file transfer)

---

## Planned XEPs

### High Priority
- [XEP-0084](https://xmpp.org/extensions/xep-0084.html) — PubSub Avatar (PEP metadata)
- [XEP-0410](https://xmpp.org/extensions/xep-0410.html) — Self-Ping (MUC keepalive)
- [XEP-0352](https://xmpp.org/extensions/xep-0352.html) — Client State Indication (CSI) — outgoing queue priority
- OMEMO (XEP-0440 + Signal-style double ratchet) — end-to-end encryption

### MUC Admin (from Gajim analysis)
- MUC Affiliation Management (bans, admins, owners)
- MUC Role Management (moderator, participant, visitor)
- MUC Room Configuration (data forms)
- MUC Voice Request (request/toggle speaking permission)
- MUC Admin (full)

### UI/UX
- XEP-0004 Data Forms Renderer (for ad-hoc commands, room config, registration)
- Privacy Lists (XEP-0016 / XEP-0191 extended)
- Bookmark Sync (XEP-0048 via PepPublish)
- Message Synchronization (XEP-0313 with full sync controls)
- Roster Item Exchange (XEP-0144)
- In-Band Registration (XEP-0077)
- Service Discovery Browser (XEP-0030 extended)
- Personal Event Publishing (XEP-0163 / PEP)
- User Tune (XEP-0118)
- User Location (XEP-0080)
- File Transfer (XEP-0065 + ICE-UDP)
- Audio/Video Calls (Jingle / XEP-0167)

### Account Features
- Multi-account support
- Remember-Me (AUTH-1)
- Account migration/export

---

## Features

- XMPP login (SASL PLAIN / SCRAM-SHA-256, STARTTLS / Direct TLS)
- Contact roster with presence indicators
- 1:1 chat with message history (MAM)
- Group chat / MUC with roles, moderation, bookmarks
- Message corrections, retractions, reactions
- File upload with image thumbnails
- Avatars (vCard-temp + PEP)
- Stream Management / session resumption
- Message Carbons (sync across devices)
- Entity Capabilities + Service Discovery
- Blocking with server-side blocklist
- Ad-Hoc Commands (partial)
- Auto-away after inactivity
- Dark / light theme
- i18n (en-US, pt-BR)
- Desktop notifications
- Privacy toggles (receipts, typing, read markers)
- MAM archiving preferences
- Remember Me (password persistence in keychain)
- macOS, Linux, Windows

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT

> Built with vibecoding, using other XMPP clients as reference (Halloy, Dino, Gajim).
