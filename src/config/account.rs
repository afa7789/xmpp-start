// MULTI: Account configuration model
//
// Each entry represents one XMPP account the user has configured.
// Passwords are stored in the OS keychain; only a reference key is persisted here.

use serde::{Deserialize, Serialize};

/// Optional proxy configuration for routing an account's connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyConfig {
    /// SOCKS5 / HTTP proxy host.
    pub host: String,
    /// Proxy port.
    pub port: u16,
}

/// Configuration for a single XMPP account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Bare JID for this account (e.g. "user@example.com").
    pub jid: String,
    /// Key used to look up the password in the OS keychain.
    /// Typically equals `jid`, but stored separately so it can be rotated.
    pub password_key: String,
    /// Whether this account should be connected on startup.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional proxy for this account's XMPP connection.
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
    /// Accent colour used in the UI to distinguish accounts (e.g. "#4A90D9").
    #[serde(default)]
    pub color: Option<String>,
}

fn default_true() -> bool {
    true
}

impl AccountConfig {
    /// Construct a minimal account config (enabled, no proxy, no colour).
    pub fn new(jid: impl Into<String>) -> Self {
        let jid = jid.into();
        let password_key = jid.clone();
        Self {
            jid,
            password_key,
            enabled: true,
            proxy: None,
            color: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_config_defaults() {
        let a = AccountConfig::new("alice@example.com");
        assert_eq!(a.jid, "alice@example.com");
        assert_eq!(a.password_key, "alice@example.com");
        assert!(a.enabled);
        assert!(a.proxy.is_none());
        assert!(a.color.is_none());
    }

    #[test]
    fn account_config_round_trip_json() {
        let a = AccountConfig {
            jid: "bob@xmpp.org".into(),
            password_key: "bob@xmpp.org".into(),
            enabled: false,
            proxy: Some(ProxyConfig {
                host: "proxy.corp.com".into(),
                port: 1080,
            }),
            color: Some("#FF5733".into()),
        };

        let json = serde_json::to_string(&a).unwrap();
        let b: AccountConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(b.jid, "bob@xmpp.org");
        assert!(!b.enabled);
        let proxy = b.proxy.unwrap();
        assert_eq!(proxy.host, "proxy.corp.com");
        assert_eq!(proxy.port, 1080);
        assert_eq!(b.color.as_deref(), Some("#FF5733"));
    }

    #[test]
    fn account_config_missing_optional_fields_deserialize() {
        // Old JSON without `proxy` / `color` / `enabled` must still parse.
        let json = r#"{"jid":"carol@test.net","password_key":"carol@test.net"}"#;
        let c: AccountConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.jid, "carol@test.net");
        assert!(c.enabled); // default_true
        assert!(c.proxy.is_none());
        assert!(c.color.is_none());
    }
}
