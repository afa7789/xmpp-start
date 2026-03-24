// Task P4.4 — Background sync orchestrator
//
// Post-connect MAM catchup pipeline.
//
// This is a pure state machine — no I/O, no async, no sqlx.
// The caller (engine) is responsible for:
//   1. Gathering the list of known conversations + their last stanza IDs.
//   2. Sending the IQ stanzas returned by `start_sync`.
//   3. Routing incoming MAM <result> messages through `on_mam_result`.
//   4. Routing MAM <fin> IQs through `on_fin`.
//   5. Draining accumulated messages and persisting them.

use std::collections::HashMap;

use uuid::Uuid;

use crate::xmpp::modules::mam::{MamFilter, MamManager, MamMessage, MamQuery, RsmQuery};
use tokio_xmpp::minidom::Element;

// ---------------------------------------------------------------------------
// SyncOrchestrator
// ---------------------------------------------------------------------------

/// Coordinates a post-connect MAM catchup across all known conversations.
///
/// Lifecycle:
/// ```text
/// SyncOrchestrator::new()
///   └─ start_sync(conversations) → Vec<(query_id, IQ Element)>
///        (send each IQ over the XMPP stream)
///   └─ on_mam_result(msg)         → Option<conversation_jid>
///   └─ on_fin(query_id)           → Option<(conversation_jid, count)>
///   └─ is_complete()              → bool
///   └─ drain_messages()           → Vec<MamMessage>
/// ```
pub struct SyncOrchestrator {
    /// conversation_jid → query_id for in-flight MAM queries.
    pending: HashMap<String, String>,
    /// query_id → conversation_jid reverse index (for fast lookup on result).
    query_to_jid: HashMap<String, String>,
    /// Accumulated messages from MAM replies.
    received: Vec<MamMessage>,
    /// Total messages fetched across all conversations.
    total_fetched: usize,
    /// Underlying MAM manager used to build IQ stanzas.
    mam: MamManager,
}

impl SyncOrchestrator {
    /// Create a new, empty orchestrator.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            query_to_jid: HashMap::new(),
            received: Vec::new(),
            total_fetched: 0,
            mam: MamManager::new(),
        }
    }

    /// Called once on connect with the list of conversations and their last
    /// stanza IDs.
    ///
    /// Returns a list of `(query_id, IQ Element)` that the engine must send
    /// over the XMPP stream.  A separate query is built for each conversation.
    /// If `last_stanza_id` is `Some`, the query uses an `after` RSM cursor so
    /// that only new messages are fetched.
    pub fn start_sync(
        &mut self,
        conversations: &[(String, Option<String>)], // (jid, last_stanza_id)
    ) -> Vec<(String, Element)> {
        let mut out = Vec::with_capacity(conversations.len());

        for (jid, last_stanza_id) in conversations {
            let query_id = Uuid::new_v4().to_string();

            let query = MamQuery {
                query_id: query_id.clone(),
                filter: MamFilter {
                    with: Some(jid.clone()),
                    start: None,
                    end: None,
                },
                rsm: RsmQuery {
                    max: 50,
                    after: last_stanza_id.clone(),
                    before: None,
                },
            };

            let iq = self.mam.build_query_iq(query);

            self.pending.insert(jid.clone(), query_id.clone());
            self.query_to_jid.insert(query_id.clone(), jid.clone());

            out.push((query_id, iq));
        }

        out
    }

    /// Called when a MAM `<result>` arrives.
    ///
    /// Returns `Some(conversation_jid)` if the result belongs to one of the
    /// active queries started by `start_sync`.  Returns `None` for results
    /// that belong to unrelated queries.
    pub fn on_mam_result(&mut self, msg: MamMessage) -> Option<String> {
        let jid = self.query_to_jid.get(&msg.query_id)?.clone();
        self.received.push(msg);
        self.total_fetched += 1;
        Some(jid)
    }

    /// Called when a MAM `<fin>` IQ arrives with the given `query_id`.
    ///
    /// Returns `Some((conversation_jid, message_count))` and removes the
    /// conversation from the pending set.  Returns `None` if the query_id is
    /// unknown (e.g., belongs to an unrelated MAM query).
    pub fn on_fin(&mut self, query_id: &str) -> Option<(String, usize)> {
        let jid = self.query_to_jid.remove(query_id)?;
        self.pending.remove(&jid);

        // Count how many messages in `received` belong to this query_id.
        let count = self
            .received
            .iter()
            .filter(|m| m.query_id == query_id)
            .count();

        Some((jid, count))
    }

    /// Returns `true` when all queries initiated by `start_sync` have completed
    /// (i.e., their `<fin>` IQ has been received).
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drain all accumulated messages.
    ///
    /// Call after each `on_fin` (or once after `is_complete`) to retrieve
    /// messages for persistence.
    pub fn drain_messages(&mut self) -> Vec<MamMessage> {
        std::mem::take(&mut self.received)
    }
}

impl Default for SyncOrchestrator {
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

    // Helper: build a minimal MamMessage for a given query_id.
    fn make_msg(query_id: &str, archive_id: &str) -> MamMessage {
        MamMessage {
            archive_id: archive_id.to_string(),
            query_id: query_id.to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            forwarded_from: "alice@example.com".to_string(),
            body: "hello".to_string(),
        }
    }

    // 1. start_sync returns exactly one IQ element per conversation.
    #[test]
    fn start_sync_returns_one_iq_per_conversation() {
        let mut orch = SyncOrchestrator::new();
        let conversations = vec![
            ("alice@example.com".to_string(), None),
            ("bob@example.com".to_string(), Some("stanza-42".to_string())),
            ("carol@example.com".to_string(), None),
        ];
        let result = orch.start_sync(&conversations);
        assert_eq!(result.len(), 3);
    }

    // 2. start_sync with no conversations returns empty vec.
    #[test]
    fn start_sync_with_no_conversations_returns_empty() {
        let mut orch = SyncOrchestrator::new();
        let result = orch.start_sync(&[]);
        assert!(result.is_empty());
        assert!(orch.is_complete());
    }

    // 3. on_mam_result accumulates messages and returns the correct JID.
    #[test]
    fn on_mam_result_accumulates_messages() {
        let mut orch = SyncOrchestrator::new();
        let conversations = vec![("alice@example.com".to_string(), None)];
        let pairs = orch.start_sync(&conversations);
        let query_id = &pairs[0].0;

        let msg1 = make_msg(query_id, "arc-1");
        let msg2 = make_msg(query_id, "arc-2");

        let jid1 = orch.on_mam_result(msg1).expect("should return JID");
        let jid2 = orch.on_mam_result(msg2).expect("should return JID");

        assert_eq!(jid1, "alice@example.com");
        assert_eq!(jid2, "alice@example.com");
        assert_eq!(orch.received.len(), 2);
        assert_eq!(orch.total_fetched, 2);
    }

    // 4. on_mam_result returns None for an unknown query_id.
    #[test]
    fn on_mam_result_unknown_query_returns_none() {
        let mut orch = SyncOrchestrator::new();
        let msg = make_msg("unknown-query-id", "arc-99");
        assert!(orch.on_mam_result(msg).is_none());
    }

    // 5. on_fin returns the JID and the count of messages received for it.
    #[test]
    fn on_fin_returns_jid_and_count() {
        let mut orch = SyncOrchestrator::new();
        let conversations = vec![("bob@example.com".to_string(), None)];
        let pairs = orch.start_sync(&conversations);
        let query_id = pairs[0].0.clone();

        orch.on_mam_result(make_msg(&query_id, "arc-a"));
        orch.on_mam_result(make_msg(&query_id, "arc-b"));
        orch.on_mam_result(make_msg(&query_id, "arc-c"));

        let (jid, count) = orch.on_fin(&query_id).expect("should return result");
        assert_eq!(jid, "bob@example.com");
        assert_eq!(count, 3);
    }

    // 6. is_complete returns true only when all fins have been received.
    #[test]
    fn is_complete_when_all_fins_received() {
        let mut orch = SyncOrchestrator::new();
        let conversations = vec![
            ("alice@example.com".to_string(), None),
            ("bob@example.com".to_string(), None),
        ];
        let pairs = orch.start_sync(&conversations);
        let qid_a = pairs[0].0.clone();
        let qid_b = pairs[1].0.clone();

        assert!(!orch.is_complete());

        orch.on_fin(&qid_a);
        assert!(!orch.is_complete(), "still one pending after first fin");

        orch.on_fin(&qid_b);
        assert!(orch.is_complete(), "all fins received — should be complete");
    }

    // 7. on_fin returns None for an unknown query_id.
    #[test]
    fn on_fin_unknown_query_returns_none() {
        let mut orch = SyncOrchestrator::new();
        assert!(orch.on_fin("no-such-query").is_none());
    }

    // 8. drain_messages empties the buffer and returns all accumulated messages.
    #[test]
    fn drain_messages_clears_buffer() {
        let mut orch = SyncOrchestrator::new();
        let pairs = orch.start_sync(&[("alice@example.com".to_string(), None)]);
        let qid = &pairs[0].0;

        orch.on_mam_result(make_msg(qid, "arc-1"));
        orch.on_mam_result(make_msg(qid, "arc-2"));

        let drained = orch.drain_messages();
        assert_eq!(drained.len(), 2);
        assert!(
            orch.received.is_empty(),
            "buffer should be empty after drain"
        );
    }

    // 9. Each IQ produced by start_sync carries a unique query_id.
    #[test]
    fn start_sync_each_iq_has_unique_query_id() {
        let mut orch = SyncOrchestrator::new();
        let conversations = vec![
            ("alice@example.com".to_string(), None),
            ("bob@example.com".to_string(), None),
            ("carol@example.com".to_string(), None),
        ];
        let pairs = orch.start_sync(&conversations);
        let mut ids: Vec<&str> = pairs.iter().map(|(qid, _)| qid.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), 3, "all query IDs must be unique");
    }

    // 10. start_sync with a last_stanza_id sets the after RSM cursor in the IQ.
    #[test]
    fn start_sync_sets_after_cursor_when_last_stanza_id_present() {
        const NS_MAM: &str = "urn:xmpp:mam:2";
        const NS_RSM: &str = "http://jabber.org/protocol/rsm";

        let mut orch = SyncOrchestrator::new();
        let conversations = vec![(
            "alice@example.com".to_string(),
            Some("cursor-xyz".to_string()),
        )];
        let pairs = orch.start_sync(&conversations);
        let iq = &pairs[0].1;

        let query_el = iq
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_MAM)
            .expect("IQ must contain MAM <query>");

        let rsm_set = query_el
            .children()
            .find(|c| c.name() == "set" && c.ns() == NS_RSM)
            .expect("<query> must contain RSM <set>");

        let after_el = rsm_set
            .children()
            .find(|c| c.name() == "after")
            .expect("<set> must contain <after> when last_stanza_id is set");

        assert_eq!(after_el.text(), "cursor-xyz");
    }
}
