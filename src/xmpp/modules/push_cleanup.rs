// Task P6.5 — WebPush VAPID unsubscribe stanza builder
// XEP reference: https://xmpp.org/extensions/xep-0357.html
//
// Builds IQ stanzas to disable push notification subscriptions on the server.
// This is needed when migrating away from WebPush — the server must be told to
// drop stale VAPID subscriptions so it stops attempting push delivery.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_PUSH};

// ---------------------------------------------------------------------------
// PushCleanup
// ---------------------------------------------------------------------------

/// XEP-0357 push-disable stanza builder.
///
/// Produces IQ stanzas that ask the server to stop delivering push
/// notifications for a specific subscription or for all subscriptions.
pub struct PushCleanup;

impl Default for PushCleanup {
    fn default() -> Self {
        Self::new()
    }
}

impl PushCleanup {
    /// Create a new `PushCleanup` builder.
    pub fn new() -> Self {
        Self
    }

    /// Build an IQ to disable push notifications for a specific push service
    /// JID and node pair.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}" xmlns="jabber:client">
    ///   <disable xmlns="urn:xmpp:push:0" jid="{push_service_jid}" node="{node}"/>
    /// </iq>
    /// ```
    pub fn build_disable_iq(&self, push_service_jid: &str, node: &str) -> Element {
        let iq_id = Uuid::new_v4().to_string();

        let disable_el = Element::builder("disable", NS_PUSH)
            .attr("jid", push_service_jid)
            .attr("node", node)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", iq_id.as_str())
            .append(disable_el)
            .build()
    }

    /// Build an IQ to disable ALL push subscriptions on the server
    /// (no `jid` or `node` attributes).
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}" xmlns="jabber:client">
    ///   <disable xmlns="urn:xmpp:push:0"/>
    /// </iq>
    /// ```
    pub fn build_disable_all_iq(&self) -> Element {
        let iq_id = Uuid::new_v4().to_string();

        let disable_el = Element::builder("disable", NS_PUSH).build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", iq_id.as_str())
            .append(disable_el)
            .build()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_disable_iq_has_correct_ns() {
        let cleaner = PushCleanup::new();
        let iq = cleaner.build_disable_iq("push.example.com", "node-abc");

        // The <disable> child must carry the XEP-0357 namespace.
        let disable_el = iq
            .children()
            .find(|c| c.name() == "disable")
            .expect("<disable> child not found");

        assert_eq!(disable_el.ns(), NS_PUSH);
    }

    #[test]
    fn build_disable_iq_has_jid_and_node() {
        let cleaner = PushCleanup::new();
        let iq = cleaner.build_disable_iq("push.example.com", "node-abc");

        assert_eq!(iq.attr("type"), Some("set"));

        let disable_el = iq
            .children()
            .find(|c| c.name() == "disable")
            .expect("<disable> child not found");

        assert_eq!(disable_el.attr("jid"), Some("push.example.com"));
        assert_eq!(disable_el.attr("node"), Some("node-abc"));
    }

    #[test]
    fn build_disable_all_iq_has_no_jid() {
        let cleaner = PushCleanup::new();
        let iq = cleaner.build_disable_all_iq();

        assert_eq!(iq.attr("type"), Some("set"));

        let disable_el = iq
            .children()
            .find(|c| c.name() == "disable")
            .expect("<disable> child not found");

        // A "disable all" stanza must not carry jid or node attributes.
        assert_eq!(disable_el.attr("jid"), None);
        assert_eq!(disable_el.attr("node"), None);
    }
}
