#![allow(dead_code)]
// Task P1.2 — SASL authentication (PLAIN + SCRAM-SHA-256)
//
// Source reference: packages/fluux-sdk/src/core/modules/Connection.ts
// SCRAM-SHA-256 preferred when offered by server.
// PLAIN only as fallback (requires TLS).

/// SASL mechanism to use for authentication.
#[derive(Debug, Clone, PartialEq)]
pub enum SaslMechanism {
    Plain,
    ScramSha1,
    ScramSha256,
}

impl SaslMechanism {
    /// Select the best mechanism from server-offered list.
    /// Prefer SCRAM-SHA-256 > SCRAM-SHA-1 > PLAIN.
    pub fn select(offered: &[String]) -> Option<Self> {
        if offered.iter().any(|m| m == "SCRAM-SHA-256") {
            Some(Self::ScramSha256)
        } else if offered.iter().any(|m| m == "SCRAM-SHA-1") {
            Some(Self::ScramSha1)
        } else if offered.iter().any(|m| m == "PLAIN") {
            Some(Self::Plain)
        } else {
            None
        }
    }
}
