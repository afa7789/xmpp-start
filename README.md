# xmpp-start

Native XMPP desktop messenger — pure Rust rewrite using [iced](https://github.com/iced-rs/iced).

> Replaces the Tauri + React/TypeScript stack of fluux-messenger with a fully native Rust application.

## Stack

| Layer | Technology |
|---|---|
| GUI | iced 0.13 |
| Async runtime | tokio |
| XMPP | tokio-xmpp + xmpp-parsers |
| TLS | rustls |
| Storage | SQLite via sqlx |
| Keychain | keyring |
| i18n | fluent-rs |

## Features

- XMPP login with SASL PLAIN / SCRAM-SHA-256
- Contact roster with presence indicators
- 1:1 chat with message history (XEP-0313 MAM)
- Group chat / MUC (XEP-0045)
- Message corrections (XEP-0308), retractions (XEP-0424), reactions (XEP-0444)
- File upload (XEP-0363)
- Stream Management / session resumption (XEP-0198)
- Message Carbons (XEP-0280)
- Dark / light theme
- macOS / Linux / Windows

## Building

```bash
cargo build --release
```

Requires Rust 1.80+.

## Running

```bash
cargo run
```

Set `RUST_LOG=xmpp_start=debug` for verbose logging.

## License

MIT
