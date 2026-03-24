// Task P6.4 — XEP-0223 Conversation Sync (PubSub private storage)
// XEP reference: https://xmpp.org/extensions/xep-0223.html
//
// Pure data module — no I/O, no async.
// Stores and retrieves the conversation list via PubSub private storage.

use tokio_xmpp::minidom::Element;
use tokio_xmpp::minidom::Node;

use super::{NS_CLIENT, NS_PUBSUB};

const NS_PUBSUB_PRIVATE: &str = "http://jabber.org/protocol/pubsub#private";
const CONV_SYNC_NODE: &str = "xmpp-start:conversations";

/// A single conversation entry stored on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncedConversation {
    /// JID of the conversation peer or room.
    pub jid: String,
    /// Whether this conversation has been archived by the user.
    pub archived: bool,
}

/// Builds and parses PubSub private-storage stanzas for the conversation list.
pub struct ConversationSyncManager;

impl ConversationSyncManager {
    pub fn new() -> Self {
        Self
    }

    /// Build a PubSub publish IQ to save the conversation list.
    ///
    /// ```xml
    /// <iq type='set'>
    ///   <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    ///     <publish node='xmpp-start:conversations'>
    ///       <item id='current'>
    ///         <conversations>
    ///           <conversation jid='alice@server' archived='false'/>
    ///         </conversations>
    ///       </item>
    ///     </publish>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_publish_iq(&self, conversations: &[SyncedConversation]) -> Element {
        let mut conversations_el =
            Element::builder("conversations", "xmpp-start:conversations").build();

        for conv in conversations {
            let conv_el = Element::builder("conversation", "xmpp-start:conversations")
                .attr("jid", conv.jid.clone())
                .attr("archived", if conv.archived { "true" } else { "false" })
                .build();
            conversations_el.append_child(conv_el);
        }

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(Node::Element(conversations_el))
            .build();

        let publish = Element::builder("publish", NS_PUBSUB)
            .attr("node", CONV_SYNC_NODE)
            .append(Node::Element(item))
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(Node::Element(publish))
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .append(Node::Element(pubsub))
            .build()
    }

    /// Build a PubSub items request to fetch the conversation list.
    pub fn build_fetch_iq(&self) -> Element {
        let items = Element::builder("items", NS_PUBSUB)
            .attr("node", CONV_SYNC_NODE)
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(Node::Element(items))
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .append(Node::Element(pubsub))
            .build()
    }

    /// Parse a PubSub items result containing the conversation list.
    ///
    /// Accepts both a bare `<conversations>` element and a full IQ wrapping
    /// `<pubsub><items><item><conversations>…`.
    pub fn parse_result(&self, el: &Element) -> Vec<SyncedConversation> {
        let conversations_el = find_conversations_el(el);
        let Some(conversations_el) = conversations_el else {
            return Vec::new();
        };

        conversations_el
            .children()
            .filter(|c| c.name() == "conversation")
            .map(|c| {
                let jid = c.attr("jid").unwrap_or("").to_string();
                let archived = matches!(c.attr("archived"), Some("true") | Some("1"));
                SyncedConversation { jid, archived }
            })
            .collect()
    }
}

/// Walk the element tree to find the `<conversations>` payload element.
fn find_conversations_el(el: &Element) -> Option<&Element> {
    if el.name() == "conversations" {
        return Some(el);
    }

    // <iq> → <pubsub> → <items> → <item> → <conversations>
    let pubsub = el.get_child("pubsub", NS_PUBSUB)?;
    let items = pubsub.get_child("items", NS_PUBSUB)?;
    let item = items.get_child("item", NS_PUBSUB)?;
    item.get_child("conversations", "xmpp-start:conversations")
}

impl Default for ConversationSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_conversations() -> Vec<SyncedConversation> {
        vec![
            SyncedConversation {
                jid: "alice@server".to_string(),
                archived: false,
            },
            SyncedConversation {
                jid: "room@muc.server".to_string(),
                archived: true,
            },
        ]
    }

    // 1. build_publish_iq contains the expected conversation elements
    #[test]
    fn build_publish_iq_contains_conversations() {
        let mgr = ConversationSyncManager::new();
        let iq = mgr.build_publish_iq(&make_conversations());

        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("type"), Some("set"));

        let pubsub = iq.get_child("pubsub", NS_PUBSUB);
        assert!(pubsub.is_some(), "<pubsub> missing");

        let publish = pubsub.unwrap().get_child("publish", NS_PUBSUB);
        assert!(publish.is_some(), "<publish> missing");
        assert_eq!(publish.unwrap().attr("node"), Some(CONV_SYNC_NODE));

        let item = publish.unwrap().get_child("item", NS_PUBSUB);
        assert!(item.is_some(), "<item> missing");
        assert_eq!(item.unwrap().attr("id"), Some("current"));

        let conversations = item
            .unwrap()
            .get_child("conversations", "xmpp-start:conversations");
        assert!(conversations.is_some(), "<conversations> missing");

        let children: Vec<_> = conversations.unwrap().children().collect();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].attr("jid"), Some("alice@server"));
        assert_eq!(children[1].attr("jid"), Some("room@muc.server"));
    }

    // 2. parse_result extracts JIDs from a synthetic IQ
    #[test]
    fn parse_result_extracts_jids() {
        let mgr = ConversationSyncManager::new();

        // Build a minimal result tree
        let mut convs_el = Element::builder("conversations", "xmpp-start:conversations").build();
        convs_el.append_child(
            Element::builder("conversation", "xmpp-start:conversations")
                .attr("jid", "alice@server")
                .attr("archived", "false")
                .build(),
        );
        convs_el.append_child(
            Element::builder("conversation", "xmpp-start:conversations")
                .attr("jid", "room@muc.server")
                .attr("archived", "true")
                .build(),
        );

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(Node::Element(convs_el))
            .build();

        let items = Element::builder("items", NS_PUBSUB)
            .attr("node", CONV_SYNC_NODE)
            .append(Node::Element(item))
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(Node::Element(items))
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .append(Node::Element(pubsub))
            .build();

        let result = mgr.parse_result(&iq);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].jid, "alice@server");
        assert_eq!(result[1].jid, "room@muc.server");
    }

    // 3. archived flag round-trips through build/parse
    #[test]
    fn archived_flag_round_trips() {
        let mgr = ConversationSyncManager::new();
        let original = make_conversations();

        let iq = mgr.build_publish_iq(&original);

        // Reuse the publish IQ's inner <conversations> element by extracting it
        let pubsub = iq.get_child("pubsub", NS_PUBSUB).unwrap();
        let publish = pubsub.get_child("publish", NS_PUBSUB).unwrap();
        let item = publish.get_child("item", NS_PUBSUB).unwrap();
        let conversations_el = item
            .get_child("conversations", "xmpp-start:conversations")
            .unwrap();

        let result = mgr.parse_result(conversations_el);
        assert_eq!(result.len(), 2);
        assert!(!result[0].archived, "alice should not be archived");
        assert!(result[1].archived, "room should be archived");
    }

    // 4. build_fetch_iq produces a get IQ with the correct node
    #[test]
    fn build_fetch_iq_correct_node() {
        let mgr = ConversationSyncManager::new();
        let iq = mgr.build_fetch_iq();

        assert_eq!(iq.attr("type"), Some("get"));
        let pubsub = iq.get_child("pubsub", NS_PUBSUB).unwrap();
        let items = pubsub.get_child("items", NS_PUBSUB).unwrap();
        assert_eq!(items.attr("node"), Some(CONV_SYNC_NODE));
    }
}
