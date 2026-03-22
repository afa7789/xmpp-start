// Task P6.4 — Per-room ignore lists (PubSub private storage)
//
// Pure data module — no I/O, no async.
// Each room's ignore list is persisted as a separate PubSub node keyed by
// `xmpp-start:ignore:{room_jid}`.

use std::collections::{HashMap, HashSet};

use tokio_xmpp::minidom::Element;
use tokio_xmpp::minidom::Node;

const IGNORE_NODE_PREFIX: &str = "xmpp-start:ignore:";
const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_CLIENT: &str = "jabber:client";
const NS_IGNORED: &str = "xmpp-start:ignored";

/// Manages per-room ignored-user lists and their PubSub persistence stanzas.
pub struct IgnoreManager {
    /// room_jid → set of ignored user JIDs
    lists: HashMap<String, HashSet<String>>,
}

impl IgnoreManager {
    pub fn new() -> Self {
        Self {
            lists: HashMap::new(),
        }
    }

    /// Add `user_jid` to the ignore list for `room_jid`.  Idempotent.
    pub fn add(&mut self, room_jid: &str, user_jid: &str) {
        self.lists
            .entry(room_jid.to_string())
            .or_default()
            .insert(user_jid.to_string());
    }

    /// Remove `user_jid` from the ignore list for `room_jid`.  No-op if absent.
    pub fn remove(&mut self, room_jid: &str, user_jid: &str) {
        if let Some(set) = self.lists.get_mut(room_jid) {
            set.remove(user_jid);
        }
    }

    /// Returns `true` if `user_jid` is ignored in `room_jid`.
    pub fn is_ignored(&self, room_jid: &str, user_jid: &str) -> bool {
        self.lists
            .get(room_jid)
            .map(|set| set.contains(user_jid))
            .unwrap_or(false)
    }

    /// Returns the current ignore list for `room_jid`, sorted for determinism.
    pub fn list(&self, room_jid: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .lists
            .get(room_jid)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Build a PubSub publish IQ to persist the ignore list for `room_jid`.
    ///
    /// ```xml
    /// <iq type='set'>
    ///   <pubsub xmlns='http://jabber.org/protocol/pubsub'>
    ///     <publish node='xmpp-start:ignore:{room_jid}'>
    ///       <item id='current'>
    ///         <ignored>
    ///           <user jid='troll@server'/>
    ///         </ignored>
    ///       </item>
    ///     </publish>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_publish_iq(&self, room_jid: &str) -> Element {
        let node_name = format!("{}{}", IGNORE_NODE_PREFIX, room_jid);

        let mut ignored_el = Element::builder("ignored", NS_IGNORED).build();

        for user_jid in self.list(room_jid) {
            let user_el = Element::builder("user", NS_IGNORED)
                .attr("jid", user_jid)
                .build();
            ignored_el.append_child(user_el);
        }

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(Node::Element(ignored_el))
            .build();

        let publish = Element::builder("publish", NS_PUBSUB)
            .attr("node", node_name.as_str())
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

    /// Parse a PubSub items result to restore the ignore list for `room_jid`.
    ///
    /// Accepts a bare `<ignored>` element or a full IQ wrapping
    /// `<pubsub><items><item><ignored>…`.
    pub fn parse_result(&mut self, room_jid: &str, el: &Element) {
        let ignored_el = find_ignored_el(el);
        let Some(ignored_el) = ignored_el else {
            return;
        };

        let set = self.lists.entry(room_jid.to_string()).or_default();
        set.clear();

        for child in ignored_el.children() {
            if child.name() == "user" {
                if let Some(jid) = child.attr("jid") {
                    set.insert(jid.to_string());
                }
            }
        }
    }
}

/// Walk the element tree to locate the `<ignored>` payload element.
fn find_ignored_el(el: &Element) -> Option<&Element> {
    if el.name() == "ignored" {
        return Some(el);
    }

    // <iq> → <pubsub> → <items> → <item> → <ignored>
    let pubsub = el.get_child("pubsub", NS_PUBSUB)?;
    let items = pubsub.get_child("items", NS_PUBSUB)?;
    let item = items.get_child("item", NS_PUBSUB)?;
    item.get_child("ignored", NS_IGNORED)
}

impl Default for IgnoreManager {
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

    // 1. add and is_ignored
    #[test]
    fn add_and_is_ignored() {
        let mut mgr = IgnoreManager::new();
        mgr.add("room@muc.server", "troll@server");
        assert!(mgr.is_ignored("room@muc.server", "troll@server"));
        assert!(!mgr.is_ignored("room@muc.server", "friend@server"));
    }

    // 2. remove clears the entry
    #[test]
    fn remove_clears_entry() {
        let mut mgr = IgnoreManager::new();
        mgr.add("room@muc.server", "troll@server");
        mgr.remove("room@muc.server", "troll@server");
        assert!(!mgr.is_ignored("room@muc.server", "troll@server"));
    }

    // 3. is_ignored returns false for an unknown room
    #[test]
    fn is_ignored_false_for_unknown_room() {
        let mgr = IgnoreManager::new();
        assert!(!mgr.is_ignored("unknown@muc.server", "anyone@server"));
    }

    // 4. build_publish_iq contains the user entries
    #[test]
    fn build_publish_iq_contains_user_entries() {
        let mut mgr = IgnoreManager::new();
        mgr.add("room@muc.server", "troll@server");
        mgr.add("room@muc.server", "spammer@server");

        let iq = mgr.build_publish_iq("room@muc.server");

        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("type"), Some("set"));

        let pubsub = iq.get_child("pubsub", NS_PUBSUB);
        assert!(pubsub.is_some(), "<pubsub> missing");

        let publish = pubsub.unwrap().get_child("publish", NS_PUBSUB);
        assert!(publish.is_some(), "<publish> missing");

        let expected_node = format!("{}room@muc.server", IGNORE_NODE_PREFIX);
        assert_eq!(publish.unwrap().attr("node"), Some(expected_node.as_str()));

        let item = publish.unwrap().get_child("item", NS_PUBSUB);
        assert!(item.is_some(), "<item> missing");

        let ignored = item.unwrap().get_child("ignored", NS_IGNORED);
        assert!(ignored.is_some(), "<ignored> missing");

        let users: Vec<_> = ignored.unwrap().children().collect();
        assert_eq!(users.len(), 2);

        let jids: Vec<&str> = users.iter().filter_map(|u| u.attr("jid")).collect();
        assert!(jids.contains(&"troll@server"));
        assert!(jids.contains(&"spammer@server"));
    }

    // 5. parse_result restores the ignore list from a synthetic IQ
    #[test]
    fn parse_result_restores_list() {
        let mut mgr = IgnoreManager::new();

        let mut ignored_el = Element::builder("ignored", NS_IGNORED).build();
        ignored_el.append_child(
            Element::builder("user", NS_IGNORED)
                .attr("jid", "troll@server")
                .build(),
        );

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", "current")
            .append(Node::Element(ignored_el))
            .build();

        let items = Element::builder("items", NS_PUBSUB)
            .attr("node", "xmpp-start:ignore:room@muc.server")
            .append(Node::Element(item))
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(Node::Element(items))
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .append(Node::Element(pubsub))
            .build();

        mgr.parse_result("room@muc.server", &iq);
        assert!(mgr.is_ignored("room@muc.server", "troll@server"));
        assert_eq!(mgr.list("room@muc.server").len(), 1);
    }

    // 6. remove is a no-op for absent user
    #[test]
    fn remove_noop_for_absent() {
        let mut mgr = IgnoreManager::new();
        mgr.add("room@muc.server", "alice@server");
        mgr.remove("room@muc.server", "nobody@server"); // must not panic
        assert_eq!(mgr.list("room@muc.server").len(), 1);
    }
}
