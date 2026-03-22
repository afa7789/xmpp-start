# Contributing

## Development setup

```bash
git clone <repo>
cd xmpp-start
cargo build
cargo test --bin xmpp-start -- --test-threads=1
```

## Code style

- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass
- Tests required for all new modules

## Architecture

```
src/
├── main.rs              # iced entry point
├── config/              # settings, keychain
├── store/               # SQLite repos (sqlx)
├── ui/                  # iced screens and widgets
│   ├── mod.rs           # App state machine
│   ├── login.rs
│   ├── chat.rs
│   ├── sidebar.rs
│   ├── conversation.rs
│   ├── muc_panel.rs
│   └── styling.rs       # XEP-0393 message styling
└── xmpp/
    ├── engine.rs         # tokio-xmpp session loop
    ├── subscription.rs   # iced ↔ tokio bridge
    ├── connection/       # TCP, TLS, SASL, proxy
    └── modules/          # one file per XEP
```

## Commit format

```
feat(scope): short description
fix(scope): short description
chore(scope): short description
```

## Running a local XMPP server for testing

[Prosody](https://prosody.im) is the recommended local server:

```bash
# macOS
brew install prosody

# Start
prosody
```

Create a test account:
```bash
prosodyctl adduser test@localhost
```
