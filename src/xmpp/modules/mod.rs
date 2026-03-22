// XMPP modules — one file per XEP.
// Port order follows the rewrite roadmap in CLAUDE.md.

// Phase 1 (Core)
// pub mod connection;    // Task P1.9 — connection state machine
// pub mod roster;        // Task P1.4 — RFC 6121 roster + presence
pub mod presence_machine; // Task P1.4b — auto-away/xa/DND state machine
                          // pub mod chat;          // Task P1.5 — message send/receive + carbons
pub mod stream_mgmt; // Task P1.6 — XEP-0198 stream management
                     // pub mod ping;          // Task P1.7 — XEP-0199 ping

// Phase 3 (MUC)
pub mod bookmarks;
pub mod muc; // Task P3.1 — XEP-0045 multi-user chat // Task P3.4 — XEP-0048 bookmarks

// Phase 4 (History)
pub mod catchup; // Task P4.3 — MAM catchup state machine
pub mod conversation_sync;
pub mod mam; // Task P4.1 — XEP-0313 message archive management
pub mod sync; // Task P4.4 — background sync orchestrator (MAM catchup) // Task P6.4 — XEP-0223 conversation sync

// Phase 5 (Rich features)
pub mod avatar; // Task P5.2 — XEP-0084 / vCard avatars
pub mod command_palette;
pub mod file_upload; // Task P5.1 — XEP-0363 file upload
pub mod link_preview; // Task P5.5 — OG / HTML meta-tag link preview parser
pub mod message_mutations; // Task P5.3 — XEP-0444 reactions, XEP-0308 corrections, XEP-0424 retractions // Task P5.5 — command palette fuzzy search

// Phase 6 (XEP parity)
pub mod account;
pub mod adhoc; // Task P6.2 — XEP-0050 ad-hoc commands
pub mod blocking; // Task P6.2 — XEP-0191 blocking
pub mod console; // Task P6.3 — XMPP console stanza log
pub mod disco; // Task P6.1 — XEP-0115 entity capabilities + XEP-0030 service discovery
pub mod entity_time; // Task P6.4 — XEP-0202 entity time
pub mod ignore; // Task P6.4 — per-room ignored users via PubSub
pub mod push_cleanup; // Task P6.5 — XEP-0357 push-disable / WebPush VAPID unsubscribe
pub mod xmpp_uri; // Task P6.3 — XEP-0147 xmpp: URI parser // Task P6.3 — XEP-0077 account management IQs
