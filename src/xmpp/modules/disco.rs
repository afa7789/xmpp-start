#![allow(dead_code)]
// Task P6.1 — XEP-0115 Entity Capabilities + XEP-0030 Service Discovery
// XEP references:
//   https://xmpp.org/extensions/xep-0115.html
//   https://xmpp.org/extensions/xep-0030.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - build disco#info and disco#items IQ get requests
//   - parse incoming disco#info / disco#items IQ results
//   - build the <c> caps element for embedding in presence stanzas
//   - cache disco#info results per JID
//   - answer feature-support queries from the cache

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use sha1::{Digest, Sha1};
use tokio_xmpp::minidom::Element;
use uuid::Uuid;

const NS_CAPS: &str = "http://jabber.org/protocol/caps";
const NS_DISCO_INFO: &str = "http://jabber.org/protocol/disco#info";
const NS_DISCO_ITEMS: &str = "http://jabber.org/protocol/disco#items";
const NS_CLIENT: &str = "jabber:client";

// ---------------------------------------------------------------------------
// XEP-0030 domain types
// ---------------------------------------------------------------------------

/// A single identity entry in a disco#info response.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoIdentity {
    /// Broad category, e.g. `"client"`, `"conference"`, `"server"`.
    pub category: String,
    /// Sub-type within the category, e.g. `"pc"`, `"phone"`, `"text"`.
    /// Named `kind` in Rust to avoid collision with the `type` keyword.
    pub kind: String,
    /// Human-readable name, e.g. `"xmpp-start"`.
    pub name: String,
}

/// The full result of a disco#info query.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoInfo {
    pub identities: Vec<DiscoIdentity>,
    /// Feature namespaces advertised by the entity.
    pub features: Vec<String>,
}

/// A single item from a disco#items response.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoItem {
    pub jid: String,
    pub name: Option<String>,
    pub node: Option<String>,
}

// ---------------------------------------------------------------------------
// XEP-0115 caps
// ---------------------------------------------------------------------------

/// Capability advertisement as defined in XEP-0115.
#[derive(Debug, Clone, PartialEq)]
pub struct Caps {
    /// The base URI of the client application, e.g. `"https://example.org/client"`.
    pub node: String,
    /// Base64-encoded hash of the sorted identity+feature string (SHA-1).
    pub ver: String,
    /// Hash algorithm identifier — always `"sha-1"` for XEP-0115 §5.
    pub hash: String,
}

// ---------------------------------------------------------------------------
// DiscoManager
// ---------------------------------------------------------------------------

/// XEP-0115 / XEP-0030 state manager.
///
/// Handles building outbound disco IQ requests, parsing inbound results,
/// caching known disco#info per JID, and computing the XEP-0115 `ver` hash
/// for our own capabilities.
///
/// All methods are pure: no I/O, no async.
pub struct DiscoManager {
    /// Cached disco#info keyed by entity JID (bare or full).
    cache: HashMap<String, DiscoInfo>,
    /// Pending disco#info IQ requests: `iq_id` → `jid`.
    pending_info: HashMap<String, String>,
    /// Pending disco#items IQ requests: `iq_id` → `jid`.
    pending_items: HashMap<String, String>,
    /// Our own caps to embed in outbound presence stanzas.
    own_caps: Caps,
}

impl DiscoManager {
    /// Create a new manager.
    ///
    /// Computes the XEP-0115 `ver` hash from `identities` and `features`
    /// using the algorithm in §5 of XEP-0115:
    ///
    /// 1. For each identity, produce `"category/type/lang/name"` (lang is empty).
    /// 2. Sort identity strings, append `"<"` after each one.
    /// 3. Sort feature strings, append `"<"` after each one.
    /// 4. Concatenate, SHA-1 the UTF-8 bytes, base64-encode.
    pub fn new(node: &str, identities: &[DiscoIdentity], features: &[&str]) -> Self {
        let ver = compute_ver_hash(identities, features);
        let own_caps = Caps {
            node: node.to_string(),
            ver,
            hash: "sha-1".to_string(),
        };
        Self {
            cache: HashMap::new(),
            pending_info: HashMap::new(),
            pending_items: HashMap::new(),
            own_caps,
        }
    }

    /// Build a disco#info IQ get request targeting `to_jid`.
    ///
    /// Registers the IQ id as pending so `on_info_result` can correlate
    /// the response.
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_info_request(&mut self, to_jid: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        let query = Element::builder("query", NS_DISCO_INFO).build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", &id)
            .attr("to", to_jid)
            .append(query)
            .build();
        self.pending_info.insert(id.clone(), to_jid.to_string());
        (id, iq)
    }

    /// Build a disco#items IQ get request targeting `to_jid`.
    ///
    /// Registers the IQ id as pending so `on_items_result` can correlate
    /// the response.
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_items_request(&mut self, to_jid: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        let query = Element::builder("query", NS_DISCO_ITEMS).build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", &id)
            .attr("to", to_jid)
            .append(query)
            .build();
        self.pending_items.insert(id.clone(), to_jid.to_string());
        (id, iq)
    }

    /// Parse an incoming disco#info result IQ.
    ///
    /// If the IQ `id` matches a pending request, removes it from
    /// `pending_info`, parses identities and features from the `<query>`
    /// child, caches the result, and returns `Some((jid, DiscoInfo))`.
    ///
    /// Returns `None` if the element is not a recognised pending result.
    pub fn on_info_result(&mut self, el: &Element) -> Option<(String, DiscoInfo)> {
        let iq_type = el.attr("type")?;
        if iq_type != "result" {
            return None;
        }
        let iq_id = el.attr("id")?;
        let jid = self.pending_info.remove(iq_id)?;

        let query = el
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_DISCO_INFO)?;

        let mut identities = Vec::new();
        let mut features = Vec::new();

        for child in query.children() {
            match child.name() {
                "identity" => {
                    let category = child.attr("category").unwrap_or("").to_string();
                    let kind = child.attr("type").unwrap_or("").to_string();
                    let name = child.attr("name").unwrap_or("").to_string();
                    identities.push(DiscoIdentity {
                        category,
                        kind,
                        name,
                    });
                }
                "feature" => {
                    if let Some(var) = child.attr("var") {
                        features.push(var.to_string());
                    }
                }
                _ => {}
            }
        }

        let info = DiscoInfo {
            identities,
            features,
        };
        self.cache.insert(jid.clone(), info.clone());
        Some((jid, info))
    }

    /// Parse an incoming disco#items result IQ.
    ///
    /// Returns `Some((jid, items))` if the IQ correlates with a pending
    /// request, `None` otherwise.
    pub fn on_items_result(&mut self, el: &Element) -> Option<(String, Vec<DiscoItem>)> {
        let iq_type = el.attr("type")?;
        if iq_type != "result" {
            return None;
        }
        let iq_id = el.attr("id")?;
        let jid = self.pending_items.remove(iq_id)?;

        let query = el
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_DISCO_ITEMS)?;

        let items: Vec<DiscoItem> = query
            .children()
            .filter(|c| c.name() == "item")
            .map(|c| DiscoItem {
                jid: c.attr("jid").unwrap_or("").to_string(),
                name: c.attr("name").map(str::to_string),
                node: c.attr("node").map(str::to_string),
            })
            .collect();

        Some((jid, items))
    }

    /// Build the `<c>` element advertising our own capabilities.
    ///
    /// This element should be appended to every outbound `<presence>` stanza.
    ///
    /// ```xml
    /// <c xmlns="http://jabber.org/protocol/caps"
    ///    hash="sha-1"
    ///    node="{node}"
    ///    ver="{ver}"/>
    /// ```
    pub fn build_caps_element(&self) -> Element {
        Element::builder("c", NS_CAPS)
            .attr("hash", &self.own_caps.hash)
            .attr("node", &self.own_caps.node)
            .attr("ver", &self.own_caps.ver)
            .build()
    }

    /// Return `true` if the cached disco#info for `jid` lists `feature`.
    ///
    /// Returns `false` when the JID is unknown or uncached.
    pub fn supports(&self, jid: &str, feature: &str) -> bool {
        self.cache
            .get(jid)
            .is_some_and(|info| info.features.iter().any(|f| f == feature))
    }

    /// Return a reference to the cached `DiscoInfo` for `jid`, if any.
    pub fn get_cached(&self, jid: &str) -> Option<&DiscoInfo> {
        self.cache.get(jid)
    }
}

// ---------------------------------------------------------------------------
// XEP-0115 §5 ver-hash algorithm
// ---------------------------------------------------------------------------

/// Compute the XEP-0115 `ver` hash.
///
/// Algorithm (§5):
/// 1. For each identity produce `"category/type/lang/name"` (lang always empty).
/// 2. Sort those strings lexicographically, append `"<"` after each.
/// 3. Sort feature strings lexicographically, append `"<"` after each.
/// 4. Concatenate everything, SHA-1 the UTF-8 bytes, base64-encode.
fn compute_ver_hash(identities: &[DiscoIdentity], features: &[&str]) -> String {
    let mut id_strs: Vec<String> = identities
        .iter()
        .map(|id| format!("{}/{}//{}", id.category, id.kind, id.name))
        .collect();
    id_strs.sort();

    let mut feat_strs: Vec<String> = features
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    feat_strs.sort();

    let mut s = String::new();
    for id_str in &id_strs {
        s.push_str(id_str);
        s.push('<');
    }
    for feat in &feat_strs {
        s.push_str(feat);
        s.push('<');
    }

    let hash = Sha1::digest(s.as_bytes());
    BASE64.encode(hash)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> DiscoManager {
        DiscoManager::new(
            "https://example.org/xmpp-start",
            &[DiscoIdentity {
                category: "client".to_string(),
                kind: "pc".to_string(),
                name: "xmpp-start".to_string(),
            }],
            &["urn:xmpp:mam:2", "urn:xmpp:carbons:2"],
        )
    }

    // 1 -----------------------------------------------------------------------
    #[test]
    fn new_computes_ver_hash() {
        let mgr = make_manager();
        // ver must be a non-empty base64 string
        assert!(!mgr.own_caps.ver.is_empty());
        // base64 alphabet: A-Z, a-z, 0-9, +, /, =
        assert!(mgr
            .own_caps
            .ver
            .chars()
            .all(|c| c.is_alphanumeric() || c == '+' || c == '/' || c == '='));
        // SHA-1 produces 20 bytes → base64 is 28 characters
        assert_eq!(mgr.own_caps.ver.len(), 28);
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn build_info_request_registers_pending() {
        let mut mgr = make_manager();
        let (id, el) = mgr.build_info_request("conference.example.org");

        assert!(!id.is_empty());
        assert_eq!(el.attr("type"), Some("get"));
        assert_eq!(el.attr("to"), Some("conference.example.org"));
        assert_eq!(el.attr("id"), Some(id.as_str()));
        assert!(mgr.pending_info.contains_key(&id));
    }

    // 3 -----------------------------------------------------------------------
    #[test]
    fn build_caps_element_has_correct_ns() {
        let mgr = make_manager();
        let caps = mgr.build_caps_element();

        assert_eq!(caps.ns(), NS_CAPS);
        assert_eq!(caps.name(), "c");
        assert_eq!(caps.attr("hash"), Some("sha-1"));
        assert_eq!(caps.attr("node"), Some("https://example.org/xmpp-start"));
        assert!(caps.attr("ver").is_some());
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn on_info_result_parses_features() {
        let mut mgr = make_manager();
        let (id, _) = mgr.build_info_request("server.example.org");

        let identity_el = Element::builder("identity", NS_DISCO_INFO)
            .attr("category", "server")
            .attr("type", "im")
            .attr("name", "Prosody")
            .build();
        let feat1 = Element::builder("feature", NS_DISCO_INFO)
            .attr("var", "urn:xmpp:mam:2")
            .build();
        let feat2 = Element::builder("feature", NS_DISCO_INFO)
            .attr("var", "urn:xmpp:carbons:2")
            .build();
        let query = Element::builder("query", NS_DISCO_INFO)
            .append(identity_el)
            .append(feat1)
            .append(feat2)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(query)
            .build();

        let result = mgr.on_info_result(&iq);
        assert!(result.is_some());
        let (jid, info) = result.unwrap();
        assert_eq!(jid, "server.example.org");
        assert!(info.features.contains(&"urn:xmpp:mam:2".to_string()));
        assert!(info.features.contains(&"urn:xmpp:carbons:2".to_string()));
        assert_eq!(info.identities.len(), 1);
        assert_eq!(info.identities[0].category, "server");
        assert_eq!(info.identities[0].kind, "im");
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn on_info_result_caches_result() {
        let mut mgr = make_manager();
        let (id, _) = mgr.build_info_request("server.example.org");

        let feat = Element::builder("feature", NS_DISCO_INFO)
            .attr("var", "urn:xmpp:ping")
            .build();
        let query = Element::builder("query", NS_DISCO_INFO)
            .append(feat)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(query)
            .build();

        mgr.on_info_result(&iq);
        assert!(mgr.get_cached("server.example.org").is_some());
        // pending entry must have been consumed
        assert!(!mgr.pending_info.contains_key(&id));
    }

    // 6 -----------------------------------------------------------------------
    #[test]
    fn supports_returns_true_for_cached_feature() {
        let mut mgr = make_manager();
        let (id, _) = mgr.build_info_request("server.example.org");

        let feat = Element::builder("feature", NS_DISCO_INFO)
            .attr("var", "urn:xmpp:ping")
            .build();
        let query = Element::builder("query", NS_DISCO_INFO)
            .append(feat)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(query)
            .build();

        mgr.on_info_result(&iq);
        assert!(mgr.supports("server.example.org", "urn:xmpp:ping"));
    }

    // 7 -----------------------------------------------------------------------
    #[test]
    fn supports_returns_false_for_unknown_jid() {
        let mgr = make_manager();
        assert!(!mgr.supports("nobody@example.org", "urn:xmpp:ping"));
    }

    // 8 -----------------------------------------------------------------------
    #[test]
    fn on_items_result_parses_items() {
        let mut mgr = make_manager();
        let (id, _) = mgr.build_items_request("example.org");

        let item1 = Element::builder("item", NS_DISCO_ITEMS)
            .attr("jid", "conference.example.org")
            .attr("name", "Chatrooms")
            .build();
        let item2 = Element::builder("item", NS_DISCO_ITEMS)
            .attr("jid", "upload.example.org")
            .attr("name", "File Upload")
            .attr("node", "upload")
            .build();
        let query = Element::builder("query", NS_DISCO_ITEMS)
            .append(item1)
            .append(item2)
            .build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(query)
            .build();

        let result = mgr.on_items_result(&iq);
        assert!(result.is_some());
        let (jid, items) = result.unwrap();
        assert_eq!(jid, "example.org");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].jid, "conference.example.org");
        assert_eq!(items[0].name, Some("Chatrooms".to_string()));
        assert_eq!(items[0].node, None);
        assert_eq!(items[1].jid, "upload.example.org");
        assert_eq!(items[1].node, Some("upload".to_string()));
        // pending entry consumed
        assert!(!mgr.pending_items.contains_key(&id));
    }
}
