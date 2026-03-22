// Task P1.1 — TCP connection + STARTTLS + Direct TLS
// Task P1.2 — SASL authentication
// Task P1.7 — DNS SRV + XEP-0156 discovery
//
// Source reference:
//   apps/fluux/src-tauri/src/xmpp_proxy/ (existing Rust proxy)
//   packages/fluux-sdk/src/core/modules/Connection.ts

pub mod dns;
pub mod sasl;
pub mod tcp;

use anyhow::Result;

/// Parsed server input from the login screen.
/// Mirrors the existing parse_server_input() in the Rust proxy.
#[derive(Debug, Clone)]
pub enum ServerTarget {
    /// Empty field or bare domain — perform SRV resolution
    Domain(String),
    /// Explicit direct TLS
    DirectTls { host: String, port: u16 },
    /// Explicit STARTTLS
    StartTls { host: String, port: u16 },
    /// WebSocket URL (RFC 7395)
    WebSocket(String),
}

impl ServerTarget {
    pub fn parse(input: &str, jid_domain: &str) -> Self {
        let input = input.trim();
        if input.is_empty() {
            return Self::Domain(jid_domain.to_string());
        }
        if input.starts_with("wss://") || input.starts_with("ws://") {
            return Self::WebSocket(input.to_string());
        }
        if let Some(host) = input.strip_prefix("tls://") {
            let (host, port) = parse_host_port(host, 5223);
            return Self::DirectTls { host, port };
        }
        if let Some(host) = input.strip_prefix("tcp://") {
            let (host, port) = parse_host_port(host, 5222);
            return Self::StartTls { host, port };
        }
        // bare host or host:port
        let (host, port) = parse_host_port(input, 5222);
        if port == 5223 {
            Self::DirectTls { host, port }
        } else {
            Self::StartTls { host, port }
        }
    }
}

fn parse_host_port(input: &str, default_port: u16) -> (String, u16) {
    match input.rsplit_once(':') {
        Some((host, port)) => {
            let port = port.parse().unwrap_or(default_port);
            (host.to_string(), port)
        }
        None => (input.to_string(), default_port),
    }
}

/// Establish an XMPP connection based on the server target.
/// Returns a connected, authenticated xmpp stream.
/// TODO: Task P1.1 + P1.2 — implement full connect flow
pub async fn connect(_jid: &str, _password: &str, _target: ServerTarget) -> Result<()> {
    todo!("Task P1.1: implement TCP connect + STARTTLS + SASL")
}
