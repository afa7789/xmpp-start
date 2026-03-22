// Task P0.2 — XmppEngine: tokio channel sender + connect stub.
// The engine owns the sender half of a channel. The iced subscription
// owns the receiver and forwards events into Message variants.

use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use super::XmppEvent;

/// Drives the XMPP session and emits events through a channel.
pub struct XmppEngine {
    tx: mpsc::Sender<XmppEvent>,
}

impl XmppEngine {
    /// Create a new engine. The caller holds the receiver.
    pub fn new(tx: mpsc::Sender<XmppEvent>) -> Self {
        Self { tx }
    }

    /// Stub: simulate a successful connection after 100 ms.
    pub async fn connect(&self) -> anyhow::Result<()> {
        sleep(Duration::from_millis(100)).await;
        self.tx
            .send(XmppEvent::Connected)
            .await
            .map_err(|e| anyhow::anyhow!("channel closed: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmpp::connection::sasl::SaslMechanism;

    // --- XmppEvent derives ---

    #[test]
    fn xmpp_event_debug_clone() {
        let e = XmppEvent::Connected;
        let cloned = e.clone();
        // Debug must not panic
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn xmpp_event_disconnected_debug_clone() {
        let e = XmppEvent::Disconnected {
            reason: "test".into(),
        };
        let cloned = e.clone();
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn xmpp_event_reconnecting_debug_clone() {
        let e = XmppEvent::Reconnecting { attempt: 3 };
        let cloned = e.clone();
        let _ = format!("{cloned:?}");
    }

    // --- SaslMechanism::select ---

    #[test]
    fn sasl_select_prefers_scram_sha256() {
        let offered = vec![
            "PLAIN".to_string(),
            "SCRAM-SHA-1".to_string(),
            "SCRAM-SHA-256".to_string(),
        ];
        assert_eq!(SaslMechanism::select(&offered), Some(SaslMechanism::ScramSha256));
    }

    #[test]
    fn sasl_select_falls_back_to_scram_sha1() {
        let offered = vec!["PLAIN".to_string(), "SCRAM-SHA-1".to_string()];
        assert_eq!(SaslMechanism::select(&offered), Some(SaslMechanism::ScramSha1));
    }

    #[test]
    fn sasl_select_falls_back_to_plain() {
        let offered = vec!["PLAIN".to_string()];
        assert_eq!(SaslMechanism::select(&offered), Some(SaslMechanism::Plain));
    }

    #[test]
    fn sasl_select_returns_none_when_nothing_matches() {
        let offered = vec!["GSSAPI".to_string(), "EXTERNAL".to_string()];
        assert_eq!(SaslMechanism::select(&offered), None);
    }

    #[test]
    fn sasl_select_empty_offered() {
        assert_eq!(SaslMechanism::select(&[]), None);
    }

    // --- connect stub sends Connected ---

    #[tokio::test]
    async fn engine_connect_sends_connected_event() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = XmppEngine::new(tx);
        engine.connect().await.expect("connect should not fail");
        let event = rx.recv().await.expect("should receive an event");
        assert!(matches!(event, XmppEvent::Connected));
    }
}
