#![allow(dead_code)]
// Task P6.4 — XEP-0202 Entity Time
// XEP reference: https://xmpp.org/extensions/xep-0202.html
//
// Pure state machine — no I/O, no async.
// Builds time request IQs and parses result IQs containing <tzo>/<utc>.

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use tokio_xmpp::minidom::Node;
use uuid::Uuid;

const NS_TIME: &str = "urn:xmpp:time";
const NS_CLIENT: &str = "jabber:client";

/// Cached entity time information for a remote JID.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityTime {
    /// The JID this entry belongs to.
    pub jid: String,
    /// UTC offset in seconds (e.g. -10800 for UTC-3).
    pub utc_offset_seconds: i32,
    /// IANA timezone string if provided (e.g. "America/Sao_Paulo").
    pub tz_name: Option<String>,
}

/// Manages outgoing entity-time requests and the resulting cache.
pub struct EntityTimeManager {
    /// jid → EntityTime cache
    cache: HashMap<String, EntityTime>,
    /// Pending IQ requests: iq_id → jid
    pending: HashMap<String, String>,
}

impl EntityTimeManager {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    /// Build a `<iq type='get'>` request for the entity time of `jid`.
    ///
    /// Returns `(iq_id, element)`. The caller must transmit the element and
    /// later pass the response to [`on_result`].
    pub fn build_request(&mut self, jid: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        self.pending.insert(id.clone(), jid.to_string());

        let time_el = Element::builder("time", NS_TIME).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", id.clone())
            .attr("to", jid)
            .append(Node::Element(time_el))
            .build();

        (id, iq)
    }

    /// Process a `<iq type='result'>` response element.
    ///
    /// Returns `Some(EntityTime)` if the stanza is a recognised time result
    /// and we had a matching pending request for it. Also inserts the result
    /// into the cache.
    pub fn on_result(&mut self, el: &Element) -> Option<EntityTime> {
        if el.name() != "iq" || el.attr("type") != Some("result") {
            return None;
        }

        let id = el.attr("id")?;
        let jid = self.pending.remove(id)?;

        let time_el = el.get_child("time", NS_TIME)?;
        let tzo_text = time_el.get_child("tzo", NS_TIME)?.text();
        let tz_name = time_el
            .get_child("tz", NS_TIME)
            .map(tokio_xmpp::minidom::Element::text)
            .filter(|s| !s.is_empty());

        let utc_offset_seconds = parse_tzo(&tzo_text)?;

        let entry = EntityTime {
            jid: jid.clone(),
            utc_offset_seconds,
            tz_name,
        };

        self.cache.insert(jid, entry.clone());
        Some(entry)
    }

    /// Look up the cached `EntityTime` for a JID.
    pub fn get(&self, jid: &str) -> Option<&EntityTime> {
        self.cache.get(jid)
    }
}

impl Default for EntityTimeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a `<tzo>` value such as `"-03:00"` or `"+05:30"` into seconds.
///
/// Format: `(+|-)HH:MM`. Returns `None` if the string does not match.
fn parse_tzo(tzo: &str) -> Option<i32> {
    let tzo = tzo.trim();
    if tzo.is_empty() {
        return None;
    }

    let (sign, rest) = if let Some(s) = tzo.strip_prefix('-') {
        (-1i32, s)
    } else if let Some(s) = tzo.strip_prefix('+') {
        (1i32, s)
    } else {
        return None;
    };

    let mut parts = rest.splitn(2, ':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next()?.parse().ok()?;

    Some(sign * (hours * 3600 + minutes * 60))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. build_request registers a pending entry
    #[test]
    fn build_request_registers_pending() {
        let mut mgr = EntityTimeManager::new();
        let (id, iq) = mgr.build_request("alice@example.com");

        // The returned ID must be registered in pending
        assert!(mgr.pending.contains_key(&id));
        assert_eq!(mgr.pending[&id], "alice@example.com");

        // Stanza must be a get IQ addressed to the target JID
        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("to"), Some("alice@example.com"));
        assert_eq!(iq.attr("id"), Some(id.as_str()));
    }

    // 2. on_result parses a <tzo> offset correctly
    #[test]
    fn on_result_parses_tzo_offset() {
        let mut mgr = EntityTimeManager::new();
        let (id, _) = mgr.build_request("bob@example.com");

        // Build a synthetic result IQ
        let mut tzo_el = Element::builder("tzo", NS_TIME).build();
        tzo_el.append_text_node("-03:00");

        let mut utc_el = Element::builder("utc", NS_TIME).build();
        utc_el.append_text_node("2026-03-21T18:00:00Z");

        let mut time_el = Element::builder("time", NS_TIME).build();
        time_el.append_child(tzo_el);
        time_el.append_child(utc_el);

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", id.as_str())
            .attr("from", "bob@example.com")
            .append(Node::Element(time_el))
            .build();

        let result = mgr.on_result(&iq);
        assert!(result.is_some());
        let et = result.unwrap();
        assert_eq!(et.utc_offset_seconds, -10800);
        assert_eq!(et.jid, "bob@example.com");
    }

    // 3. on_result caches the entry
    #[test]
    fn on_result_caches_entry() {
        let mut mgr = EntityTimeManager::new();
        let (id, _) = mgr.build_request("carol@example.com");

        let mut tzo_el = Element::builder("tzo", NS_TIME).build();
        tzo_el.append_text_node("+05:30");

        let mut time_el = Element::builder("time", NS_TIME).build();
        time_el.append_child(tzo_el);

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", id.as_str())
            .attr("from", "carol@example.com")
            .append(Node::Element(time_el))
            .build();

        mgr.on_result(&iq);

        let cached = mgr.get("carol@example.com");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().utc_offset_seconds, 19800); // 5*3600 + 30*60
    }

    // 4. get returns None for unknown JID
    #[test]
    fn get_returns_none_for_unknown() {
        let mgr = EntityTimeManager::new();
        assert!(mgr.get("nobody@example.com").is_none());
    }

    // 5. parse_tzo handles UTC (+00:00)
    #[test]
    fn parse_tzo_utc() {
        assert_eq!(parse_tzo("+00:00"), Some(0));
    }

    // 6. parse_tzo rejects malformed input
    #[test]
    fn parse_tzo_invalid() {
        assert!(parse_tzo("not-a-tzo").is_none());
        assert!(parse_tzo("").is_none());
    }
}
