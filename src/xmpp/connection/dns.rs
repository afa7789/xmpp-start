// Task P1.7 — DNS SRV resolution + XEP-0156 host-meta discovery
//
// SRV lookup order (RFC 6120 + XEP-0368):
//   1. _xmpps-client._tcp.{domain}  → Direct TLS
//   2. _xmpp-client._tcp.{domain}   → STARTTLS
//   3. Fallback: {domain}:5222 STARTTLS
//
// Source reference: apps/fluux/src-tauri/src/xmpp_proxy/dns.rs

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ResolvedEndpoint {
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
}

#[derive(Debug, Clone)]
pub enum TlsMode {
    Direct,
    StartTls,
}

/// Resolve the best connection endpoint for a domain.
/// TODO: Task P1.7 — implement using hickory-resolver
pub async fn resolve(domain: &str) -> Result<ResolvedEndpoint> {
    todo!(
        "Task P1.7: implement SRV resolution for domain {}",
        domain
    )
}
