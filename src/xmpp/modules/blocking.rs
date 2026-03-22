// Task P6.2 — XEP-0191 Blocking Command
// XEP reference: https://xmpp.org/extensions/xep-0191.html
//
// Pure state machine — no I/O, no async.
// Builds block/unblock IQs and processes blocklist results and push stanzas.

use std::collections::HashSet;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

const NS_BLOCKING: &str = "urn:xmpp:blocking";
const NS_CLIENT: &str = "jabber:client";

// ---------------------------------------------------------------------------
// BlockingManager
// ---------------------------------------------------------------------------

/// XEP-0191 Blocking Command state manager.
///
/// Tracks the set of currently blocked JIDs, builds outbound IQs, and
/// processes inbound blocklist results and server-push stanzas.
///
/// All methods are pure: no I/O, no async.
pub struct BlockingManager {
    /// Currently blocked JIDs.
    blocked: HashSet<String>,
}

impl BlockingManager {
    /// Create a new manager with an empty block list.
    pub fn new() -> Self {
        Self {
            blocked: HashSet::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Outbound IQ builders
    // -----------------------------------------------------------------------

    /// Build a blocklist fetch IQ.
    ///
    /// ```xml
    /// <iq type="get" id="{uuid}">
    ///   <blocklist xmlns="urn:xmpp:blocking"/>
    /// </iq>
    /// ```
    pub fn build_fetch_iq(&self) -> Element {
        let id = Uuid::new_v4().to_string();
        let blocklist = Element::builder("blocklist", NS_BLOCKING).build();
        Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", &id)
            .append(blocklist)
            .build()
    }

    /// Build a block IQ for one or more JIDs.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}">
    ///   <block xmlns="urn:xmpp:blocking">
    ///     <item jid="troll@server"/>
    ///   </block>
    /// </iq>
    /// ```
    pub fn build_block_iq(&mut self, jids: &[&str]) -> Element {
        let id = Uuid::new_v4().to_string();
        let mut block_builder = Element::builder("block", NS_BLOCKING);
        for jid in jids {
            let item = Element::builder("item", NS_BLOCKING)
                .attr("jid", *jid)
                .build();
            block_builder = block_builder.append(item);
        }
        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(block_builder.build())
            .build()
    }

    /// Build an unblock IQ for one or more JIDs.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}">
    ///   <unblock xmlns="urn:xmpp:blocking">
    ///     <item jid="troll@server"/>
    ///   </unblock>
    /// </iq>
    /// ```
    pub fn build_unblock_iq(&mut self, jids: &[&str]) -> Element {
        let id = Uuid::new_v4().to_string();
        let mut unblock_builder = Element::builder("unblock", NS_BLOCKING);
        for jid in jids {
            let item = Element::builder("item", NS_BLOCKING)
                .attr("jid", *jid)
                .build();
            unblock_builder = unblock_builder.append(item);
        }
        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(unblock_builder.build())
            .build()
    }

    // -----------------------------------------------------------------------
    // Inbound IQ / push handlers
    // -----------------------------------------------------------------------

    /// Parse the blocklist result IQ. Replaces the internal set entirely.
    ///
    /// Expected format:
    /// ```xml
    /// <iq type="result">
    ///   <blocklist xmlns="urn:xmpp:blocking">
    ///     <item jid="spam@server"/>
    ///   </blocklist>
    /// </iq>
    /// ```
    pub fn on_blocklist_result(&mut self, el: &Element) {
        if el.attr("type") != Some("result") {
            return;
        }
        let Some(blocklist) = el
            .children()
            .find(|c| c.name() == "blocklist" && c.ns() == NS_BLOCKING)
        else {
            return;
        };

        self.blocked.clear();
        for item in blocklist.children().filter(|c| c.name() == "item") {
            if let Some(jid) = item.attr("jid") {
                self.blocked.insert(jid.to_string());
            }
        }
    }

    /// Parse an incoming block push (server notifying us of a new block).
    ///
    /// Expected format:
    /// ```xml
    /// <iq type="set">
    ///   <block xmlns="urn:xmpp:blocking">
    ///     <item jid="troll@server"/>
    ///   </block>
    /// </iq>
    /// ```
    pub fn on_block_push(&mut self, el: &Element) {
        if el.attr("type") != Some("set") {
            return;
        }
        let Some(block) = el
            .children()
            .find(|c| c.name() == "block" && c.ns() == NS_BLOCKING)
        else {
            return;
        };

        for item in block.children().filter(|c| c.name() == "item") {
            if let Some(jid) = item.attr("jid") {
                self.blocked.insert(jid.to_string());
            }
        }
    }

    /// Parse an incoming unblock push.
    ///
    /// Expected format:
    /// ```xml
    /// <iq type="set">
    ///   <unblock xmlns="urn:xmpp:blocking">
    ///     <item jid="troll@server"/>
    ///   </unblock>
    /// </iq>
    /// ```
    ///
    /// An `<unblock>` with no `<item>` children means "unblock all".
    pub fn on_unblock_push(&mut self, el: &Element) {
        if el.attr("type") != Some("set") {
            return;
        }
        let Some(unblock) = el
            .children()
            .find(|c| c.name() == "unblock" && c.ns() == NS_BLOCKING)
        else {
            return;
        };

        let items: Vec<&str> = unblock
            .children()
            .filter(|c| c.name() == "item")
            .filter_map(|c| c.attr("jid"))
            .collect();

        if items.is_empty() {
            // No items = unblock all (XEP-0191 §3.3).
            self.blocked.clear();
        } else {
            for jid in items {
                self.blocked.remove(jid);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Return `true` if `jid` is in the block list.
    pub fn is_blocked(&self, jid: &str) -> bool {
        self.blocked.contains(jid)
    }

    /// Return a sorted list of all blocked JIDs.
    pub fn blocked_list(&self) -> Vec<String> {
        let mut list: Vec<String> = self.blocked.iter().cloned().collect();
        list.sort();
        list
    }
}

impl Default for BlockingManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1 -----------------------------------------------------------------------
    #[test]
    fn build_block_iq_has_ns() {
        let mut mgr = BlockingManager::new();
        let el = mgr.build_block_iq(&["troll@example.org"]);

        assert_eq!(el.attr("type"), Some("set"));
        let block = el
            .children()
            .find(|c| c.name() == "block")
            .expect("no block child");
        assert_eq!(block.ns(), NS_BLOCKING);

        let item = block
            .children()
            .find(|c| c.name() == "item")
            .expect("no item child");
        assert_eq!(item.attr("jid"), Some("troll@example.org"));
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn on_blocklist_result_populates_set() {
        let mut mgr = BlockingManager::new();

        let item1 = Element::builder("item", NS_BLOCKING)
            .attr("jid", "spam@example.org")
            .build();
        let item2 = Element::builder("item", NS_BLOCKING)
            .attr("jid", "troll@example.org")
            .build();
        let blocklist = Element::builder("blocklist", NS_BLOCKING)
            .append(item1)
            .append(item2)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", "fetch-1")
            .append(blocklist)
            .build();

        mgr.on_blocklist_result(&iq);

        assert!(mgr.is_blocked("spam@example.org"));
        assert!(mgr.is_blocked("troll@example.org"));
        assert_eq!(mgr.blocked_list().len(), 2);
    }

    // 3 -----------------------------------------------------------------------
    #[test]
    fn is_blocked_true_after_block() {
        let mut mgr = BlockingManager::new();

        // Simulate receiving a block push from the server.
        let item = Element::builder("item", NS_BLOCKING)
            .attr("jid", "badactor@example.org")
            .build();
        let block = Element::builder("block", NS_BLOCKING).append(item).build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", "push-1")
            .append(block)
            .build();

        mgr.on_block_push(&iq);
        assert!(mgr.is_blocked("badactor@example.org"));
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn on_unblock_push_removes_entry() {
        let mut mgr = BlockingManager::new();

        // Seed the block list directly.
        mgr.blocked.insert("troll@example.org".to_string());
        mgr.blocked.insert("spam@example.org".to_string());

        let item = Element::builder("item", NS_BLOCKING)
            .attr("jid", "troll@example.org")
            .build();
        let unblock = Element::builder("unblock", NS_BLOCKING)
            .append(item)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", "push-2")
            .append(unblock)
            .build();

        mgr.on_unblock_push(&iq);

        assert!(!mgr.is_blocked("troll@example.org"));
        // Other entries must remain.
        assert!(mgr.is_blocked("spam@example.org"));
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn blocked_list_sorted() {
        let mut mgr = BlockingManager::new();
        mgr.blocked.insert("charlie@example.org".to_string());
        mgr.blocked.insert("alice@example.org".to_string());
        mgr.blocked.insert("bob@example.org".to_string());

        let list = mgr.blocked_list();
        assert_eq!(
            list,
            vec![
                "alice@example.org",
                "bob@example.org",
                "charlie@example.org"
            ]
        );
    }
}
