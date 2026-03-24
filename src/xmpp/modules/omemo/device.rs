// OMEMO device list management (XEP-0384)
//
// Pure state machine — no I/O, no async.
// Builds and parses PEP device list stanzas.
// The engine feeds parsed results into OmemoStore asynchronously.

use tokio_xmpp::minidom::Element;

const NS_OMEMO: &str = "urn:xmpp:omemo:2";
const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_CLIENT: &str = "jabber:client";

/// XEP-0384 device list node name.
pub const OMEMO_DEVICES_NODE: &str = "urn:xmpp:omemo:2:devices";
/// XEP-0384 bundle node prefix — append `/{device_id}`.
pub const OMEMO_BUNDLE_NODE_PREFIX: &str = "urn:xmpp:omemo:2:bundles";

// ---------------------------------------------------------------------------
// DeviceManager
// ---------------------------------------------------------------------------

/// Manages the local device ID and builds/parses PEP device list stanzas.
///
/// This is a pure state machine: all network I/O is performed by the engine.
#[derive(Debug, Default)]
pub struct DeviceManager {
    /// Own device ID for the active session. Zero means not yet initialized.
    own_device_id: u32,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the own device ID (called after key generation in OmemoManager::enable).
    pub fn set_own_device_id(&mut self, id: u32) {
        self.own_device_id = id;
    }

    /// Returns the own device ID. Zero if OMEMO has not been enabled.
    pub fn own_device_id(&self) -> u32 {
        self.own_device_id
    }

    // -----------------------------------------------------------------------
    // Stanza builders
    // -----------------------------------------------------------------------

    /// Build a PubSub publish IQ for the device list.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <publish node="urn:xmpp:omemo:2:devices">
    ///       <item id="current">
    ///         <devices xmlns="urn:xmpp:omemo:2">
    ///           <device id="12345"/>
    ///           ...
    ///         </devices>
    ///       </item>
    ///     </publish>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_device_list_publish(&self, devices: &[u32]) -> Element {
        let id = uuid::Uuid::new_v4().to_string();

        let mut devices_el = Element::builder("devices", NS_OMEMO).build();
        for &device_id in devices {
            let dev_el = Element::builder("device", NS_OMEMO)
                .attr("id", device_id.to_string())
                .build();
            devices_el.append_child(dev_el);
        }

        let item_el = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(devices_el)
            .build();

        let publish_el = Element::builder("publish", NS_PUBSUB)
            .attr("node", OMEMO_DEVICES_NODE)
            .append(item_el)
            .build();

        let pubsub_el = Element::builder("pubsub", NS_PUBSUB)
            .append(publish_el)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", id)
            .append(pubsub_el)
            .build()
    }

    /// Build a PubSub fetch IQ to request the device list for `peer_jid`.
    ///
    /// ```xml
    /// <iq type="get" to="{peer_jid}" id="{uuid}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <items node="urn:xmpp:omemo:2:devices"/>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_device_list_fetch(&self, peer_jid: &str) -> (String, Element) {
        let id = uuid::Uuid::new_v4().to_string();

        let items_el = Element::builder("items", NS_PUBSUB)
            .attr("node", OMEMO_DEVICES_NODE)
            .build();

        let pubsub_el = Element::builder("pubsub", NS_PUBSUB)
            .append(items_el)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", peer_jid)
            .attr("id", &id)
            .append(pubsub_el)
            .build();

        (id, iq)
    }

    /// Build a PubSub fetch IQ to request the pre-key bundle for a specific device.
    ///
    /// ```xml
    /// <iq type="get" to="{peer_jid}" id="{uuid}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <items node="urn:xmpp:omemo:2:bundles/{device_id}"/>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_bundle_fetch(&self, peer_jid: &str, device_id: u32) -> (String, Element) {
        let id = uuid::Uuid::new_v4().to_string();
        let node = format!("{}/{}", OMEMO_BUNDLE_NODE_PREFIX, device_id);

        let items_el = Element::builder("items", NS_PUBSUB)
            .attr("node", node)
            .build();

        let pubsub_el = Element::builder("pubsub", NS_PUBSUB)
            .append(items_el)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", peer_jid)
            .attr("id", &id)
            .append(pubsub_el)
            .build();

        (id, iq)
    }

    // -----------------------------------------------------------------------
    // Stanza parsers
    // -----------------------------------------------------------------------

    /// Parse a `<devices xmlns="urn:xmpp:omemo:2">` element and return device IDs.
    ///
    /// Accepts the element whether it arrives:
    /// - directly as an IQ result child, or
    /// - wrapped in a PubSub `<items>/<item>` hierarchy (PEP push).
    ///
    /// Returns an empty Vec on parse failure rather than an error.
    pub fn parse_device_list(element: &Element) -> Vec<u32> {
        // Walk down to find the <devices> element.
        let devices_el = find_devices_element(element);

        match devices_el {
            None => vec![],
            Some(el) => el
                .children()
                .filter(|child| child.name() == "device" && child.ns() == NS_OMEMO)
                .filter_map(|child| child.attr("id")?.parse::<u32>().ok())
                .collect(),
        }
    }

    /// Check whether a PEP event message carries an OMEMO device list update.
    /// Returns `Some(jid)` if the `from` JID and device-list node are found.
    pub fn is_device_list_event(message: &Element) -> Option<String> {
        // <message from="..."><event xmlns="...pubsub#event"><items node="...devices">
        let event = message.get_child("event", "http://jabber.org/protocol/pubsub#event")?;
        let items = event.get_child("items", "http://jabber.org/protocol/pubsub#event")?;
        let node = items.attr("node")?;
        if node != OMEMO_DEVICES_NODE {
            return None;
        }
        let from = message.attr("from")?;
        Some(from.to_owned())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Walk an element tree to find the first `<devices xmlns="urn:xmpp:omemo:2">`.
fn find_devices_element(el: &Element) -> Option<&Element> {
    if el.name() == "devices" && el.ns() == NS_OMEMO {
        return Some(el);
    }
    for child in el.children() {
        if let Some(found) = find_devices_element(child) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_devices_element(ids: &[u32]) -> Element {
        let mut el = Element::builder("devices", NS_OMEMO).build();
        for &id in ids {
            el.append_child(
                Element::builder("device", NS_OMEMO)
                    .attr("id", id.to_string())
                    .build(),
            );
        }
        el
    }

    #[test]
    fn parse_device_list_direct() {
        let el = make_devices_element(&[111, 222, 333]);
        let ids = DeviceManager::parse_device_list(&el);
        assert_eq!(ids, vec![111, 222, 333]);
    }

    #[test]
    fn parse_device_list_nested_in_pubsub() {
        // Simulate a PEP push structure:
        // <message><event><items node="..."><item id="current"><devices>...</devices></item>
        let devices_el = make_devices_element(&[42, 99]);

        let item_el = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(devices_el)
            .build();

        let items_el = Element::builder("items", "http://jabber.org/protocol/pubsub#event")
            .attr("node", OMEMO_DEVICES_NODE)
            .append(item_el)
            .build();

        let event_el =
            Element::builder("event", "http://jabber.org/protocol/pubsub#event")
                .append(items_el)
                .build();

        let message_el = Element::builder("message", NS_CLIENT)
            .attr("from", "bob@example.com")
            .append(event_el)
            .build();

        let ids = DeviceManager::parse_device_list(&message_el);
        assert_eq!(ids, vec![42, 99]);
    }

    #[test]
    fn parse_device_list_empty() {
        let el = make_devices_element(&[]);
        let ids = DeviceManager::parse_device_list(&el);
        assert!(ids.is_empty());
    }

    #[test]
    fn parse_device_list_ignores_invalid_ids() {
        let mut el = Element::builder("devices", NS_OMEMO).build();
        // valid
        el.append_child(
            Element::builder("device", NS_OMEMO)
                .attr("id", "123")
                .build(),
        );
        // missing id attr — should be skipped
        el.append_child(Element::builder("device", NS_OMEMO).build());
        // non-numeric id — should be skipped
        el.append_child(
            Element::builder("device", NS_OMEMO)
                .attr("id", "notanumber")
                .build(),
        );

        let ids = DeviceManager::parse_device_list(&el);
        assert_eq!(ids, vec![123]);
    }

    #[test]
    fn build_device_list_publish_roundtrip() {
        let mgr = DeviceManager::new();
        let stanza = mgr.build_device_list_publish(&[11, 22, 33]);
        // Must be an <iq type="set">
        assert_eq!(stanza.name(), "iq");
        assert_eq!(stanza.attr("type"), Some("set"));

        // Walk down to find <devices>
        let ids = DeviceManager::parse_device_list(&stanza);
        assert_eq!(ids, vec![11, 22, 33]);
    }

    #[test]
    fn build_device_list_fetch_has_correct_node() {
        let mgr = DeviceManager::new();
        let (_id, iq) = mgr.build_device_list_fetch("bob@example.com");
        assert_eq!(iq.attr("to"), Some("bob@example.com"));
        assert_eq!(iq.attr("type"), Some("get"));
        // Check the node attribute somewhere in the tree
        let xml_str = String::from(&iq);
        assert!(xml_str.contains(OMEMO_DEVICES_NODE));
    }

    #[test]
    fn build_bundle_fetch_has_correct_node() {
        let mgr = DeviceManager::new();
        let (_id, iq) = mgr.build_bundle_fetch("bob@example.com", 42);
        let xml_str = String::from(&iq);
        assert!(xml_str.contains("urn:xmpp:omemo:2:bundles/42"));
    }

    #[test]
    fn is_device_list_event_detects_push() {
        let items_el =
            Element::builder("items", "http://jabber.org/protocol/pubsub#event")
                .attr("node", OMEMO_DEVICES_NODE)
                .build();
        let event_el =
            Element::builder("event", "http://jabber.org/protocol/pubsub#event")
                .append(items_el)
                .build();
        let message_el = Element::builder("message", NS_CLIENT)
            .attr("from", "alice@example.com")
            .append(event_el)
            .build();

        let result = DeviceManager::is_device_list_event(&message_el);
        assert_eq!(result, Some("alice@example.com".to_string()));
    }

    #[test]
    fn is_device_list_event_ignores_other_nodes() {
        let items_el =
            Element::builder("items", "http://jabber.org/protocol/pubsub#event")
                .attr("node", "some:other:node")
                .build();
        let event_el =
            Element::builder("event", "http://jabber.org/protocol/pubsub#event")
                .append(items_el)
                .build();
        let message_el = Element::builder("message", NS_CLIENT)
            .attr("from", "alice@example.com")
            .append(event_el)
            .build();

        assert!(DeviceManager::is_device_list_event(&message_el).is_none());
    }
}
