// Task P5.2 — XEP-0084 User Avatar + vCard-temp (XEP-0054) avatar support
// XEP references:
//   https://xmpp.org/extensions/xep-0084.html
//   https://xmpp.org/extensions/xep-0054.html
//
// This is a pure state machine — no I/O, no async, no image processing.
// Supports two avatar mechanisms:
//   1. XEP-0084 PubSub-based avatar (modern): publish/subscribe to
//      `urn:xmpp:avatar:data` and `urn:xmpp:avatar:metadata` nodes.
//   2. vCard-temp (XEP-0054): legacy avatar via IQ vCard, used as fallback.

use std::collections::HashMap;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_PUBSUB, NS_VCARD};

const NS_AVATAR_DATA: &str = "urn:xmpp:avatar:data";
const NS_AVATAR_META: &str = "urn:xmpp:avatar:metadata";
const NS_PUBSUB_EVENT: &str = "http://jabber.org/protocol/pubsub#event";

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Avatar metadata and optional raw image data for a single JID.
#[derive(Debug, Clone, PartialEq)]
pub struct AvatarInfo {
    /// Bare JID of the contact this avatar belongs to.
    pub jid: String,
    /// SHA-1 hash of the image data (hex string), used as the PubSub item ID.
    pub sha1: String,
    /// MIME type, e.g. `"image/png"`.
    pub mime_type: String,
    /// Raw image bytes. Empty when only metadata has been received so far.
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// AvatarManager
// ---------------------------------------------------------------------------

/// Manages avatar metadata/data for a set of JIDs.
///
/// Supports both XEP-0084 PubSub avatars and vCard-temp PHOTO fallback.
/// All methods are synchronous — the caller is responsible for sending/
/// receiving XMPP stanzas and calling the appropriate handler.
pub struct AvatarManager {
    /// jid → AvatarInfo (metadata only, or full data once fetched)
    cache: HashMap<String, AvatarInfo>,
    /// IQ id → JID for in-flight vCard fetch requests.
    pending_vcards: HashMap<String, String>,
}

impl Default for AvatarManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AvatarManager {
    /// Creates an empty manager.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            pending_vcards: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // XEP-0084
    // -----------------------------------------------------------------------

    /// Parse an XEP-0084 metadata PubSub event notification.
    ///
    /// Expected stanza shape:
    /// ```xml
    /// <message from="user@server">
    ///   <event xmlns="http://jabber.org/protocol/pubsub#event">
    ///     <items node="urn:xmpp:avatar:metadata">
    ///       <item id="{sha1}">
    ///         <metadata xmlns="urn:xmpp:avatar:metadata">
    ///           <info bytes="..." id="{sha1}" type="image/png"/>
    ///         </metadata>
    ///       </item>
    ///     </items>
    ///   </event>
    /// </message>
    /// ```
    ///
    /// Stores an `AvatarInfo` with empty `data` (the caller should then call
    /// `build_avatar_data_request` to fetch the actual bytes).
    ///
    /// Returns `None` if the stanza is not a valid avatar metadata event.
    pub fn on_avatar_metadata_event(&mut self, from_jid: &str, el: &Element) -> Option<AvatarInfo> {
        // Find <event xmlns="...pubsub#event">
        let event = el
            .children()
            .find(|c| c.name() == "event" && c.ns() == NS_PUBSUB_EVENT)?;

        // Find <items node="urn:xmpp:avatar:metadata"> (or draft ":2" suffix).
        let items = event.children().find(|c| {
            c.name() == "items"
                && matches!(
                    c.attr("node"),
                    Some("urn:xmpp:avatar:metadata") | Some("urn:xmpp:avatar:metadata:2")
                )
        })?;

        // First <item id="{sha1}">
        let item = items.children().find(|c| c.name() == "item")?;
        let sha1 = item.attr("id")?.to_string();

        // <metadata xmlns="urn:xmpp:avatar:metadata"> (or draft ":2" suffix)
        let metadata = item.children().find(|c| {
            c.name() == "metadata"
                && (c.ns() == NS_AVATAR_META || c.ns() == "urn:xmpp:avatar:metadata:2")
        })?;

        let info_el = metadata.children().find(|c| c.name() == "info")?;
        let mime_type = info_el.attr("type").unwrap_or("image/png").to_string();

        let avatar = AvatarInfo {
            jid: from_jid.to_string(),
            sha1,
            mime_type,
            data: Vec::new(),
        };

        self.cache.insert(from_jid.to_string(), avatar.clone());
        Some(avatar)
    }

    /// Build a PubSub `<iq type="get">` to fetch avatar data for a JID.
    ///
    /// `sha1` is the item ID obtained from the metadata event.
    ///
    /// ```xml
    /// <iq type="get" to="{jid}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <items node="urn:xmpp:avatar:data">
    ///       <item id="{sha1}"/>
    ///     </items>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_avatar_data_request(&self, to_jid: &str, sha1: &str) -> Element {
        let item = Element::builder("item", NS_PUBSUB).attr("id", sha1).build();

        let items = Element::builder("items", NS_PUBSUB)
            .attr("node", NS_AVATAR_DATA)
            .append(item)
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB).append(items).build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("to", to_jid)
            .append(pubsub)
            .build()
    }

    /// Parse a PubSub items result containing avatar data.
    ///
    /// Expected stanza shape:
    /// ```xml
    /// <iq type="result" from="{jid}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <items node="urn:xmpp:avatar:data">
    ///       <item id="{sha1}">
    ///         <data xmlns="urn:xmpp:avatar:data">{base64}</data>
    ///       </item>
    ///     </items>
    ///   </pubsub>
    /// </iq>
    /// ```
    ///
    /// Decodes the base64 payload and updates the cache entry for `from_jid`.
    /// Returns `None` if parsing or base64 decoding fails.
    pub fn on_avatar_data_result(&mut self, from_jid: &str, el: &Element) -> Option<AvatarInfo> {
        let pubsub = el
            .children()
            .find(|c| c.name() == "pubsub" && c.ns() == NS_PUBSUB)?;

        let items = pubsub
            .children()
            .find(|c| c.name() == "items" && c.attr("node") == Some(NS_AVATAR_DATA))?;

        let item = items.children().find(|c| c.name() == "item")?;
        let sha1 = item.attr("id").unwrap_or("").to_string();

        let data_el = item
            .children()
            .find(|c| c.name() == "data" && c.ns() == NS_AVATAR_DATA)?;

        let encoded = data_el.text();
        // Strip whitespace that servers sometimes insert.
        let encoded_clean: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
        let raw = BASE64.decode(encoded_clean.as_bytes()).ok()?;

        // Merge with any cached metadata, or build a new entry.
        let mime_type = self
            .cache
            .get(from_jid)
            .map_or_else(|| "image/png".to_string(), |a| a.mime_type.clone());

        let avatar = AvatarInfo {
            jid: from_jid.to_string(),
            sha1,
            mime_type,
            data: raw,
        };

        self.cache.insert(from_jid.to_string(), avatar.clone());
        Some(avatar)
    }

    // -----------------------------------------------------------------------
    // vCard-temp (XEP-0054)
    // -----------------------------------------------------------------------

    /// Build a vCard IQ `get` request for the given JID.
    ///
    /// Registers the request as pending (iq id → jid) so that
    /// `on_vcard_result` can correlate the response.
    ///
    /// Returns `(iq_id, element)`.
    ///
    /// ```xml
    /// <iq type="get" id="{id}" to="{jid}">
    ///   <vCard xmlns="vcard-temp"/>
    /// </iq>
    /// ```
    pub fn build_vcard_request(&mut self, jid: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();

        self.pending_vcards.insert(id.clone(), jid.to_string());

        let vcard = Element::builder("vCard", NS_VCARD).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", id.as_str())
            .attr("to", jid)
            .append(vcard)
            .build();

        (id, iq)
    }

    /// Parse a vCard result IQ and extract the avatar from `<PHOTO><BINVAL>`.
    ///
    /// Expected stanza shape:
    /// ```xml
    /// <iq type="result" id="{id}">
    ///   <vCard xmlns="vcard-temp">
    ///     <PHOTO>
    ///       <TYPE>image/png</TYPE>
    ///       <BINVAL>{base64}</BINVAL>
    ///     </PHOTO>
    ///   </vCard>
    /// </iq>
    /// ```
    ///
    /// Resolves the JID from `pending_vcards` using the IQ `id`. Updates the
    /// cache and removes the pending entry. Returns `None` if:
    /// - the IQ id is not a pending vCard request,
    /// - no `<PHOTO>/<BINVAL>` is present, or
    /// - base64 decoding fails.
    pub fn on_vcard_result(&mut self, el: &Element) -> Option<AvatarInfo> {
        let iq_id = el.attr("id")?;
        let jid = self.pending_vcards.remove(iq_id)?.to_string();

        let vcard = el
            .children()
            .find(|c| c.name() == "vCard" && c.ns() == NS_VCARD)?;

        let photo = vcard.children().find(|c| c.name() == "PHOTO")?;

        let binval = photo.children().find(|c| c.name() == "BINVAL")?;

        let encoded = binval.text();
        let encoded_clean: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();

        if encoded_clean.is_empty() {
            return None;
        }

        let raw = BASE64.decode(encoded_clean.as_bytes()).ok()?;

        let mime_type = photo
            .children()
            .find(|c| c.name() == "TYPE")
            .map(tokio_xmpp::minidom::Element::text)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "image/png".to_string());

        // Use SHA-1 hex of the data as the item ID (consistent with XEP-0084).
        // We produce a simple hex digest without pulling in a sha1 crate:
        // store an empty string as sha1 — callers can compute it if needed.
        let sha1 = String::new();

        let avatar = AvatarInfo {
            jid: jid.clone(),
            sha1,
            mime_type,
            data: raw,
        };

        self.cache.insert(jid, avatar.clone());
        Some(avatar)
    }

    // -----------------------------------------------------------------------
    // XEP-0084: Publish own avatar
    // -----------------------------------------------------------------------

    /// Build a PubSub publish IQ to publish avatar metadata.
    ///
    /// `sha1` is the SHA-1 hash of the image data (hex string), used as the item ID.
    /// `bytes` is the size of the image in bytes.
    /// `mime_type` is the MIME type (e.g. "image/png").
    ///
    /// ```xml
    /// <iq type="set" to="pubsub.myhost.com" id="{id}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <publish node="urn:xmpp:avatar:metadata">
    ///       <item id="{sha1}">
    ///         <metadata xmlns="urn:xmpp:avatar:metadata">
    ///           <info id="{sha1}" bytes="{size}" type="{mime_type}"/>
    ///         </metadata>
    ///       </item>
    ///     </publish>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_avatar_metadata_publish(
        &self,
        pubsub_jid: &str,
        sha1: &str,
        bytes: usize,
        mime_type: &str,
    ) -> Element {
        let info = Element::builder("info", NS_AVATAR_META)
            .attr("id", sha1)
            .attr("bytes", bytes.to_string())
            .attr("type", mime_type)
            .build();

        let metadata = Element::builder("metadata", NS_AVATAR_META)
            .append(info)
            .build();

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", sha1)
            .append(metadata)
            .build();

        let publish = Element::builder("publish", NS_PUBSUB)
            .attr("node", NS_AVATAR_META)
            .append(item)
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(publish)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("to", pubsub_jid)
            .append(pubsub)
            .build()
    }

    /// Build a PubSub publish IQ to publish avatar image data.
    ///
    /// `sha1` is the SHA-1 hash of the image data (hex string), used as the item ID.
    /// `data` is the raw image bytes.
    /// `mime_type` is the MIME type (e.g. "image/png").
    ///
    /// ```xml
    /// <iq type="set" to="pubsub.myhost.com" id="{id}">
    ///   <pubsub xmlns="http://jabber.org/protocol/pubsub">
    ///     <publish node="urn:xmpp:avatar:data">
    ///       <item id="{sha1}">
    ///         <data xmlns="urn:xmpp:avatar:data">{base64}</data>
    ///       </item>
    ///     </publish>
    ///   </pubsub>
    /// </iq>
    /// ```
    pub fn build_avatar_data_publish(
        &self,
        pubsub_jid: &str,
        sha1: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Element {
        let encoded = BASE64.encode(data);

        let data_el = Element::builder("data", NS_AVATAR_DATA)
            .append(encoded)
            .build();

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", sha1)
            .append(data_el)
            .build();

        let publish = Element::builder("publish", NS_PUBSUB)
            .attr("node", NS_AVATAR_DATA)
            .append(item)
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB)
            .append(publish)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("to", pubsub_jid)
            .append(pubsub)
            .build()
    }

    // -----------------------------------------------------------------------
    // Cache
    // -----------------------------------------------------------------------

    /// Return the cached `AvatarInfo` for a JID, if any.
    #[allow(dead_code)]
    pub fn get(&self, jid: &str) -> Option<&AvatarInfo> {
        self.cache.get(jid)
    }

    /// Insert or replace the avatar for the JID stored in `info.jid`.
    #[allow(dead_code)]
    pub fn set(&mut self, info: AvatarInfo) {
        self.cache.insert(info.jid.clone(), info);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Helpers to build test stanzas as minidom Elements
    // ------------------------------------------------------------------

    fn metadata_event(from: &str, sha1: &str, mime: &str) -> Element {
        // Build from inside out.
        let info = Element::builder("info", NS_AVATAR_META)
            .attr("bytes", "1234")
            .attr("id", sha1)
            .attr("type", mime)
            .build();

        let metadata = Element::builder("metadata", NS_AVATAR_META)
            .append(info)
            .build();

        let item = Element::builder("item", NS_PUBSUB_EVENT)
            .attr("id", sha1)
            .append(metadata)
            .build();

        let items = Element::builder("items", NS_PUBSUB_EVENT)
            .attr("node", NS_AVATAR_META)
            .append(item)
            .build();

        let event = Element::builder("event", NS_PUBSUB_EVENT)
            .append(items)
            .build();

        Element::builder("message", NS_CLIENT)
            .attr("from", from)
            .append(event)
            .build()
    }

    fn data_result(from: &str, sha1: &str, raw: &[u8]) -> Element {
        let encoded = BASE64.encode(raw);

        let data_el = Element::builder("data", NS_AVATAR_DATA)
            .append(encoded.as_str())
            .build();

        let item = Element::builder("item", NS_PUBSUB)
            .attr("id", sha1)
            .append(data_el)
            .build();

        let items = Element::builder("items", NS_PUBSUB)
            .attr("node", NS_AVATAR_DATA)
            .append(item)
            .build();

        let pubsub = Element::builder("pubsub", NS_PUBSUB).append(items).build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("from", from)
            .append(pubsub)
            .build()
    }

    fn vcard_result(iq_id: &str, mime: &str, raw: &[u8]) -> Element {
        let encoded = BASE64.encode(raw);

        let type_el = Element::builder("TYPE", NS_VCARD).append(mime).build();

        let binval = Element::builder("BINVAL", NS_VCARD)
            .append(encoded.as_str())
            .build();

        let photo = Element::builder("PHOTO", NS_VCARD)
            .append(type_el)
            .append(binval)
            .build();

        let vcard = Element::builder("vCard", NS_VCARD).append(photo).build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", iq_id)
            .append(vcard)
            .build()
    }

    fn vcard_result_no_photo(iq_id: &str) -> Element {
        let vcard = Element::builder("vCard", NS_VCARD).build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", iq_id)
            .append(vcard)
            .build()
    }

    // ------------------------------------------------------------------
    // Test 1: on_avatar_metadata_event extracts sha1 and mime type
    // ------------------------------------------------------------------
    #[test]
    fn on_avatar_metadata_event_extracts_sha1_and_mime() {
        let mut mgr = AvatarManager::new();
        let from = "alice@example.com";
        let sha1 = "aabbccdd1122334455667788";
        let mime = "image/jpeg";

        let el = metadata_event(from, sha1, mime);
        let result = mgr.on_avatar_metadata_event(from, &el);

        assert!(result.is_some(), "should return Some(AvatarInfo)");
        let info = result.unwrap();
        assert_eq!(info.jid, from);
        assert_eq!(info.sha1, sha1);
        assert_eq!(info.mime_type, mime);
        assert!(info.data.is_empty(), "data should be empty until fetched");
    }

    // ------------------------------------------------------------------
    // Test 2: build_avatar_data_request has correct node attribute
    // ------------------------------------------------------------------
    #[test]
    fn build_avatar_data_request_has_correct_node() {
        let mgr = AvatarManager::new();
        let jid = "bob@example.com";
        let sha1 = "deadbeef";

        let iq = mgr.build_avatar_data_request(jid, sha1);

        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("to"), Some(jid));

        let pubsub = iq
            .children()
            .find(|c| c.name() == "pubsub")
            .expect("<pubsub> missing");

        let items = pubsub
            .children()
            .find(|c| c.name() == "items")
            .expect("<items> missing");

        assert_eq!(
            items.attr("node"),
            Some(NS_AVATAR_DATA),
            "items node must be NS_AVATAR_DATA"
        );

        let item = items
            .children()
            .find(|c| c.name() == "item")
            .expect("<item> missing");

        assert_eq!(item.attr("id"), Some(sha1));
    }

    // ------------------------------------------------------------------
    // Test 3: on_avatar_data_result decodes base64 correctly
    // ------------------------------------------------------------------
    #[test]
    fn on_avatar_data_result_decodes_base64() {
        let mut mgr = AvatarManager::new();
        let from = "carol@example.com";
        let sha1 = "cafebabe";
        let raw = b"\x89PNG\r\n\x1a\n";

        let el = data_result(from, sha1, raw);
        let result = mgr.on_avatar_data_result(from, &el);

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.jid, from);
        assert_eq!(info.sha1, sha1);
        assert_eq!(info.data, raw);
    }

    // ------------------------------------------------------------------
    // Test 4: build_vcard_request registers as pending
    // ------------------------------------------------------------------
    #[test]
    fn build_vcard_request_registers_pending() {
        let mut mgr = AvatarManager::new();
        let jid = "dave@example.com";

        let (id, iq) = mgr.build_vcard_request(jid);

        // IQ stanza shape
        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("to"), Some(jid));
        assert_eq!(iq.attr("id"), Some(id.as_str()));

        let vcard = iq
            .children()
            .find(|c| c.name() == "vCard" && c.ns() == NS_VCARD)
            .expect("<vCard> missing");
        let _ = vcard; // shape confirmed

        // Pending entry must be registered
        assert!(
            mgr.pending_vcards.contains_key(&id),
            "pending_vcards must contain the generated id"
        );
        assert_eq!(mgr.pending_vcards[&id], jid);
    }

    // ------------------------------------------------------------------
    // Test 5: on_vcard_result extracts PHOTO/BINVAL
    // ------------------------------------------------------------------
    #[test]
    fn on_vcard_result_extracts_photo() {
        let mut mgr = AvatarManager::new();
        let jid = "eve@example.com";
        let raw = b"fake-image-data";

        let (id, _) = mgr.build_vcard_request(jid);
        let el = vcard_result(&id, "image/png", raw);

        let result = mgr.on_vcard_result(&el);

        assert!(result.is_some());
        let info = result.unwrap();
        assert_eq!(info.jid, jid);
        assert_eq!(info.mime_type, "image/png");
        assert_eq!(info.data, raw);
    }

    // ------------------------------------------------------------------
    // Test 6: get() returns stored avatar from cache
    // ------------------------------------------------------------------
    #[test]
    fn cache_get_returns_stored_avatar() {
        let mut mgr = AvatarManager::new();
        let avatar = AvatarInfo {
            jid: "frank@example.com".to_string(),
            sha1: "abc123".to_string(),
            mime_type: "image/webp".to_string(),
            data: vec![1, 2, 3],
        };

        mgr.set(avatar.clone());
        let retrieved = mgr.get("frank@example.com");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &avatar);
    }

    // ------------------------------------------------------------------
    // Test 7: set() overwrites an existing cache entry
    // ------------------------------------------------------------------
    #[test]
    fn set_overwrites_existing() {
        let mut mgr = AvatarManager::new();
        let jid = "grace@example.com";

        mgr.set(AvatarInfo {
            jid: jid.to_string(),
            sha1: "old-sha1".to_string(),
            mime_type: "image/png".to_string(),
            data: vec![0xFF],
        });

        mgr.set(AvatarInfo {
            jid: jid.to_string(),
            sha1: "new-sha1".to_string(),
            mime_type: "image/jpeg".to_string(),
            data: vec![0xAB, 0xCD],
        });

        let info = mgr.get(jid).expect("avatar should be cached");
        assert_eq!(info.sha1, "new-sha1");
        assert_eq!(info.mime_type, "image/jpeg");
        assert_eq!(info.data, vec![0xAB, 0xCD]);
    }

    // ------------------------------------------------------------------
    // Test 8: on_vcard_result with no PHOTO element returns None
    // ------------------------------------------------------------------
    #[test]
    fn on_vcard_result_no_photo_returns_none() {
        let mut mgr = AvatarManager::new();
        let jid = "henry@example.com";

        let (id, _) = mgr.build_vcard_request(jid);
        let el = vcard_result_no_photo(&id);

        let result = mgr.on_vcard_result(&el);

        assert!(result.is_none(), "should return None when no PHOTO present");
        // The pending entry should have been consumed regardless.
        assert!(
            !mgr.pending_vcards.contains_key(&id),
            "pending entry should be removed even on failure"
        );
    }
}
