// XMPP modules — one file per XEP.
// Port order follows the rewrite roadmap in CLAUDE.md.

// Phase 1 (Core)
// pub mod connection;    // Task P1.9 — connection state machine
// pub mod roster;        // Task P1.4 — RFC 6121 roster + presence
pub mod presence_machine; // Task P1.4b — auto-away/xa/DND state machine
// pub mod chat;          // Task P1.5 — message send/receive + carbons
pub mod stream_mgmt;   // Task P1.6 — XEP-0198 stream management
// pub mod ping;          // Task P1.7 — XEP-0199 ping

// Phase 3 (MUC)
pub mod muc;           // Task P3.1 — XEP-0045 multi-user chat
pub mod bookmarks;     // Task P3.4 — XEP-0048 bookmarks

// Phase 4 (History)
pub mod mam;           // Task P4.1 — XEP-0313 message archive management
pub mod catchup;       // Task P4.3 — MAM catchup state machine
pub mod sync;          // Task P4.4 — background sync orchestrator (MAM catchup)
// pub mod conversation_sync; // Task P6.4 — XEP-0223 conversation sync

// Phase 5 (Rich features)
// pub mod http_upload;   // Task P5.1 — XEP-0363 file upload
// pub mod avatar;        // Task P5.2 — XEP-0084 / vCard avatars
// pub mod reactions;     // Task P5.3 — XEP-0444 reactions
// pub mod corrections;   // Task P5.3 — XEP-0308 last message correction
// pub mod retractions;   // Task P5.3 — XEP-0424 message retraction

// Phase 6 (XEP parity)
// pub mod caps;          // Task P6.1 — XEP-0115 entity capabilities
// pub mod disco;         // Task P6.1 — XEP-0030 service discovery
// pub mod adhoc;         // Task P6.2 — XEP-0050 ad-hoc commands
// pub mod blocking;      // Task P6.2 — XEP-0191 blocking
// pub mod entity_time;   // Task P6.4 — XEP-0202 entity time
