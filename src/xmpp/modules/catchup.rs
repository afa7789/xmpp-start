#![allow(dead_code)]
// Task P4.3 — MAM catchup state machine
//
// Tracks per-conversation MAM catchup progress.  No I/O, no async — purely
// in-memory state so the engine can drive it without coupling to sqlx or tokio.
//
// Lifecycle per conversation:
//   1. Engine calls `start(jid, last_stanza_id)` → gets a (query_id, MamQuery)
//      to send over the wire.
//   2. As <result> stanzas arrive, engine calls `on_result(query_id, jid)` to
//      verify ownership.
//   3. When the server sends <fin>, engine calls `on_fin(query_id)`.
//   4. Engine emits `XmppEvent::CatchupFinished` and moves on.

use std::collections::HashMap;

use uuid::Uuid;

use crate::xmpp::modules::mam::{MamFilter, MamQuery, RsmQuery};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Per-conversation catchup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatchupState {
    /// No active fetch for this conversation.
    Idle,
    /// A query has been issued and we are waiting for <result>s and <fin>.
    Fetching {
        query_id: String,
        /// The `after` cursor we sent — the stanza-id of the last stored message,
        /// or `None` if we asked for the full archive.
        after: Option<String>,
    },
    /// The <fin complete='true'> (or any <fin>) has been received; all pages
    /// for this round of catchup have arrived.
    Complete,
}

// ---------------------------------------------------------------------------
// CatchupManager
// ---------------------------------------------------------------------------

/// Tracks MAM catchup state for each conversation JID.
///
/// All methods are pure: they only mutate in-memory state and return values
/// for the caller (the engine) to act on.  No I/O is performed here.
pub struct CatchupManager {
    /// conversation_jid → current state.
    states: HashMap<String, CatchupState>,
    /// query_id → conversation_jid — reverse index so `on_result`/`on_fin`
    /// can look up the JID from the query_id arriving in the stanza.
    query_to_jid: HashMap<String, String>,
}

impl CatchupManager {
    /// Create an empty manager.
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            query_to_jid: HashMap::new(),
        }
    }

    /// Begin a catchup fetch for `conversation_jid`.
    ///
    /// * `last_stanza_id` — the archive-id of the last message already stored
    ///   in the local DB.  Pass `None` to fetch the full archive.
    ///
    /// Returns `(query_id, MamQuery)` that the engine must send to the server.
    /// The conversation moves to `Fetching`.
    pub fn start(
        &mut self,
        conversation_jid: &str,
        last_stanza_id: Option<&str>,
    ) -> (String, MamQuery) {
        let query_id = Uuid::new_v4().to_string();

        let query = MamQuery {
            query_id: query_id.clone(),
            filter: MamFilter {
                with: Some(conversation_jid.to_string()),
                start: None,
                end: None,
            },
            rsm: RsmQuery {
                max: 50,
                after: last_stanza_id.map(std::string::ToString::to_string),
                before: None,
            },
        };

        let after = last_stanza_id.map(std::string::ToString::to_string);

        self.states.insert(
            conversation_jid.to_string(),
            CatchupState::Fetching {
                query_id: query_id.clone(),
                after,
            },
        );
        self.query_to_jid
            .insert(query_id.clone(), conversation_jid.to_string());

        (query_id, query)
    }

    /// Called when a MAM `<result queryid='…'>` arrives.
    ///
    /// Returns the conversation JID if `query_id` belongs to an active
    /// `Fetching` query; `None` otherwise (unknown / already completed).
    pub fn on_result<'a>(&'a self, query_id: &str, _conversation_jid: &str) -> Option<&'a str> {
        let jid = self.query_to_jid.get(query_id)?;
        match self.states.get(jid.as_str()) {
            Some(CatchupState::Fetching { .. }) => Some(jid.as_str()),
            _ => None,
        }
    }

    /// Called when a `<fin>` IQ arrives for `query_id`.
    ///
    /// Moves the corresponding conversation to `Complete` and removes the
    /// reverse index entry.  Safe to call with an unknown `query_id`.
    pub fn on_fin(&mut self, query_id: &str) {
        if let Some(jid) = self.query_to_jid.remove(query_id) {
            self.states.insert(jid, CatchupState::Complete);
        }
    }

    /// Returns `true` when there are no conversations in the `Fetching` state.
    pub fn is_idle(&self) -> bool {
        self.states
            .values()
            .all(|s| !matches!(s, CatchupState::Fetching { .. }))
    }

    /// Reset all state — call on disconnect so stale query_ids are not matched
    /// against server stanzas that arrive during a new session.
    pub fn reset(&mut self) {
        self.states.clear();
        self.query_to_jid.clear();
    }

    /// Return the current state for a conversation, or `Idle` if unknown.
    pub fn state_for(&self, conversation_jid: &str) -> &CatchupState {
        self.states
            .get(conversation_jid)
            .unwrap_or(&CatchupState::Idle)
    }
}

impl Default for CatchupManager {
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

    // 1. start() returns a non-empty query_id that is a valid UUID string.
    #[test]
    fn start_returns_valid_query_id() {
        let mut mgr = CatchupManager::new();
        let (query_id, query) = mgr.start("alice@example.com", None);

        assert!(!query_id.is_empty(), "query_id must not be empty");
        // UUID v4 format: 8-4-4-4-12 hex chars separated by hyphens (36 chars).
        assert_eq!(query_id.len(), 36, "query_id should be a UUID string");
        assert_eq!(query.query_id, query_id);
        assert_eq!(
            query.filter.with.as_deref(),
            Some("alice@example.com"),
            "filter.with must be set to the conversation JID"
        );
        assert_eq!(query.rsm.max, 50, "default page size is 50");
        assert!(
            query.rsm.after.is_none(),
            "no after cursor when last_stanza_id is None"
        );
    }

    // 2. start() passes last_stanza_id as the RSM <after> cursor.
    #[test]
    fn start_sets_after_cursor_when_last_stanza_id_given() {
        let mut mgr = CatchupManager::new();
        let (_, query) = mgr.start("bob@example.com", Some("stanza-id-42"));
        assert_eq!(query.rsm.after.as_deref(), Some("stanza-id-42"));
    }

    // 3. on_result returns the JID for an active query.
    #[test]
    fn on_result_returns_jid_for_active_query() {
        let mut mgr = CatchupManager::new();
        let (query_id, _) = mgr.start("carol@example.com", None);

        let jid = mgr.on_result(&query_id, "carol@example.com");
        assert_eq!(jid, Some("carol@example.com"));
    }

    // 4. on_result returns None for an unknown query_id.
    #[test]
    fn on_result_returns_none_for_unknown_query() {
        let mgr = CatchupManager::new();
        let result = mgr.on_result("no-such-query-id", "alice@example.com");
        assert!(result.is_none());
    }

    // 5. on_fin moves the conversation to Complete.
    #[test]
    fn on_fin_marks_complete() {
        let mut mgr = CatchupManager::new();
        let (query_id, _) = mgr.start("dave@example.com", Some("last-id"));

        // While fetching, state is Fetching.
        assert!(matches!(
            mgr.state_for("dave@example.com"),
            CatchupState::Fetching { .. }
        ));

        mgr.on_fin(&query_id);

        assert_eq!(mgr.state_for("dave@example.com"), &CatchupState::Complete);
    }

    // 6. is_idle() returns true when all conversations are Complete (none Fetching).
    #[test]
    fn is_idle_when_all_complete() {
        let mut mgr = CatchupManager::new();

        // Start two conversations.
        let (qid_a, _) = mgr.start("eve@example.com", None);
        let (qid_b, _) = mgr.start("frank@example.com", None);

        // Both fetching — not idle.
        assert!(!mgr.is_idle());

        mgr.on_fin(&qid_a);
        // One still fetching — not idle.
        assert!(!mgr.is_idle());

        mgr.on_fin(&qid_b);
        // All complete — idle.
        assert!(mgr.is_idle());
    }

    // 7. reset() clears all state and reverse index.
    #[test]
    fn reset_clears_all_states() {
        let mut mgr = CatchupManager::new();
        let (query_id, _) = mgr.start("grace@example.com", None);

        // Verify there's active state.
        assert!(mgr.on_result(&query_id, "grace@example.com").is_some());
        assert!(!mgr.is_idle());

        mgr.reset();

        // After reset: no state, idle, on_result returns None.
        assert!(mgr.is_idle());
        assert!(mgr.on_result(&query_id, "grace@example.com").is_none());
        assert_eq!(mgr.state_for("grace@example.com"), &CatchupState::Idle);
    }

    // 8. is_idle() returns true for a fresh manager (no conversations registered).
    #[test]
    fn is_idle_on_new_manager() {
        let mgr = CatchupManager::new();
        assert!(mgr.is_idle());
    }

    // 9. on_fin with an unknown query_id is a no-op (no panic, no state change).
    #[test]
    fn on_fin_with_unknown_query_id_is_noop() {
        let mut mgr = CatchupManager::new();
        let (query_id, _) = mgr.start("henry@example.com", None);

        mgr.on_fin("not-a-real-query-id");

        // Original query should still be Fetching.
        assert!(matches!(
            mgr.state_for("henry@example.com"),
            CatchupState::Fetching { .. }
        ));
        assert!(!mgr.is_idle());

        // Clean up so the test is self-contained.
        mgr.on_fin(&query_id);
    }

    // 10. After on_fin, on_result for the same query_id returns None.
    #[test]
    fn on_result_returns_none_after_fin() {
        let mut mgr = CatchupManager::new();
        let (query_id, _) = mgr.start("ivy@example.com", None);

        mgr.on_fin(&query_id);

        let result = mgr.on_result(&query_id, "ivy@example.com");
        assert!(
            result.is_none(),
            "on_result must return None once fin has been received"
        );
    }
}
