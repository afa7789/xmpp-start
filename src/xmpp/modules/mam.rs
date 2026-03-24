#![allow(dead_code)]
// Task P4.1 — XEP-0313 Message Archive Management + XEP-0059 Result Set Management
// XEP references:
//   https://xmpp.org/extensions/xep-0313.html
//   https://xmpp.org/extensions/xep-0059.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - build MAM query IQ stanzas (with optional RSM pagination)
//   - parse incoming <message> wrappers carrying archived stanzas
//   - parse the <fin> IQ that signals end-of-page and returns RSM metadata

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_DATA, NS_FORWARD, NS_MAM};

const NS_RSM: &str = "http://jabber.org/protocol/rsm";
const NS_DELAY: &str = "urn:ietf:params:xml:ns:xmpp-delay";

// ---------------------------------------------------------------------------
// XEP-0059 Result Set Management
// ---------------------------------------------------------------------------

/// XEP-0059 RSM metadata returned with a page of results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsmSet {
    /// UID of the first item in this page.
    pub first: Option<String>,
    /// UID of the last item in this page.  Use as `after` cursor for the next page.
    pub last: Option<String>,
    /// Total number of items in the full result set, if the server reports it.
    pub count: Option<u32>,
}

/// XEP-0059 RSM pagination parameters sent with a MAM query.
#[derive(Debug, Clone)]
pub struct RsmQuery {
    /// Maximum number of items to return.
    pub max: u32,
    /// Fetch items after this UID (forward pagination cursor from previous page).
    pub after: Option<String>,
    /// Fetch items before this UID (reverse pagination).
    pub before: Option<String>,
}

// ---------------------------------------------------------------------------
// XEP-0313 Message Archive Management
// ---------------------------------------------------------------------------

/// Filtering parameters for a MAM query.
#[derive(Debug, Clone)]
pub struct MamFilter {
    /// Restrict results to messages exchanged with this JID.
    pub with: Option<String>,
    /// Return messages sent on or after this ISO 8601 timestamp.
    pub start: Option<String>,
    /// Return messages sent on or before this ISO 8601 timestamp.
    pub end: Option<String>,
}

/// A full MAM page request combining a filter and RSM pagination parameters.
#[derive(Debug, Clone)]
pub struct MamQuery {
    /// Unique ID that correlates the IQ and its streamed result messages.
    pub query_id: String,
    /// Content filter applied by the server.
    pub filter: MamFilter,
    /// RSM page size and cursor.
    pub rsm: RsmQuery,
}

/// A single archived message unwrapped from a MAM `<result>` wrapper.
#[derive(Debug, Clone)]
pub struct MamMessage {
    /// The archive-assigned UID from `<result id='…'>`.
    pub archive_id: String,
    /// The `queryid` attribute — must match the originating `MamQuery.query_id`.
    pub query_id: String,
    /// The delivery timestamp from `<delay stamp='…'>`.
    pub timestamp: String,
    /// The `from` attribute of the inner forwarded `<message>`.
    pub forwarded_from: String,
    /// Text body of the inner forwarded `<message>`.
    pub body: String,
}

/// Accumulated results for one completed (or in-progress) MAM query page.
#[derive(Debug, Clone)]
pub struct MamResult {
    /// Messages received so far for this query, in delivery order.
    pub messages: Vec<MamMessage>,
    /// RSM metadata returned in the `<fin>` stanza.
    pub rsm: RsmSet,
    /// `true` when `<fin complete='true'>` was received — no more pages.
    pub complete: bool,
}

// ---------------------------------------------------------------------------
// MamManager
// ---------------------------------------------------------------------------

/// XEP-0313 / XEP-0059 state manager.
///
/// Holds pending queries and accumulates messages until the `<fin>` IQ
/// arrives.  All methods are pure: they only mutate in-memory state and
/// return stanzas/parsed values for the caller to act on.
pub struct MamManager {
    /// In-flight queries keyed by `query_id`.
    pending_queries: HashMap<String, MamQuery>,
    /// Accumulated results keyed by `query_id`.
    results: HashMap<String, MamResult>,
}

impl MamManager {
    /// Creates an empty manager with no pending queries.
    pub fn new() -> Self {
        Self {
            pending_queries: HashMap::new(),
            results: HashMap::new(),
        }
    }

    /// Build a MAM query IQ stanza and register the query as pending.
    ///
    /// The returned `Element` must be written to the XMPP stream by the caller.
    ///
    /// ```xml
    /// <iq type='set' id='…'>
    ///   <query xmlns='urn:xmpp:mam:2' queryid='{query_id}'>
    ///     <x xmlns='jabber:x:data' type='submit'>
    ///       <field var='FORM_TYPE'><value>urn:xmpp:mam:2</value></field>
    ///       <!-- optional filter fields -->
    ///     </x>
    ///     <set xmlns='http://jabber.org/protocol/rsm'>
    ///       <max>{n}</max>
    ///       <!-- optional <after> / <before> -->
    ///     </set>
    ///   </query>
    /// </iq>
    /// ```
    pub fn build_query_iq(&mut self, query: MamQuery) -> Element {
        let iq_id = Uuid::new_v4().to_string();
        let query_id = query.query_id.clone();

        // --- Data form fields ---
        let form_type_field = Element::builder("field", NS_DATA)
            .attr("var", "FORM_TYPE")
            .append(Element::builder("value", NS_DATA).append(NS_MAM).build())
            .build();

        let mut form_builder = Element::builder("x", NS_DATA)
            .attr("type", "submit")
            .append(form_type_field);

        if let Some(ref with) = query.filter.with {
            let with_field = Element::builder("field", NS_DATA)
                .attr("var", "with")
                .append(
                    Element::builder("value", NS_DATA)
                        .append(with.as_str())
                        .build(),
                )
                .build();
            form_builder = form_builder.append(with_field);
        }

        if let Some(ref start) = query.filter.start {
            let start_field = Element::builder("field", NS_DATA)
                .attr("var", "start")
                .append(
                    Element::builder("value", NS_DATA)
                        .append(start.as_str())
                        .build(),
                )
                .build();
            form_builder = form_builder.append(start_field);
        }

        if let Some(ref end) = query.filter.end {
            let end_field = Element::builder("field", NS_DATA)
                .attr("var", "end")
                .append(
                    Element::builder("value", NS_DATA)
                        .append(end.as_str())
                        .build(),
                )
                .build();
            form_builder = form_builder.append(end_field);
        }

        // --- RSM <set> ---
        let max_el = Element::builder("max", NS_RSM)
            .append(query.rsm.max.to_string().as_str())
            .build();

        let mut rsm_builder = Element::builder("set", NS_RSM).append(max_el);

        if let Some(ref after) = query.rsm.after {
            let after_el = Element::builder("after", NS_RSM)
                .append(after.as_str())
                .build();
            rsm_builder = rsm_builder.append(after_el);
        }

        if let Some(ref before) = query.rsm.before {
            let before_el = Element::builder("before", NS_RSM)
                .append(before.as_str())
                .build();
            rsm_builder = rsm_builder.append(before_el);
        }

        // --- Assemble <query> ---
        let mam_query = Element::builder("query", NS_MAM)
            .attr("queryid", query_id.as_str())
            .append(form_builder.build())
            .append(rsm_builder.build())
            .build();

        // --- Assemble <iq> ---
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", iq_id.as_str())
            .append(mam_query)
            .build();

        // Register the query and an empty result accumulator.
        self.results.insert(
            query_id.clone(),
            MamResult {
                messages: Vec::new(),
                rsm: RsmSet {
                    first: None,
                    last: None,
                    count: None,
                },
                complete: false,
            },
        );
        self.pending_queries.insert(query_id, query);

        iq
    }

    /// Parse an incoming `<message>` stanza that wraps a MAM `<result>`.
    ///
    /// Returns `Some(MamMessage)` when the stanza matches the expected shape:
    /// ```xml
    /// <message>
    ///   <result xmlns='urn:xmpp:mam:2' queryid='…' id='archive-id'>
    ///     <forwarded xmlns='urn:xmpp:forward:0'>
    ///       <delay xmlns='urn:ietf:params:xml:ns:xmpp-delay' stamp='…'/>
    ///       <message from='…'><body>…</body></message>
    ///     </forwarded>
    ///   </result>
    /// </message>
    /// ```
    /// The parsed message is also appended to the in-progress result buffer
    /// for the matching `query_id`.
    pub fn on_mam_message(&mut self, el: &Element) -> Option<MamMessage> {
        if el.name() != "message" {
            return None;
        }

        // Find <result xmlns='urn:xmpp:mam:2'>
        let result_el = el
            .children()
            .find(|c| c.name() == "result" && c.ns() == NS_MAM)?;

        let archive_id = result_el.attr("id")?.to_string();
        let query_id = result_el.attr("queryid")?.to_string();

        // Find <forwarded xmlns='urn:xmpp:forward:0'>
        let forwarded = result_el
            .children()
            .find(|c| c.name() == "forwarded" && c.ns() == NS_FORWARD)?;

        // Extract timestamp from <delay stamp='…'>
        let timestamp = forwarded
            .children()
            .find(|c| c.name() == "delay" && c.ns() == NS_DELAY)
            .and_then(|d| d.attr("stamp"))
            .unwrap_or("")
            .to_string();

        // Find the inner <message>
        let inner_msg = forwarded.children().find(|c| c.name() == "message")?;

        let forwarded_from = inner_msg.attr("from").unwrap_or("").to_string();

        let body = inner_msg
            .children()
            .find(|c| c.name() == "body")
            .map(tokio_xmpp::minidom::Element::text)
            .unwrap_or_default();

        let msg = MamMessage {
            archive_id,
            query_id: query_id.clone(),
            timestamp,
            forwarded_from,
            body,
        };

        // Accumulate into the in-progress result buffer.
        if let Some(result) = self.results.get_mut(&query_id) {
            result.messages.push(msg.clone());
        }

        Some(msg)
    }

    /// Parse the `<fin>` IQ that ends a MAM page.
    ///
    /// On success returns `(query_id, MamResult)` with the accumulated messages
    /// and RSM metadata.  The query is removed from the pending set.
    ///
    /// Expected shape:
    /// ```xml
    /// <iq type='result'>
    ///   <fin xmlns='urn:xmpp:mam:2' complete='true'>
    ///     <set xmlns='http://jabber.org/protocol/rsm'>
    ///       <first>uid-1</first><last>uid-n</last><count>42</count>
    ///     </set>
    ///   </fin>
    /// </iq>
    /// ```
    pub fn on_fin_iq(&mut self, el: &Element) -> Option<(String, MamResult)> {
        if el.name() != "iq" {
            return None;
        }

        // Find <fin xmlns='urn:xmpp:mam:2'>
        let fin = el
            .children()
            .find(|c| c.name() == "fin" && c.ns() == NS_MAM)?;

        // The query_id is carried in the <fin queryid='…'> attribute on some
        // servers, but the spec actually correlates via the IQ id.  We store
        // the query_id in the fin element's queryid attr when present; otherwise
        // we look for a pending query by inspecting the IQ id mapping.
        // For simplicity (matching the specified XML shape) we read queryid from
        // <fin> if present, otherwise fall back to the only pending query.
        let query_id = if let Some(qid) = fin.attr("queryid") {
            qid.to_string()
        } else if self.pending_queries.len() == 1 {
            self.pending_queries.keys().next()?.to_string()
        } else {
            return None;
        };

        let complete = fin.attr("complete") == Some("true");

        // Parse the RSM <set>
        let mut first: Option<String> = None;
        let mut last: Option<String> = None;
        let mut count: Option<u32> = None;

        if let Some(rsm_set) = fin
            .children()
            .find(|c| c.name() == "set" && c.ns() == NS_RSM)
        {
            for child in rsm_set.children() {
                match child.name() {
                    "first" => first = Some(child.text()),
                    "last" => last = Some(child.text()),
                    "count" => count = child.text().parse().ok(),
                    _ => {}
                }
            }
        }

        // Finalise the accumulated result.
        let mut result = self.results.remove(&query_id).unwrap_or(MamResult {
            messages: Vec::new(),
            rsm: RsmSet {
                first: None,
                last: None,
                count: None,
            },
            complete: false,
        });

        result.rsm = RsmSet { first, last, count };
        result.complete = complete;

        self.pending_queries.remove(&query_id);

        Some((query_id, result))
    }

    /// Returns `true` when a query with the given `query_id` is still pending.
    pub fn is_pending(&self, query_id: &str) -> bool {
        self.pending_queries.contains_key(query_id)
    }
}

impl Default for MamManager {
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

    // Helpers ----------------------------------------------------------------

    fn make_query(query_id: &str) -> MamQuery {
        MamQuery {
            query_id: query_id.to_string(),
            filter: MamFilter {
                with: Some("bob@example.com".to_string()),
                start: None,
                end: None,
            },
            rsm: RsmQuery {
                max: 50,
                after: None,
                before: None,
            },
        }
    }

    /// Build a well-formed MAM message wrapper element.
    fn make_mam_message(
        query_id: &str,
        archive_id: &str,
        stamp: &str,
        from: &str,
        body: &str,
    ) -> Element {
        let body_el = Element::builder("body", NS_CLIENT).append(body).build();

        let inner_msg = Element::builder("message", NS_CLIENT)
            .attr("from", from)
            .append(body_el)
            .build();

        let delay = Element::builder("delay", NS_DELAY)
            .attr("stamp", stamp)
            .build();

        let forwarded = Element::builder("forwarded", NS_FORWARD)
            .append(delay)
            .append(inner_msg)
            .build();

        let result_el = Element::builder("result", NS_MAM)
            .attr("queryid", query_id)
            .attr("id", archive_id)
            .append(forwarded)
            .build();

        Element::builder("message", NS_CLIENT)
            .append(result_el)
            .build()
    }

    /// Build a well-formed <fin> IQ element.
    fn make_fin_iq(
        query_id: Option<&str>,
        complete: bool,
        first: &str,
        last: &str,
        count: u32,
    ) -> Element {
        let first_el = Element::builder("first", NS_RSM).append(first).build();
        let last_el = Element::builder("last", NS_RSM).append(last).build();
        let count_el = Element::builder("count", NS_RSM)
            .append(count.to_string().as_str())
            .build();

        let rsm_set = Element::builder("set", NS_RSM)
            .append(first_el)
            .append(last_el)
            .append(count_el)
            .build();

        let mut fin_builder = Element::builder("fin", NS_MAM)
            .attr("complete", if complete { "true" } else { "false" })
            .append(rsm_set);

        if let Some(qid) = query_id {
            fin_builder = fin_builder.attr("queryid", qid);
        }

        Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .append(fin_builder.build())
            .build()
    }

    // Tests ------------------------------------------------------------------

    // 1. New manager has no pending queries.
    #[test]
    fn mam_manager_new_is_empty() {
        let mgr = MamManager::new();
        assert!(!mgr.is_pending("any-id"));
        assert!(mgr.pending_queries.is_empty());
        assert!(mgr.results.is_empty());
    }

    // 2. build_query_iq registers the query as pending.
    #[test]
    fn build_query_iq_stores_pending() {
        let mut mgr = MamManager::new();
        let q = make_query("qid-1");
        mgr.build_query_iq(q);
        assert!(mgr.is_pending("qid-1"));
    }

    // 3. build_query_iq sets the queryid attribute on the <query> element.
    #[test]
    fn build_query_iq_contains_queryid() {
        let mut mgr = MamManager::new();
        let q = make_query("qid-2");
        let iq = mgr.build_query_iq(q);

        let query_el = iq
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_MAM)
            .expect("iq must contain <query xmlns='urn:xmpp:mam:2'>");

        assert_eq!(query_el.attr("queryid"), Some("qid-2"));
    }

    // 4. build_query_iq includes a <set><max> RSM element.
    #[test]
    fn build_query_iq_has_max_rsm() {
        let mut mgr = MamManager::new();
        let mut q = make_query("qid-3");
        q.rsm.max = 20;
        let iq = mgr.build_query_iq(q);

        let query_el = iq
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_MAM)
            .expect("must contain <query>");

        let rsm_set = query_el
            .children()
            .find(|c| c.name() == "set" && c.ns() == NS_RSM)
            .expect("query must contain RSM <set>");

        let max_el = rsm_set
            .children()
            .find(|c| c.name() == "max")
            .expect("<set> must contain <max>");

        assert_eq!(max_el.text(), "20");
    }

    // 5. is_pending returns true immediately after building the query.
    #[test]
    fn is_pending_returns_true_after_query() {
        let mut mgr = MamManager::new();
        assert!(!mgr.is_pending("qid-5"));
        mgr.build_query_iq(make_query("qid-5"));
        assert!(mgr.is_pending("qid-5"));
    }

    // 6. on_mam_message parses body, archive_id, query_id, from, and timestamp.
    #[test]
    fn on_mam_message_parses_body() {
        let mut mgr = MamManager::new();
        mgr.build_query_iq(make_query("qid-6"));

        let el = make_mam_message(
            "qid-6",
            "archive-001",
            "2024-01-15T10:00:00Z",
            "bob@example.com/res",
            "Hello, Alice!",
        );

        let msg = mgr.on_mam_message(&el).expect("should parse mam message");
        assert_eq!(msg.archive_id, "archive-001");
        assert_eq!(msg.query_id, "qid-6");
        assert_eq!(msg.timestamp, "2024-01-15T10:00:00Z");
        assert_eq!(msg.forwarded_from, "bob@example.com/res");
        assert_eq!(msg.body, "Hello, Alice!");
    }

    // 7. on_fin_iq returns the accumulated result with RSM metadata.
    #[test]
    fn on_fin_iq_returns_result() {
        let mut mgr = MamManager::new();
        mgr.build_query_iq(make_query("qid-7"));

        // Push two messages first.
        let m1 = make_mam_message(
            "qid-7",
            "uid-1",
            "2024-01-01T00:00:00Z",
            "bob@example.com",
            "msg1",
        );
        let m2 = make_mam_message(
            "qid-7",
            "uid-2",
            "2024-01-01T00:01:00Z",
            "bob@example.com",
            "msg2",
        );
        mgr.on_mam_message(&m1);
        mgr.on_mam_message(&m2);

        let fin = make_fin_iq(Some("qid-7"), false, "uid-1", "uid-2", 100);
        let (returned_qid, result) = mgr.on_fin_iq(&fin).expect("should return result");

        assert_eq!(returned_qid, "qid-7");
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.rsm.first, Some("uid-1".to_string()));
        assert_eq!(result.rsm.last, Some("uid-2".to_string()));
        assert_eq!(result.rsm.count, Some(100));
        assert!(!result.complete);
    }

    // 8. on_fin_iq correctly reads the complete='true' flag.
    #[test]
    fn on_fin_iq_complete_flag() {
        let mut mgr = MamManager::new();
        mgr.build_query_iq(make_query("qid-8"));

        let fin = make_fin_iq(Some("qid-8"), true, "uid-a", "uid-z", 5);
        let (_, result) = mgr.on_fin_iq(&fin).expect("should return result");

        assert!(result.complete);
    }

    // 9. on_fin_iq removes the query from pending.
    #[test]
    fn on_fin_iq_clears_pending() {
        let mut mgr = MamManager::new();
        mgr.build_query_iq(make_query("qid-9"));
        assert!(mgr.is_pending("qid-9"));

        let fin = make_fin_iq(Some("qid-9"), true, "a", "b", 0);
        mgr.on_fin_iq(&fin);

        assert!(!mgr.is_pending("qid-9"));
    }

    // 10. on_mam_message returns None for non-MAM messages.
    #[test]
    fn on_mam_message_ignores_plain_message() {
        let mut mgr = MamManager::new();
        let el = Element::builder("message", NS_CLIENT)
            .attr("from", "bob@example.com")
            .append(Element::builder("body", NS_CLIENT).append("hi").build())
            .build();

        assert!(mgr.on_mam_message(&el).is_none());
    }

    // 11. build_query_iq includes <after> cursor when rsm.after is set.
    #[test]
    fn build_query_iq_includes_after_cursor() {
        let mut mgr = MamManager::new();
        let mut q = make_query("qid-11");
        q.rsm.after = Some("cursor-abc".to_string());
        let iq = mgr.build_query_iq(q);

        let query_el = iq
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_MAM)
            .unwrap();

        let rsm_set = query_el
            .children()
            .find(|c| c.name() == "set" && c.ns() == NS_RSM)
            .unwrap();

        let after_el = rsm_set
            .children()
            .find(|c| c.name() == "after")
            .expect("<set> must contain <after> when cursor set");

        assert_eq!(after_el.text(), "cursor-abc");
    }
}
