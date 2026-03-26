// Task P1.1  — TCP + STARTTLS (via tokio-xmpp AsyncClient<ServerConfig>)
// Task P1.2  — SASL authentication (handled by tokio-xmpp)
// Task P1.3  — XML stream parser (xmpp-parsers)
// Task P1.4  — RFC 6121 Roster + presence
// Task P1.5  — Message send/receive + XEP-0280 Carbons
// Task P1.7  — DNS SRV (tokio-xmpp ServerConfig::UseSrv)
// Task P1.9  — Connection state machine

use std::collections::VecDeque;

use futures::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_xmpp::connect::ServerConnector;
use tokio_xmpp::{
    jid::Jid,
    minidom::Element,
    parsers::{
        iq::Iq,
        message::{Body, Message as XmppMessage, MessageType},
        roster::Roster,
    },
    starttls::ServerConfig,
    AsyncClient, AsyncConfig,
};

use super::{
    connection::{
        dns,
        proxy::{ProxyLifecycle, TransportKind},
        ConnectConfig,
    },
    handlers::{
        handle_iq, handle_message, handle_presence, has_omemo_encrypted,
        omemo_check_prekey_rotation, omemo_encrypt_and_send, omemo_try_decrypt, OmemoEncryptError,
        NS_CHAT_MARKERS, NS_RECEIPTS,
    },
    modules::account::AccountManager,
    modules::adhoc::AdhocManager,
    modules::avatar::AvatarManager,
    modules::blocking::BlockingManager,
    modules::bob,
    modules::bookmarks::BookmarkManager,
    modules::catchup::CatchupManager,
    modules::conversation_sync::ConversationSyncManager,
    modules::disco::{DiscoIdentity, DiscoManager},
    modules::entity_time::EntityTimeManager,
    modules::file_upload::FileUploadManager,
    modules::geoloc,
    modules::ignore::IgnoreManager,
    modules::mam::{MamFilter, MamManager, MamQuery, RsmQuery},
    modules::message_mutations::MutationManager,
    modules::muc::MucManager,
    modules::muc_admin::{AffiliationAction, MucAdminManager},
    modules::muc_config::MucConfigManager,
    modules::muc_voice::MucVoiceManager,
    modules::omemo::{bundle::OmemoManager, device::DeviceManager, store::OmemoStore, TrustState},
    modules::presence_machine::PresenceMachine,
    modules::push::PushManager,
    modules::registration::RegistrationManager,
    modules::spam_report::build_spam_report,
    modules::stickers,
    modules::stream_mgmt::StreamMgmt,
    modules::sync::SyncOrchestrator,
    modules::vcard_edit::VCardEditManager,
    modules::{NS_FASTEN, NS_FORWARD, NS_GEOLOC, NS_MAM, NS_MUC_USER, NS_PUBSUB_EVENT},
    IncomingMessage, XmppCommand, XmppEvent,
};

const NS_CARBONS: &str = "urn:xmpp:carbons:2";
// L3: XEP-0425 message moderation namespaces
const NS_MODERATION: &str = "urn:xmpp:message-moderate:0";
// J9: XEP-0077 registration namespace

// ---------------------------------------------------------------------------
// Outbound stanza rate limiter (token bucket)
// ---------------------------------------------------------------------------

/// Token-bucket rate limiter for outbound XMPP stanzas.
///
/// Allows up to `capacity` stanzas to be sent in a burst, then refills at
/// `rate` tokens per second.  When the bucket is empty, `try_consume` returns
/// `false` and the caller should stop draining the outbox until the next loop
/// iteration (which gives the bucket time to refill).
///
/// Additionally, `warn_if_high_depth` logs a warning when the outbox backlog
/// exceeds `MAX_OUTBOX_DEPTH`, which is a sign of persistent flooding.
struct StanzaRateLimiter {
    /// Maximum token count (burst size).
    capacity: f64,
    /// Available tokens.
    tokens: f64,
    /// Refill rate in tokens per second.
    rate: f64,
    /// Last time the bucket was refilled.
    last_refill: Instant,
}

/// Warn (once per crossing) when the outbox grows beyond this depth.
const MAX_OUTBOX_DEPTH: usize = 100;

impl StanzaRateLimiter {
    /// Create a new limiter allowing `capacity` burst stanzas refilling at
    /// `rate` stanzas/second.
    fn new(capacity: f64, rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            rate,
            last_refill: Instant::now(),
        }
    }

    /// Attempt to consume one token.  Returns `true` if the stanza may be
    /// sent, `false` if the bucket is exhausted.
    fn try_consume(&mut self) -> bool {
        // Refill proportional to elapsed time.
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Log a warning when the outbox depth exceeds `MAX_OUTBOX_DEPTH`.
    fn warn_if_high_depth(&self, depth: usize) {
        if depth > MAX_OUTBOX_DEPTH {
            tracing::warn!(
                "outbox: backlog depth {} exceeds limit {} — possible stanza flood",
                depth,
                MAX_OUTBOX_DEPTH
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Public engine entry-point
// ---------------------------------------------------------------------------

/// Runs the XMPP engine loop.
///
/// Waits for the first [`XmppCommand::Connect`] before dialling the server.
/// On disconnect the engine returns to the idle state and waits again.
///
/// `db` is an optional SQLite pool used for OMEMO key/session persistence.
/// Pass `None` when the database is not available (e.g., tests, multi-engine).
pub async fn run_engine(
    event_tx: mpsc::Sender<XmppEvent>,
    mut cmd_rx: mpsc::Receiver<XmppCommand>,
    db: Option<SqlitePool>,
) {
    loop {
        // Wait for a command.
        match cmd_rx.recv().await {
            Some(XmppCommand::Connect(config)) => {
                tracing::info!("engine: connecting as {}", config.jid);
                run_session(config, &event_tx, &mut cmd_rx, db.clone()).await;
                // returns to Idle
            }
            Some(XmppCommand::Register(config)) => {
                tracing::info!("engine: registering account {}", config.jid);
                run_registration_session(config, &event_tx, &mut cmd_rx).await;
                // returns to Idle
            }
            Some(_) | None => {
                // Other commands while idle are silently ignored.
                // Channel closed → exit loop.
                if cmd_rx.is_closed() {
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session loop
// ---------------------------------------------------------------------------

// S6: bit flags for privacy: bit 0=receipts, bit 1=typing, bit 2=read_markers (1=enabled)
async fn run_session(
    config: ConnectConfig,
    event_tx: &mpsc::Sender<XmppEvent>,
    cmd_rx: &mut mpsc::Receiver<XmppCommand>,
    db: Option<SqlitePool>,
) {
    let jid: Jid = match config.jid.parse() {
        Ok(j) => j,
        Err(e) => {
            let _ = event_tx
                .send(XmppEvent::Disconnected {
                    reason: format!("Invalid JID: {e}"),
                })
                .await;
            return;
        }
    };

    // DC-18: drive the proxy lifecycle state machine when a proxy is configured.
    // ProxyLifecycle is a pure state machine — it does not open sockets itself.
    // Here we transition it through Starting → Running so that the transport-kind
    // logic (TCP → WebSocket fallback after 3 failures) is exercised on every
    // connection attempt.  Real SOCKS5/HTTP tunnel injection is deferred to a
    // future task.
    let mut proxy_lifecycle = ProxyLifecycle::new(5_000);
    if let Some((proxy_type, proxy_host, proxy_port)) = config.proxy_config() {
        tracing::info!(
            "engine: proxy configured — type={} host={} port={}",
            proxy_type,
            proxy_host,
            proxy_port
        );
        match proxy_lifecycle.start() {
            Ok(()) => {
                proxy_lifecycle.on_started();
                tracing::debug!(
                    "engine: proxy lifecycle → Running (transport={:?})",
                    proxy_lifecycle.transport()
                );
            }
            Err(e) => {
                tracing::warn!("engine: proxy lifecycle start error: {e}");
            }
        }
        if proxy_lifecycle.transport() == TransportKind::WebSocket {
            tracing::info!(
                "engine: proxy transport fell back to WebSocket after repeated failures"
            );
        }
    }

    // Build the server connector.
    //   1. Explicit host in `server` field → use it directly.
    //   2. `manual_srv` set → resolve that SRV record via dns::resolve_with_override.
    //   3. Neither → let tokio-xmpp do standard RFC 6120 SRV discovery.
    let server = if !config.server.trim().is_empty() {
        let (host, port) = parse_host_port(&config.server, 5222);
        ServerConfig::Manual { host, port }
    } else if let Some(ref srv_name) = config.manual_srv {
        match dns::resolve_with_override(jid.domain().as_str(), Some(srv_name.as_str())).await {
            Ok(ep) => {
                tracing::info!(
                    "engine: SRV override {} resolved to {}:{}",
                    srv_name,
                    ep.host,
                    ep.port
                );
                ServerConfig::Manual {
                    host: ep.host,
                    port: ep.port,
                }
            }
            Err(e) => {
                tracing::warn!(
                    "engine: SRV override resolution failed ({}), falling back to UseSrv",
                    e
                );
                ServerConfig::UseSrv
            }
        }
    } else {
        ServerConfig::UseSrv
    };

    let mut client = AsyncClient::new_with_config(AsyncConfig {
        jid,
        password: config.password.clone(),
        server,
    });
    client.set_reconnect(false); // we manage reconnect ourselves

    // Outbox for stanzas that need to be sent after a select! arm.
    let mut outbox: VecDeque<Element> = VecDeque::new();
    // Rate limiter: burst of 30 stanzas (handles connect flood), refilling at 10 stanzas/second.
    let mut rate_limiter = StanzaRateLimiter::new(30.0, 10.0);
    let mut reconnect_attempt: u32 = 0;

    // C1: XEP-0198 stream management tracker
    let mut sm = StreamMgmt::new();
    // C4: XEP-0191 blocking command manager
    let mut blocking_mgr = BlockingManager::new();
    // own JID (set on Online, used for carbon detection)
    let mut own_jid_str = String::new();
    // C3: XEP-0313 MAM + catchup state
    let mut mam_mgr = MamManager::new();
    let mut catchup_mgr = CatchupManager::new();
    // C2: XEP-0153/presence state machine
    let mut presence_machine = PresenceMachine::new();
    // J2: Set custom status message from config
    presence_machine.set_status_message(config.status_message.clone());
    // C5: XEP-0115/XEP-0030 service discovery + caps
    let mut disco_mgr = DiscoManager::new(
        "https://github.com/rexisce",
        &[DiscoIdentity {
            category: "client".to_string(),
            kind: "pc".to_string(),
            name: "rexisce".to_string(),
        }],
        &["urn:xmpp:mam:2", "urn:xmpp:carbons:2"],
    );
    // E4: XEP-0363 file upload manager
    let mut file_upload_mgr = FileUploadManager::new();
    // H1: avatar manager (vCard-temp fallback)
    let mut avatar_mgr = AvatarManager::new();
    // D3: XEP-0045 multi-user chat manager
    let mut muc_mgr = MucManager::new();
    // K1: XEP-0045 room config manager
    let mut muc_config_mgr = MucConfigManager::new();
    // D4: XEP-0048 bookmarks manager
    let mut bookmark_mgr = BookmarkManager::new();
    // K7: XEP-0357 push notifications manager
    let mut push_mgr = PushManager::new();
    // K2: XEP-0054 own vCard editing manager
    let mut vcard_edit_mgr = VCardEditManager::new();
    // L4: XEP-0050 ad-hoc commands manager
    let mut adhoc_mgr = AdhocManager::new();
    // DC-8: XEP-0202 entity time manager
    let _entity_time_mgr = EntityTimeManager::new();
    // DC-6: XEP-0045 MUC admin manager (affiliations + role changes)
    let mut muc_admin_mgr = MucAdminManager::new();
    // DC-6: XEP-0045 MUC voice request manager
    let muc_voice_mgr = MucVoiceManager::new();
    // DC-10: per-room ignore lists (PubSub private storage)
    let mut ignore_mgr = IgnoreManager::new();
    // DC-10: conversation sync (PubSub private storage, XEP-0223)
    let conv_sync_mgr = ConversationSyncManager::new();

    // DC-5: XEP-0308 corrections, XEP-0424 retractions, XEP-0444 reactions
    let mutation_mgr = MutationManager::new();

    // P4.4: bulk post-connect MAM sync orchestrator
    let mut sync_orch = SyncOrchestrator::new();

    // DC-9: XEP-0077 in-band registration (change-password / delete-account)
    let mut account_mgr = AccountManager::new();

    // MEMO: XEP-0384 OMEMO encryption manager — only active when a DB pool is available.
    let mut omemo_mgr: Option<OmemoManager> = db
        .as_ref()
        .map(|pool| OmemoManager::new(OmemoStore::new(pool.clone())));

    // S6: privacy settings — control whether we send receipts, typing, read markers
    // bit 0=receipts, bit 1=typing, bit 2=read_markers (1=enabled, per-session local)
    let flags: u8 = (config.send_receipts as u8)
        | ((config.send_typing as u8) << 1)
        | ((config.send_read_markers as u8) << 2);

    loop {
        // Drain outbox before blocking on the next event.
        // Warn when the backlog is large (possible flood).
        rate_limiter.warn_if_high_depth(outbox.len());
        while let Some(stanza) = outbox.pop_front() {
            // Backpressure: if the token bucket is exhausted, return the stanza
            // to the front of the queue and stop draining until the next loop
            // iteration, giving the bucket time to refill.
            if !rate_limiter.try_consume() {
                tracing::warn!(
                    "outbox: rate limit reached — deferring {} queued stanza(s)",
                    outbox.len() + 1
                );
                outbox.push_front(stanza);
                break;
            }
            // C1: record sent stanza and check for queue desync
            // Only count real stanzas (message/presence/iq), not SM nonzas (<a>, <r>).
            let is_sm_nonza = stanza.ns() == "urn:xmpp:sm:3"
                && (stanza.name() == "a" || stanza.name() == "r");
            if !is_sm_nonza {
                sm.on_stanza_sent(stanza.clone());
            }
            if sm.has_queue_desync() {
                tracing::warn!(
                    "stream_mgmt: unacked queue desync — {} pending, h={}",
                    sm.pending_count(),
                    sm.h()
                );
            }
            // C1: every 5 stanzas sent, proactively request an ack from server
            if sm.pending_count() % 5 == 0 && sm.pending_count() > 0 {
                outbox.push_back(sm.build_request());
            }
            // F1: emit sent stanza to debug console
            let xml_str = String::from(&stanza);
            let _ = event_tx
                .send(XmppEvent::ConsoleEntry {
                    direction: "sent".into(),
                    xml: xml_str,
                })
                .await;
            if let Err(e) = client.send_stanza(stanza).await {
                tracing::warn!("send_stanza failed: {e}");
            }
        }

        tokio::select! {
            maybe_event = client.next() => {
                match maybe_event {
                    None => {
                        tracing::info!("engine: stream ended");
                        break;
                    }
                    Some(ev) => {
                        let auth_failure = handle_client_event(ev, event_tx, &mut outbox, &mut reconnect_attempt, &mut sm, &mut blocking_mgr, &mut own_jid_str, &mut mam_mgr, &mut catchup_mgr, &mut presence_machine, &mut disco_mgr, &mut file_upload_mgr, &mut avatar_mgr, &mut muc_mgr, &mut muc_config_mgr, &mut bookmark_mgr, &mut push_mgr, &mut vcard_edit_mgr, &mut adhoc_mgr, &mut omemo_mgr, &mut ignore_mgr, &conv_sync_mgr, &mut account_mgr, &mut sync_orch, &config.jid, config.push_service_jid.as_deref(), flags).await;
                        if auth_failure {
                            tracing::info!("engine: breaking session loop due to auth failure");
                            break;
                        }
                    }
                }
            }

            maybe_cmd = cmd_rx.recv() => {
                match maybe_cmd {
                    None | Some(XmppCommand::Disconnect) => {
                        tracing::info!("engine: disconnect requested");
                        let _ = client.send_end().await;
                        break;
                    }
                    Some(XmppCommand::Connect(_)) => {
                        // BUG-7: a new Connect while already running means the user is
                        // correcting credentials. Break so the outer loop can restart.
                        tracing::info!("engine: new Connect received while running — restarting session");
                        let _ = client.send_end().await;
                        break;
                    }
                    Some(XmppCommand::SendMessage { to, body, id }) => {
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(make_message(to_jid, &body, &id));
                        }
                    }
                    Some(XmppCommand::BlockJid(jid)) => {
                        outbox.push_back(blocking_mgr.build_block_iq(&[jid.as_str()]));
                        tracing::info!("blocking: sent block IQ for {jid}");
                    }
                    Some(XmppCommand::UnblockJid(jid)) => {
                        outbox.push_back(blocking_mgr.build_unblock_iq(&[jid.as_str()]));
                        tracing::info!("blocking: sent unblock IQ for {jid}");
                    }
                    Some(XmppCommand::SendRoomInvitation { room, user, reason }) => {
                        // K3: Send a room invitation
                        if let (Ok(room_jid), Ok(user_jid)) = (room.parse::<Jid>(), user.parse::<Jid>()) {
                            outbox.push_back(MucManager::build_invitation(&room_jid, &user_jid, reason.as_deref()));
                            tracing::info!("muc: sent invitation to {user} for room {room}");
                        }
                    }
                    Some(XmppCommand::SendChatState { to, composing }) => {
                        // S6: respect user's privacy preference for typing indicators
                        if flags & 0b010 != 0 {
                            if let Ok(to_jid) = to.parse::<Jid>() {
                                outbox.push_back(make_chat_state_message(to_jid, composing));
                            }
                        }
                    }
                    Some(XmppCommand::AddContact(jid)) => {
                        outbox.push_back(make_roster_set(&jid));
                        tracing::info!("roster: sent add-contact IQ for {jid}");
                    }
                    Some(XmppCommand::RequestUploadSlot { filename, size, mime }) => {
                        // E4: request upload slot from server's upload service
                        // Use a well-known upload service JID pattern; in production, discover via disco.
                        let Some(domain) = config.jid.split('@').nth(1) else {
                            tracing::warn!("file_upload: cannot determine server domain from JID; skipping upload");
                            continue;
                        };
                        let upload_jid = format!("upload.{domain}");
                        let (_, iq) = file_upload_mgr.request_slot(&filename, size, &mime, &upload_jid);
                        outbox.push_back(iq);
                        tracing::info!("file_upload: requested slot for {filename}");
                    }
                    Some(XmppCommand::FetchAvatar(jid)) => {
                        let (_, iq) = avatar_mgr.build_vcard_request(&jid);
                        outbox.push_back(iq);
                        tracing::debug!("avatar: fetching vCard for {jid}");
                    }
                    Some(XmppCommand::SetAvatar { data, mime_type }) => {
                        // H2: Publish own avatar via XEP-0084 PubSub
                        // Compute SHA-1 hash of the image data
                        use sha1::{Digest, Sha1};
                        let mut hasher = Sha1::new();
                        hasher.update(&data);
                        let sha1 = format!("{:x}", hasher.finalize());
                        // Default to user's own JID for pubsub service (discover via disco in production)
                        let pubsub_jid = config.jid.split('@').nth(1).map_or_else(|| "pubsub.example.com".to_string(), |d| format!("pubsub.{}", d));
                        // First publish metadata
                        let meta_iq = avatar_mgr.build_avatar_metadata_publish(&pubsub_jid, &sha1, data.len(), &mime_type);
                        outbox.push_back(meta_iq);
                        // Then publish data
                        let data_iq = avatar_mgr.build_avatar_data_publish(&pubsub_jid, &sha1, &data, &mime_type);
                        outbox.push_back(data_iq);
                        tracing::info!("avatar: published own avatar ({} bytes)", data.len());
                    }
                    Some(XmppCommand::SendReaction { to, msg_id, emojis }) => {
                        // E3: Build XEP-0444 reaction stanza via MutationManager
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            let emoji_refs: Vec<&str> = emojis.iter().map(String::as_str).collect();
                            let el = mutation_mgr.build_reaction(&to_jid.to_string(), &msg_id, &emoji_refs);
                            outbox.push_back(el);
                            tracing::debug!("reactions: sent {} reaction(s) to {to_jid}", emojis.len());
                        }
                    }
                    Some(XmppCommand::SetPresence(status)) => {
                        // C2: Update user presence status and broadcast to server
                        presence_machine.set_user_status(status);
                        if let Some(stanza) = presence_machine.build_presence_stanza() {
                            outbox.push_back(stanza);
                        }
                    }
                    Some(XmppCommand::SendCorrection { to, original_id, new_body }) => {
                        // E1: XEP-0308 message correction via MutationManager
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(mutation_mgr.build_correction(&to_jid.to_string(), &original_id, &new_body));
                        }
                    }
                    Some(XmppCommand::SendRetraction { to, origin_id }) => {
                        // E2: XEP-0424 message retraction via MutationManager
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(mutation_mgr.build_retraction(&to_jid.to_string(), &origin_id));
                        }
                    }
                    // L3: XEP-0425 message moderation (moderator removes any room message)
                    Some(XmppCommand::ModerateMessage { room_jid, message_id, reason }) => {
                        outbox.push_back(make_moderation_message(&room_jid, &message_id, reason.as_deref()));
                        tracing::info!("muc: moderating message {message_id} in {room_jid}");
                    }
                    // DC-6: Kick a user (role = none)
                    Some(XmppCommand::KickUser { room_jid, nick }) => {
                        let (_, iq) = muc_admin_mgr.build_role_query(&room_jid, &nick, "none");
                        outbox.push_back(iq);
                        tracing::info!("muc: kicking {nick} from {room_jid}");
                    }
                    // DC-6: Ban a user (affiliation = outcast)
                    Some(XmppCommand::BanUser { room_jid, jid }) => {
                        let (_, iq) = muc_admin_mgr.build_affiliation_query(&room_jid, AffiliationAction::Ban(jid.clone()));
                        outbox.push_back(iq);
                        tracing::info!("muc: banning {jid} from {room_jid}");
                    }
                    // DC-6: Set arbitrary affiliation
                    Some(XmppCommand::SetAffiliation { room_jid, action }) => {
                        let (_, iq) = muc_admin_mgr.build_affiliation_query(&room_jid, action);
                        outbox.push_back(iq);
                        tracing::info!("muc: setting affiliation in {room_jid}");
                    }
                    // DC-6: Grant voice (participant role)
                    Some(XmppCommand::GrantVoice { room_jid, nick }) => {
                        let (_, iq) = muc_admin_mgr.build_role_query(&room_jid, &nick, "participant");
                        outbox.push_back(iq);
                        tracing::info!("muc: granting voice to {nick} in {room_jid}");
                    }
                    // DC-6: Revoke voice (visitor role)
                    Some(XmppCommand::RevokeVoice { room_jid, nick }) => {
                        let (_, iq) = muc_admin_mgr.build_role_query(&room_jid, &nick, "visitor");
                        outbox.push_back(iq);
                        tracing::info!("muc: revoking voice from {nick} in {room_jid}");
                    }
                    // DC-6: Grant moderator role
                    Some(XmppCommand::GrantModerator { room_jid, nick }) => {
                        let (_, iq) = muc_admin_mgr.build_role_query(&room_jid, &nick, "moderator");
                        outbox.push_back(iq);
                        tracing::info!("muc: granting moderator to {nick} in {room_jid}");
                    }
                    // DC-6: Request voice (visitor sends voice request to room)
                    Some(XmppCommand::RequestVoice { room_jid, nick }) => {
                        let msg = muc_voice_mgr.build_voice_request(&room_jid, &nick);
                        outbox.push_back(msg);
                        tracing::info!("muc: requesting voice in {room_jid} as {nick}");
                    }
                    // DC-6: Approve a voice request
                    Some(XmppCommand::ApproveVoice { room_jid, nick }) => {
                        let msg = muc_voice_mgr.build_approve_voice(&room_jid, &nick);
                        outbox.push_back(msg);
                        tracing::info!("muc: approving voice for {nick} in {room_jid}");
                    }
                    // DC-6: Decline a voice request
                    Some(XmppCommand::DeclineVoice { room_jid, nick }) => {
                        let msg = muc_voice_mgr.build_decline_voice(&room_jid, &nick);
                        outbox.push_back(msg);
                        tracing::info!("muc: declining voice for {nick} in {room_jid}");
                    }
                    Some(XmppCommand::JoinRoom { jid, nick }) => {
                        // D3: XEP-0045 MUC join
                        outbox.push_back(muc_mgr.join_room(&jid, &nick));
                        tracing::info!("muc: joining room {jid} as {nick}");
                    }
                    Some(XmppCommand::FetchRoomList) => {
                        // K2: Browse public rooms — send disco#items to MUC service
                        // Default MUC service: conference.<domain>
                        let muc_service = config.jid.split('@').nth(1).map_or_else(|| "conference.example.com".to_string(), |d| format!("conference.{}", d));
                        let (_, iq) = disco_mgr.build_items_request(&muc_service);
                        outbox.push_back(iq);
                        tracing::info!("k2: fetching room list from {}", muc_service);
                    }
                    // K1: Create a new MUC room (same as join; server sends 201 on creation)
                    Some(XmppCommand::CreateRoom { local, service, nick }) => {
                        let room_jid = format!("{}@{}", local, service);
                        outbox.push_back(muc_mgr.join_room(&room_jid, &nick));
                        tracing::info!("muc: creating room {room_jid} as {nick}");
                    }
                    // K1: Submit room configuration form
                    Some(XmppCommand::ConfigureRoom { room_jid, config }) => {
                        let (_, iq) = muc_config_mgr.build_config_submit(&room_jid, &config);
                        outbox.push_back(iq);
                        tracing::info!("muc: submitting config for {room_jid}");
                    }
                    Some(XmppCommand::LeaveRoom(jid)) => {
                        // D3: XEP-0045 MUC leave
                        if let Some(stanza) = muc_mgr.leave_room(&jid) {
                            outbox.push_back(stanza);
                            tracing::info!("muc: leaving room {jid}");
                        }
                    }
                    Some(XmppCommand::SendDisplayed { to, id }) => {
                        // S6: respect user's privacy preference for read markers
                        if flags & 0b100 != 0 {
                            outbox.push_back(make_displayed_message(&to, &id));
                            tracing::debug!("chat-markers: sent displayed for {id} to {to}");
                        }
                    }
                    Some(XmppCommand::SetMamPrefs { default_mode }) => {
                        // J10: set MAM archiving preferences
                        outbox.push_back(make_mam_prefs_set(&default_mode));
                        tracing::debug!("mam: sent prefs-set default={default_mode}");
                    }
                    // K2: Fetch own vCard
                    Some(XmppCommand::FetchOwnVCard) => {
                        let (_, iq) = vcard_edit_mgr.build_get();
                        outbox.push_back(iq);
                        tracing::debug!("vcard_edit: fetching own vCard");
                    }
                    // K2: Publish own vCard
                    Some(XmppCommand::SetOwnVCard(fields)) => {
                        let (_, iq) = vcard_edit_mgr.build_set(&fields);
                        outbox.push_back(iq);
                        tracing::info!("vcard_edit: publishing own vCard");
                    }
                    // L4: Execute an ad-hoc command
                    Some(XmppCommand::ExecuteAdhocCommand { to_jid, node }) => {
                        let (_, iq) = adhoc_mgr.build_execute(&to_jid, &node);
                        outbox.push_back(iq);
                        tracing::info!("adhoc: executing command {} on {}", node, to_jid);
                    }
                    // L4: Continue an in-progress ad-hoc command
                    Some(XmppCommand::ContinueAdhocCommand {
                        to_jid,
                        node,
                        session_id,
                        fields,
                    }) => {
                        let (_, iq) =
                            adhoc_mgr.build_continue(&to_jid, &node, &session_id, &fields);
                        outbox.push_back(iq);
                        tracing::info!(
                            "adhoc: continuing command {} session {}",
                            node,
                            session_id
                        );
                    }
                    // L4: Cancel an in-progress ad-hoc command
                    Some(XmppCommand::CancelAdhocCommand {
                        to_jid,
                        node,
                        session_id,
                    }) => {
                        let (_, iq) = adhoc_mgr.build_cancel(&to_jid, &node, &session_id);
                        outbox.push_back(iq);
                        tracing::info!(
                            "adhoc: cancelling command {} session {}",
                            node,
                            session_id
                        );
                    }
                    // L4: Discover ad-hoc commands on a target JID
                    Some(XmppCommand::DiscoverAdhocCommands { target_jid }) => {
                        let (_, iq) = disco_mgr.build_items_request(&target_jid);
                        outbox.push_back(iq);
                        tracing::info!("adhoc: discovering commands on {}", target_jid);
                    }
                    Some(XmppCommand::RemoveContact(_))
                    | Some(XmppCommand::RenameContact { .. })
                    | Some(XmppCommand::FetchVCard(_))
                    | Some(XmppCommand::FetchHistory { .. })
                    | Some(XmppCommand::Register(_))
                    | Some(XmppCommand::SubmitRegistration { .. }) => {
                        // Not yet implemented inside an active session — silently ignore.
                    }
                    // S1: auto-away — user has been idle, transition to AutoAway
                    Some(XmppCommand::UserIdle) => {
                        let before = presence_machine.effective_status();
                        presence_machine.on_idle_detected();
                        let after = presence_machine.effective_status();
                        if before != after {
                            if let Some(stanza) = presence_machine.build_presence_stanza() {
                                outbox.push_back(stanza);
                            }
                        }
                    }
                    // S1: auto-away — user has been idle for extended period, transition to AutoXa
                    Some(XmppCommand::UserExtendedIdle) => {
                        let before = presence_machine.effective_status();
                        presence_machine.on_sleep_detected();
                        let after = presence_machine.effective_status();
                        if before != after {
                            if let Some(stanza) = presence_machine.build_presence_stanza() {
                                outbox.push_back(stanza);
                            }
                        }
                    }
                    // S1: auto-away — user is active again, restore pre-idle status
                    Some(XmppCommand::UserActive) => {
                        let before = presence_machine.effective_status();
                        presence_machine.on_activity_detected();
                        let after = presence_machine.effective_status();
                        if before != after {
                            if let Some(stanza) = presence_machine.build_presence_stanza() {
                                outbox.push_back(stanza);
                            }
                        }
                    }
                    // K7: Enable push notifications (XEP-0357)
                    Some(XmppCommand::EnablePush { service_jid }) => {
                        let iq = push_mgr.build_enable_iq(&service_jid);
                        outbox.push_back(iq);
                        tracing::info!("push: enabling notifications via {}", service_jid);
                    }
                    // K7: Disable push notifications (XEP-0357)
                    Some(XmppCommand::DisablePush { service_jid }) => {
                        let iq = push_mgr.build_disable_iq(&service_jid);
                        outbox.push_back(iq);
                        tracing::info!("push: disabling notifications for {}", service_jid);
                    }
                    // K7: Disable all push notifications (XEP-0357)
                    Some(XmppCommand::DisableAllPush) => {
                        let iq = push_mgr.build_disable_all_iq();
                        outbox.push_back(iq);
                        tracing::info!("push: disabling all notifications");
                    }
                    // MULTI: account management — not yet wired to engine session logic.
                    Some(XmppCommand::SwitchAccount(_))
                    | Some(XmppCommand::AddAccount(_))
                    | Some(XmppCommand::RemoveAccount(_))
                    | Some(XmppCommand::ConnectAccount(_))
                    | Some(XmppCommand::DisconnectAccount(_)) => {
                        // No-op until multi-session engine is implemented.
                    }
                    // L3: Publish user location via PEP (XEP-0080).
                    Some(XmppCommand::PublishLocation(loc)) => {
                        let iq = geoloc::build_geoloc_publish(&loc);
                        outbox.push_back(iq);
                        tracing::debug!("geoloc: publishing location ({}, {})", loc.lat, loc.lon);
                    }
                    Some(XmppCommand::ReportSpam { jid, reason }) => {
                        let iq = build_spam_report(&jid, reason.as_deref());
                        outbox.push_back(iq);
                    }
                    Some(XmppCommand::RequestBob { cid, from }) => {
                        let iq = bob::build_bob_request(&cid, &from);
                        outbox.push_back(iq);
                    }
                    Some(XmppCommand::SendSticker { to, pack_id, sticker }) => {
                        let msg = stickers::build_sticker_message(&to, &pack_id, &sticker);
                        outbox.push_back(msg);
                    }
                    // DC-10: Add user to per-room ignore list and persist to PubSub.
                    Some(XmppCommand::IgnoreUser { room_jid, user_jid }) => {
                        ignore_mgr.add(&room_jid, &user_jid);
                        outbox.push_back(ignore_mgr.build_publish_iq(&room_jid));
                        tracing::info!("ignore: ignoring {user_jid} in {room_jid}");
                    }
                    // DC-10: Remove user from per-room ignore list and persist to PubSub.
                    Some(XmppCommand::UnignoreUser { room_jid, user_jid }) => {
                        ignore_mgr.remove(&room_jid, &user_jid);
                        outbox.push_back(ignore_mgr.build_publish_iq(&room_jid));
                        tracing::info!("ignore: unignoring {user_jid} in {room_jid}");
                    }
                    // DC-10: Fetch ignore list for a room from PubSub.
                    Some(XmppCommand::FetchIgnoreList { room_jid }) => {
                        outbox.push_back(IgnoreManager::build_fetch_iq(&room_jid));
                        tracing::debug!("ignore: fetching ignore list for {room_jid}");
                    }
                    // DC-10: Persist current conversation list to PubSub private storage.
                    Some(XmppCommand::SyncConversations(conversations)) => {
                        outbox.push_back(conv_sync_mgr.build_publish_iq(&conversations));
                        tracing::debug!("conv_sync: persisting {} conversations", conversations.len());
                    }
                    // DC-10: Fetch conversation list from PubSub private storage.
                    Some(XmppCommand::FetchConversations) => {
                        outbox.push_back(conv_sync_mgr.build_fetch_iq());
                        tracing::debug!("conv_sync: fetching conversations");
                    }
                    // DC-9: XEP-0077 change-password
                    Some(XmppCommand::ChangePassword { username, new_password }) => {
                        let (_, iq) = account_mgr.build_change_password_iq(&username, &new_password);
                        outbox.push_back(iq);
                        tracing::info!("account: sent change-password IQ for {username}");
                    }
                    // DC-9: XEP-0077 delete account
                    Some(XmppCommand::DeleteAccount) => {
                        let (_, iq) = account_mgr.build_delete_account_iq();
                        outbox.push_back(iq);
                        tracing::info!("account: sent delete-account IQ");
                    }
                    // P4.4: bulk post-connect MAM sync across multiple conversations
                    Some(XmppCommand::StartMamSync(conversations)) => {
                        let pairs = sync_orch.start_sync(&conversations);
                        let count = pairs.len();
                        for (_, iq) in pairs {
                            outbox.push_back(iq);
                        }
                        tracing::info!("sync: started bulk MAM sync for {count} conversation(s)");
                    }
                    // MEMO: Enable OMEMO — generate keys and publish device list + bundle.
                    Some(XmppCommand::OmemoEnable) => {
                        if let Some(ref mut mgr) = omemo_mgr {
                            match mgr.enable(&config.jid).await {
                                Ok(stanzas) => {
                                    let device_id = mgr.device_mgr.own_device_id();
                                    for stanza in stanzas {
                                        outbox.push_back(stanza);
                                    }
                                    tracing::info!("omemo: enabled with device_id={device_id}");
                                    // Notify the UI that OMEMO is now active.
                                    let _ = event_tx
                                        .send(XmppEvent::OmemoEnabled { device_id })
                                        .await;
                                    // Emit own device list received so UI knows OMEMO is active.
                                    let _ = event_tx.send(XmppEvent::OmemoDeviceListReceived {
                                        jid: config.jid.clone(),
                                        devices: vec![device_id],
                                    }).await;
                                }
                                Err(e) => {
                                    tracing::error!("omemo: enable failed: {e}");
                                }
                            }
                        } else {
                            tracing::warn!("omemo: OmemoEnable received but no DB pool available");
                        }
                    }
                    // MEMO: Encrypt and send an OMEMO message.
                    Some(XmppCommand::OmemoEncryptMessage { to, body }) => {
                        if let Some(ref mut mgr) = omemo_mgr {
                            match omemo_encrypt_and_send(mgr, &config.jid, &to, &body).await {
                                Ok(stanza) => {
                                    outbox.push_back(stanza);
                                    tracing::info!("omemo: encrypted message queued for {to}");
                                }
                                Err(OmemoEncryptError::NoSessions { device_ids }) => {
                                    // We know the device IDs — fetch bundles directly.
                                    tracing::info!(
                                        "omemo: fetching bundles for {} device(s) of {to}",
                                        device_ids.len()
                                    );
                                    for device_id in &device_ids {
                                        let (iq_id, iq) =
                                            mgr.device_mgr.build_bundle_fetch(&to, *device_id);
                                        mgr.track_bundle_fetch(iq_id, to.clone(), *device_id);
                                        outbox.push_back(iq);
                                    }
                                    let _ = event_tx.send(XmppEvent::OmemoKeyExchangeNeeded {
                                        jid: to.clone(),
                                    }).await;
                                }
                                Err(OmemoEncryptError::NoTrustedDevices) => {
                                    tracing::warn!("omemo: no trusted devices for {to}, fetching device list");
                                    // Fetch device list — track it so the result triggers bundle fetches.
                                    let (iq_id, iq) = mgr.device_mgr.build_device_list_fetch(&to);
                                    mgr.track_device_list_fetch(iq_id, to.clone());
                                    outbox.push_back(iq);
                                    let _ = event_tx.send(XmppEvent::OmemoKeyExchangeNeeded {
                                        jid: to.clone(),
                                    }).await;
                                }
                                Err(OmemoEncryptError::Other(e)) => {
                                    tracing::error!("omemo: encrypt failed for {to}: {e}");
                                    let _ = event_tx.send(XmppEvent::OmemoKeyExchangeNeeded {
                                        jid: to.clone(),
                                    }).await;
                                }
                            }
                        } else {
                            tracing::warn!("omemo: OmemoEncryptMessage received but no DB pool");
                        }
                    }
                    // MEMO: Update trust for a peer device.
                    Some(XmppCommand::OmemoTrustDevice { jid, device_id }) => {
                        if let Some(ref mgr) = omemo_mgr {
                            if let Err(e) = mgr.store
                                .set_trust(&config.jid, &jid, device_id, TrustState::Trusted)
                                .await
                            {
                                tracing::error!("omemo: set_trust failed for {jid}/{device_id}: {e}");
                            } else {
                                tracing::info!("omemo: device {device_id} of {jid} marked trusted");
                            }
                        }
                    }
                }
            }
        }
    }

    let _ = event_tx
        .send(XmppEvent::Disconnected {
            reason: "session ended".into(),
        })
        .await;
}

// ---------------------------------------------------------------------------
// Event dispatching
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn handle_client_event(
    ev: tokio_xmpp::Event,
    event_tx: &mpsc::Sender<XmppEvent>,
    outbox: &mut VecDeque<Element>,
    reconnect_attempt: &mut u32,
    sm: &mut StreamMgmt,
    blocking_mgr: &mut BlockingManager,
    own_jid_str: &mut String,
    mam_mgr: &mut MamManager,
    catchup_mgr: &mut CatchupManager,
    presence_machine: &mut PresenceMachine,
    disco_mgr: &mut DiscoManager,
    file_upload_mgr: &mut FileUploadManager,
    avatar_mgr: &mut AvatarManager,
    muc_mgr: &mut MucManager,
    muc_config_mgr: &mut MucConfigManager,
    bookmark_mgr: &mut BookmarkManager,
    push_mgr: &mut PushManager,
    vcard_edit_mgr: &mut VCardEditManager,
    adhoc_mgr: &mut AdhocManager,
    omemo_mgr: &mut Option<OmemoManager>,
    ignore_mgr: &mut IgnoreManager,
    conv_sync_mgr: &ConversationSyncManager,
    account_mgr: &mut AccountManager,
    sync_orch: &mut SyncOrchestrator,
    account_jid: &str,
    push_service_jid: Option<&str>,
    privacy_flags: u8,
) -> bool {
    match ev {
        tokio_xmpp::Event::Online { bound_jid, .. } => {
            *reconnect_attempt = 0;
            *own_jid_str = bound_jid.to_string();
            tracing::info!("engine: online as {bound_jid}");

            // Request roster (P1.4).
            outbox.push_back(make_roster_get());

            // Enable message carbons (P1.5 / XEP-0280).
            outbox.push_back(make_carbons_enable());

            // Announce presence (C2: track state, C5: with XEP-0115 caps).
            presence_machine.on_connected();
            outbox.push_back(make_presence_with_caps(
                disco_mgr,
                presence_machine.status_message(),
            ));

            // C4: fetch blocklist (XEP-0191)
            outbox.push_back(blocking_mgr.build_fetch_iq());

            // C3: MAM catchup — query archive for the last 50 messages overall
            let (server_query_id, mam_query) = catchup_mgr.start("__server__", None);
            let catchup_query = MamQuery {
                query_id: mam_query.query_id.clone(),
                filter: MamFilter {
                    with: None,
                    start: None,
                    end: None,
                },
                rsm: RsmQuery {
                    max: 50,
                    after: None,
                    before: None,
                },
            };
            outbox.push_back(mam_mgr.build_query_iq(catchup_query));
            tracing::info!("mam: triggered post-connect catchup (query_id={server_query_id})");

            // D4: fetch bookmarks from private XML storage (XEP-0048)
            outbox.push_back(make_bookmarks_get_iq());
            tracing::debug!("bookmarks: requested private XML storage");

            // J10: fetch MAM archiving preferences
            outbox.push_back(make_mam_prefs_get_iq());
            tracing::debug!("mam: requested archiving preferences");

            // DC-7: XEP-0357 push cleanup — disable all stale push subscriptions on connect
            outbox.push_back(push_mgr.build_disable_all_iq());
            tracing::debug!("push: sent disable-all push IQ on connect");

            // K7: XEP-0357 push notifications — enable on connect if configured
            if let Some(svc) = push_service_jid {
                outbox.push_back(push_mgr.build_enable_iq(svc));
                tracing::info!("push: enabling notifications via {svc} on connect");
            }

            // MEMO: Auto-publish OMEMO device list + bundle on reconnect if identity exists.
            if let Some(ref mut mgr) = omemo_mgr {
                match mgr.republish_stanzas(account_jid).await {
                    Ok(Some(stanzas)) => {
                        let device_id = mgr.device_mgr.own_device_id();
                        for stanza in stanzas {
                            outbox.push_back(stanza);
                        }
                        tracing::info!(
                            "omemo: republished device list + bundle on reconnect (device_id={device_id})"
                        );
                    }
                    Ok(None) => {
                        tracing::debug!("omemo: no identity found, OMEMO not yet enabled");
                    }
                    Err(e) => {
                        tracing::warn!("omemo: failed to republish on connect: {e}");
                    }
                }
            }

            let _ = event_tx
                .send(XmppEvent::Connected {
                    bound_jid: bound_jid.to_string(),
                })
                .await;
        }

        tokio_xmpp::Event::Disconnected(err) => {
            presence_machine.on_disconnected();
            let unacked = sm.unacked_stanzas().len();
            let err_str = err.to_string();
            tracing::warn!(
                "engine: disconnected — {err_str} ({unacked} unacked stanzas, h={})",
                sm.h()
            );
            // C1: reset stream management counters for the next session
            sm.reset();
            // P4.3: discard stale catchup query IDs so they are not matched in the new session
            catchup_mgr.reset();

            // BUG-7: detect auth failures — do not reconnect, surface the error instead
            let is_auth_error = is_auth_error(&err_str);
            if is_auth_error {
                tracing::warn!("engine: auth failure detected, not reconnecting");
                let _ = event_tx
                    .send(XmppEvent::Disconnected { reason: err_str })
                    .await;
                return true;
            }

            *reconnect_attempt += 1;
            let _ = event_tx
                .send(XmppEvent::Reconnecting {
                    attempt: *reconnect_attempt,
                })
                .await;
        }

        tokio_xmpp::Event::Stanza(el) => {
            // C1: record inbound stanza and maybe send coalesced ack
            // Only count real stanzas for h, not SM nonzas (<a>, <r>).
            let is_sm_nonza = el.ns() == "urn:xmpp:sm:3"
                && (el.name() == "a" || el.name() == "r");
            if !is_sm_nonza {
                sm.on_stanza_received();
                if let Some(ack) = sm.maybe_send_ack() {
                    outbox.push_back(ack);
                }
            }
            dispatch_stanza(
                el,
                event_tx,
                blocking_mgr,
                sm,
                outbox,
                own_jid_str,
                mam_mgr,
                catchup_mgr,
                sync_orch,
                disco_mgr,
                file_upload_mgr,
                avatar_mgr,
                muc_mgr,
                muc_config_mgr,
                bookmark_mgr,
                vcard_edit_mgr,
                adhoc_mgr,
                omemo_mgr,
                ignore_mgr,
                conv_sync_mgr,
                account_mgr,
                account_jid,
                privacy_flags,
            )
            .await;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_stanza(
    el: Element,
    event_tx: &mpsc::Sender<XmppEvent>,
    blocking_mgr: &mut BlockingManager,
    sm: &mut StreamMgmt,
    outbox: &mut VecDeque<Element>,
    own_jid_str: &str,
    mam_mgr: &mut MamManager,
    catchup_mgr: &mut CatchupManager,
    sync_orch: &mut SyncOrchestrator,
    disco_mgr: &mut DiscoManager,
    file_upload_mgr: &mut FileUploadManager,
    avatar_mgr: &mut AvatarManager,
    muc_mgr: &mut MucManager,
    muc_config_mgr: &mut MucConfigManager,
    bookmark_mgr: &mut BookmarkManager,
    vcard_edit_mgr: &mut VCardEditManager,
    adhoc_mgr: &mut AdhocManager,
    omemo_mgr: &mut Option<OmemoManager>,
    ignore_mgr: &mut IgnoreManager,
    conv_sync_mgr: &ConversationSyncManager,
    account_mgr: &mut AccountManager,
    account_jid: &str,
    privacy_flags: u8,
) {
    // F1: emit received stanza to debug console before routing
    let xml_str = String::from(&el);
    let _ = event_tx
        .send(XmppEvent::ConsoleEntry {
            direction: "recv".into(),
            xml: xml_str,
        })
        .await;

    // XEP-0198: handle server <a h='...'> acks
    if el.name() == "a" && el.ns() == "urn:xmpp:sm:3" {
        if let Some(h) = el.attr("h").and_then(|v| v.parse::<u32>().ok()) {
            sm.on_ack_received(h);
        }
        return;
    }
    // XEP-0198: handle server <r/> ack requests
    if el.name() == "r" && el.ns() == "urn:xmpp:sm:3" {
        if let Some(ack) = sm.flush_ack() {
            outbox.push_back(ack);
        }
        return;
    }
    match el.name() {
        "iq" => {
            handle_iq(
                el,
                event_tx,
                outbox,
                blocking_mgr,
                mam_mgr,
                catchup_mgr,
                sync_orch,
                disco_mgr,
                file_upload_mgr,
                avatar_mgr,
                muc_config_mgr,
                bookmark_mgr,
                vcard_edit_mgr,
                adhoc_mgr,
                ignore_mgr,
                conv_sync_mgr,
                omemo_mgr,
                account_mgr,
                account_jid,
            )
            .await
        }
        "message" => {
            // L3: XEP-0425 moderation — detect before muc_mgr.on_groupchat_message()
            if el.attr("type") == Some("groupchat") {
                if let Some(apply_to) = el
                    .children()
                    .find(|c| c.name() == "apply-to" && c.ns() == NS_FASTEN)
                {
                    let has_moderated = apply_to
                        .children()
                        .any(|c| c.name() == "moderated" && c.ns() == NS_MODERATION);
                    if has_moderated {
                        if let Some(target_id) = apply_to.attr("id") {
                            let from = el.attr("from").unwrap_or("");
                            let room_jid = from.split('/').next().unwrap_or(from).to_string();
                            let _ = event_tx
                                .send(XmppEvent::MessageModerated {
                                    room_jid,
                                    message_id: target_id.to_string(),
                                })
                                .await;
                        }
                        return;
                    }
                }
            }
            // D3: XEP-0045 groupchat message — route through MucManager
            if let Some(muc_msg) = muc_mgr.on_groupchat_message(&el) {
                let _ = event_tx
                    .send(XmppEvent::MessageReceived(IncomingMessage {
                        id: muc_msg.id,
                        from: format!("{}/{}", muc_msg.room_jid, muc_msg.from_nick),
                        body: muc_msg.body,
                        is_historical: false,
                    }))
                    .await;
                return;
            }
            // C3: XEP-0313 MAM result wrapper — extract forwarded message
            if let Some(mam_msg) = mam_mgr.on_mam_message(&el) {
                // P4.4: notify bulk sync orchestrator of each incoming MAM result
                sync_orch.on_mam_result(mam_msg.clone());
                if !mam_msg.body.is_empty() {
                    let bare_from = mam_msg
                        .forwarded_from
                        .split('/')
                        .next()
                        .unwrap_or(&mam_msg.forwarded_from)
                        .to_string();
                    if !blocking_mgr.is_blocked(&bare_from) {
                        let _ = event_tx
                            .send(XmppEvent::MessageReceived(IncomingMessage {
                                id: mam_msg.archive_id,
                                from: mam_msg.forwarded_from,
                                body: mam_msg.body,
                                is_historical: true,
                            }))
                            .await;
                    }
                }
                return;
            }
            // XEP-0280: carbon <sent> — own message from another device
            if let Some(inner) = extract_carbon(&el, "sent") {
                let body = inner
                    .children()
                    .find(|c| c.name() == "body")
                    .map(Element::text)
                    .unwrap_or_default();
                if !body.is_empty() {
                    let _ = event_tx
                        .send(XmppEvent::MessageReceived(IncomingMessage {
                            id: inner.attr("id").unwrap_or("").to_string(),
                            from: own_jid_str.to_string(),
                            body,
                            is_historical: false,
                        }))
                        .await;
                }
                return;
            }
            // XEP-0280: carbon <received> — message received on another device
            if let Some(inner) = extract_carbon(&el, "received") {
                handle_message(inner, event_tx, blocking_mgr, outbox, privacy_flags).await;
                return;
            }
            // J6: XEP-0084 PEP avatar metadata notification
            // <message><event xmlns="...pubsub#event"><items node="urn:xmpp:avatar:metadata">
            // Note: some servers use the draft namespace "urn:xmpp:avatar:metadata:2", both handled.
            {
                let is_avatar_meta = el.children().any(|c| {
                    c.name() == "event"
                        && c.ns() == "http://jabber.org/protocol/pubsub#event"
                        && c.children().any(|items| {
                            items.name() == "items"
                                && matches!(
                                    items.attr("node"),
                                    Some("urn:xmpp:avatar:metadata")
                                        | Some("urn:xmpp:avatar:metadata:2")
                                )
                        })
                });
                if is_avatar_meta {
                    let from_jid = el.attr("from").unwrap_or("").to_string();
                    if let Some(info) = avatar_mgr.on_avatar_metadata_event(&from_jid, &el) {
                        // Fetch the actual avatar data
                        let fetch_iq = avatar_mgr.build_avatar_data_request(&info.jid, &info.sha1);
                        outbox.push_back(fetch_iq);
                        tracing::debug!("avatar: fetching XEP-0084 data for {from_jid}");
                    }
                    return;
                }
            }
            // L3: XEP-0080 GeoLoc PEP notification
            // <message><event xmlns="...pubsub#event"><items node="http://jabber.org/protocol/geoloc">
            {
                let is_geoloc_event = el.children().any(|c| {
                    c.name() == "event"
                        && c.ns() == NS_PUBSUB_EVENT
                        && c.children().any(|items| {
                            items.name() == "items" && items.attr("node") == Some(NS_GEOLOC)
                        })
                });
                if is_geoloc_event {
                    let from = el.attr("from").unwrap_or("").to_string();
                    if let Some(location) = geoloc::parse_geoloc(&el) {
                        let _ = event_tx
                            .send(XmppEvent::LocationReceived {
                                _from: from,
                                _location: location,
                            })
                            .await;
                    }
                    return;
                }
            }
            // MEMO: Check for OMEMO device list PEP push before plain-text path.
            if let Some(from_jid) = DeviceManager::is_device_list_event(&el) {
                let devices = DeviceManager::parse_device_list(&el);
                // Persist device list update in the store.
                if let Some(ref mut mgr) = omemo_mgr {
                    if let Err(e) = mgr
                        .store
                        .sync_device_list(account_jid, &from_jid, &devices)
                        .await
                    {
                        tracing::warn!("omemo: sync_device_list failed for {from_jid}: {e}");
                    } else {
                        // Auto-fetch bundles for devices that have no Olm session yet.
                        for &device_id in &devices {
                            let has_session = mgr
                                .store
                                .load_session(account_jid, &from_jid, device_id)
                                .await
                                .unwrap_or(None)
                                .is_some();
                            if !has_session {
                                let (iq_id, iq) =
                                    mgr.device_mgr.build_bundle_fetch(&from_jid, device_id);
                                mgr.track_bundle_fetch(iq_id, from_jid.clone(), device_id);
                                outbox.push_back(iq);
                                tracing::debug!(
                                    "omemo: fetching bundle for {from_jid}/{device_id}"
                                );
                            }
                        }
                    }
                }
                let _ = event_tx
                    .send(XmppEvent::OmemoDeviceListReceived {
                        jid: from_jid,
                        devices,
                    })
                    .await;
                return;
            }
            // MEMO: Check for OMEMO <encrypted> stanza (incoming encrypted message).
            {
                if has_omemo_encrypted(&el) {
                    if let Some(ref mut mgr) = omemo_mgr {
                        let from = el.attr("from").unwrap_or("").to_string();
                        match omemo_try_decrypt(mgr, account_jid, &el).await {
                            Ok(Some(body)) => {
                                let _ = event_tx
                                    .send(XmppEvent::OmemoMessageDecrypted { from, body })
                                    .await;
                                // Check pre-key stock and replenish if below threshold.
                                omemo_check_prekey_rotation(mgr, account_jid, outbox).await;
                            }
                            Ok(None) => {
                                tracing::debug!(
                                    "omemo: key-transport message (no payload), ignored"
                                );
                                // A key-transport also consumes a pre-key -- check rotation.
                                omemo_check_prekey_rotation(mgr, account_jid, outbox).await;
                            }
                            Err(e) => {
                                tracing::warn!("omemo: decrypt failed from {from}: {e}");
                            }
                        }
                    } else {
                        tracing::warn!("omemo: received encrypted message but OMEMO not available");
                    }
                    return;
                }
            }
            handle_message(el, event_tx, blocking_mgr, outbox, privacy_flags).await;
        }
        "presence" => {
            // D3: update MUC occupant lists from room presence stanzas
            muc_mgr.on_presence(&el);
            // K1: detect room-created status code 201 in MUC owner presence
            {
                let is_room_created = el.children().any(|c| {
                    c.name() == "x"
                        && c.ns() == NS_MUC_USER
                        && c.children()
                            .any(|s| s.name() == "status" && s.attr("code") == Some("201"))
                });
                if is_room_created {
                    if let Some(from) = el.attr("from") {
                        let room_jid = from.split('/').next().unwrap_or(from).to_string();
                        let (_, iq) = muc_config_mgr.build_config_request(&room_jid);
                        outbox.push_back(iq);
                        tracing::info!("muc: room {room_jid} created, requesting config form");
                    }
                }
            }
            handle_presence(el, event_tx).await;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Carbon helper (XEP-0280)
// ---------------------------------------------------------------------------

/// Extract the inner forwarded `<message>` from a carbon wrapper.
/// Returns `None` if the element is not a carbon of the given direction.
fn extract_carbon(el: &Element, direction: &str) -> Option<Element> {
    let carbon = el
        .children()
        .find(|c| c.name() == direction && c.ns() == NS_CARBONS)?;
    let forwarded = carbon
        .children()
        .find(|c| c.name() == "forwarded" && c.ns() == NS_FORWARD)?;
    forwarded
        .children()
        .find(|c| c.name() == "message")
        .cloned()
}

// ---------------------------------------------------------------------------
// Stanza builders
// ---------------------------------------------------------------------------

fn make_roster_get() -> Element {
    Iq::from_get(
        uuid::Uuid::new_v4().to_string(),
        Roster {
            ver: None,
            items: vec![],
        },
    )
    .into()
}

fn make_carbons_enable() -> Element {
    // XEP-0280: <iq type="set"><enable xmlns="urn:xmpp:carbons:2"/></iq>
    Iq::from_set(
        uuid::Uuid::new_v4().to_string(),
        xmpp_parsers::carbons::Enable,
    )
    .into()
}

fn make_presence_with_caps(disco_mgr: &DiscoManager, status_message: Option<&str>) -> Element {
    let caps_el = disco_mgr.build_caps_element();
    let mut raw = Element::builder("presence", "jabber:client").build();
    raw.append_child(caps_el);
    // J2: Include custom status message if set
    if let Some(msg) = status_message {
        let status_el = Element::builder("status", "jabber:client")
            .append(msg)
            .build();
        raw.append_child(status_el);
    }
    raw
}

fn make_message(to: Jid, body: &str, id: &str) -> Element {
    let mut msg = XmppMessage::new(Some(to));
    msg.type_ = MessageType::Chat;
    msg.id = Some(id.to_string());
    msg.bodies.insert(String::new(), Body(body.to_owned()));
    let mut el: Element = msg.into();
    // K4: request a delivery receipt (XEP-0184)
    el.append_child(Element::builder("request", NS_RECEIPTS).build());
    el
}

/// G2: Build a XEP-0085 chat state stanza.
fn make_chat_state_message(to: Jid, composing: bool) -> Element {
    // Build raw minidom element: <message type="chat" to="..."><composing|paused xmlns="jabber:x:chatstates"/></message>
    let state_name = if composing { "composing" } else { "paused" };
    let state_el = Element::builder(state_name, "jabber:x:chatstates").build();
    let mut msg = XmppMessage::new(Some(to));
    msg.type_ = MessageType::Chat;
    let el: Element = msg.into();
    // Reconstruct with the child since XmppMessage doesn't support arbitrary payloads cleanly
    let mut raw = Element::builder("message", "jabber:client")
        .attr("type", "chat")
        .attr("to", el.attr("to").unwrap_or(""))
        .build();
    raw.append_child(state_el);
    raw
}

/// H3: Build a roster-set IQ to add a contact.
fn make_roster_set(jid: &str) -> Element {
    let item = Element::builder("item", "jabber:iq:roster")
        .attr("jid", jid)
        .build();
    let query = Element::builder("query", "jabber:iq:roster")
        .append(item)
        .build();
    let mut iq = Element::builder("iq", "jabber:client")
        .attr("type", "set")
        .attr("id", uuid::Uuid::new_v4().to_string())
        .build();
    iq.append_child(query);
    iq
}

/// L3: Build a XEP-0425 message moderation stanza for a MUC room.
pub fn make_moderation_message(room_jid: &str, target_id: &str, reason: Option<&str>) -> Element {
    let mut moderated = Element::builder("moderated", NS_MODERATION)
        .append(Element::builder("retract", "urn:xmpp:message-retract:1").build());
    if let Some(r) = reason {
        moderated = moderated.append(Element::builder("reason", NS_MODERATION).append(r).build());
    }
    let apply_to = Element::builder("apply-to", NS_FASTEN)
        .attr("id", target_id)
        .append(moderated.build())
        .build();
    Element::builder("message", "jabber:client")
        .attr("to", room_jid)
        .attr("type", "groupchat")
        .attr("id", uuid::Uuid::new_v4().to_string())
        .append(apply_to)
        .build()
}

/// D4: Build a private-XML-get IQ to fetch bookmarks (XEP-0048).
fn make_bookmarks_get_iq() -> Element {
    let storage = Element::builder("storage", "storage:bookmarks").build();
    let query = Element::builder("query", "jabber:iq:private")
        .append(storage)
        .build();
    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("id", "bookmarks-get")
        .append(query)
        .build()
}

/// K5: Build an XEP-0333 <displayed> chat marker message.
fn make_displayed_message(to: &str, id: &str) -> Element {
    Element::builder("message", "jabber:client")
        .attr("to", to)
        .append(
            Element::builder("displayed", NS_CHAT_MARKERS)
                .attr("id", id)
                .build(),
        )
        .build()
}

/// J10: Build a MAM prefs get IQ (XEP-0313).
fn make_mam_prefs_get_iq() -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "get")
        .attr("id", uuid::Uuid::new_v4().to_string())
        .append(Element::builder("prefs", NS_MAM).build())
        .build()
}

/// J10: Build a MAM prefs set IQ with the given default archiving mode.
fn make_mam_prefs_set(default_mode: &str) -> Element {
    Element::builder("iq", "jabber:client")
        .attr("type", "set")
        .attr("id", uuid::Uuid::new_v4().to_string())
        .append(
            Element::builder("prefs", NS_MAM)
                .attr("default", default_mode)
                .build(),
        )
        .build()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_host_port(input: &str, default_port: u16) -> (String, u16) {
    match input.trim().rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(default_port)),
        None => (input.trim().to_string(), default_port),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Registration session (J9 / XEP-0077)
// ---------------------------------------------------------------------------

async fn run_registration_session(
    config: ConnectConfig,
    event_tx: &mpsc::Sender<XmppEvent>,
    cmd_rx: &mut mpsc::Receiver<XmppCommand>,
) {
    let jid: Jid = match config.jid.parse() {
        Ok(j) => j,
        Err(e) => {
            let _ = event_tx
                .send(XmppEvent::RegistrationFailure(format!("Invalid JID: {e}")))
                .await;
            return;
        }
    };

    // DC-19: honour manual_srv override in the registration path too.
    //   1. Explicit host in `server` field → use it directly.
    //   2. `manual_srv` set → resolve that SRV record via dns::resolve_with_override.
    //   3. Neither → let tokio-xmpp do standard RFC 6120 SRV discovery.
    let server = if !config.server.trim().is_empty() {
        let (host, port) = parse_host_port(&config.server, 5222);
        ServerConfig::Manual { host, port }
    } else if let Some(ref srv_name) = config.manual_srv {
        match dns::resolve_with_override(jid.domain().as_str(), Some(srv_name.as_str())).await {
            Ok(ep) => ServerConfig::Manual {
                host: ep.host,
                port: ep.port,
            },
            Err(e) => {
                tracing::warn!(
                    "engine: registration SRV override failed ({}), falling back to UseSrv",
                    e
                );
                ServerConfig::UseSrv
            }
        }
    } else {
        ServerConfig::UseSrv
    };

    // Connect manually via ServerConfig. This bypasses AsyncClient's auto-auth.
    let mut stream = match server.connect(&jid, "jabber:client").await {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx
                .send(XmppEvent::RegistrationFailure(format!(
                    "Connection failed: {e}"
                )))
                .await;
            return;
        }
    };

    // Request registration form fields from the server.
    let get_iq = RegistrationManager::build_get_form("reg1");
    if let Err(e) = stream.send(tokio_xmpp::Packet::Stanza(get_iq)).await {
        let _ = event_tx
            .send(XmppEvent::RegistrationFailure(format!(
                "Failed to send request: {e}"
            )))
            .await;
        return;
    }

    loop {
        tokio::select! {
            maybe_event = stream.next() => {
                match maybe_event {
                    Some(Ok(tokio_xmpp::Packet::Stanza(el))) => {
                        if let Some(query) = RegistrationManager::parse_registration_query(&el) {
                            // Send form to UI for user interaction.
                            let _ = event_tx.send(XmppEvent::RegistrationFormReceived {
                                _server: config.server.clone(),
                                _form: query,
                            }).await;
                        } else if el.name() == "iq" && el.attr("type") == Some("result") {
                            // IQ result without query often means successful registration submission.
                            let _ = event_tx.send(XmppEvent::RegistrationSuccess).await;
                            return;
                        } else if el.name() == "iq" && el.attr("type") == Some("error") {
                            // Registration failed.
                            let reason = el.get_child("error", "jabber:client")
                                .or_else(|| el.get_child("error", "urn:ietf:params:xml:ns:xmpp-stanzas"))
                                .map_or("unknown error", |e: &Element| e.name());
                            let _ = event_tx.send(XmppEvent::RegistrationFailure(reason.to_string())).await;
                            return;
                        }
                    }
                    Some(Err(e)) => {
                        let _ = event_tx.send(XmppEvent::RegistrationFailure(format!("Stream error: {e}"))).await;
                        return;
                    }
                    Some(Ok(_)) => {} // Ignore other stream events (features, etc.) during registration.
                    None => {
                        let _ = event_tx.send(XmppEvent::RegistrationFailure("Stream closed prematurely".into())).await;
                        return;
                    }
                }
            }
            maybe_cmd = cmd_rx.recv() => {
                match maybe_cmd {
                    Some(XmppCommand::SubmitRegistration { server: _, form }) => {
                        let submit_iq = RegistrationManager::build_registration_form_submit("reg2", form);
                        if let Err(e) = stream.send(tokio_xmpp::Packet::Stanza(submit_iq)).await {
                             let _ = event_tx.send(XmppEvent::RegistrationFailure(format!("Failed to send submission: {e}"))).await;
                             return;
                        }
                    }
                    Some(XmppCommand::Disconnect) => {
                        let _ = stream.close().await;
                        return;
                    }
                    Some(_) => {} // Ignore other commands during registration.
                    None => return,
                }
            }
        }
    }
}

/// Returns true if the error string indicates an authentication failure.
///
/// When true, the engine should emit `Disconnected` instead of `Reconnecting`
/// because retrying with the same credentials would never succeed.
pub fn is_auth_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("not-authorized")
        || lower.contains("authentication")
        || lower.contains("credentials")
        || lower.contains("sasl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmpp::connection::sasl::SaslMechanism;
    use crate::xmpp::RosterContact;

    // --- XmppEvent derives ---

    #[test]
    fn xmpp_event_debug_clone() {
        let e = XmppEvent::Connected {
            bound_jid: "user@example.com/res".into(),
        };
        let cloned = e.clone();
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn xmpp_event_disconnected_debug_clone() {
        let e = XmppEvent::Disconnected {
            reason: "test".into(),
        };
        let cloned = e.clone();
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn xmpp_event_reconnecting_debug_clone() {
        let e = XmppEvent::Reconnecting { attempt: 3 };
        let cloned = e.clone();
        let _ = format!("{cloned:?}");
    }

    #[test]
    fn xmpp_event_roster_received() {
        let contacts = vec![RosterContact {
            jid: "alice@example.com".into(),
            name: Some("Alice".into()),
            subscription: "Both".into(),
        }];
        let e = XmppEvent::RosterReceived(contacts);
        let _ = format!("{e:?}");
    }

    #[test]
    fn xmpp_event_message_received() {
        let e = XmppEvent::MessageReceived(IncomingMessage {
            id: "m1".into(),
            from: "alice@example.com".into(),
            body: "Hello!".into(),
            is_historical: false,
        });
        let _ = format!("{e:?}");
    }

    #[test]
    fn xmpp_event_presence_updated() {
        let e = XmppEvent::PresenceUpdated {
            jid: "alice@example.com".into(),
            available: true,
        };
        let _ = format!("{e:?}");
    }

    // --- SaslMechanism::select ---

    #[test]
    fn sasl_select_prefers_scram_sha256() {
        let offered = vec![
            "PLAIN".to_string(),
            "SCRAM-SHA-1".to_string(),
            "SCRAM-SHA-256".to_string(),
        ];
        assert_eq!(
            SaslMechanism::select(&offered),
            Some(SaslMechanism::ScramSha256)
        );
    }

    #[test]
    fn sasl_select_falls_back_to_scram_sha1() {
        let offered = vec!["PLAIN".to_string(), "SCRAM-SHA-1".to_string()];
        assert_eq!(
            SaslMechanism::select(&offered),
            Some(SaslMechanism::ScramSha1)
        );
    }

    #[test]
    fn sasl_select_falls_back_to_plain() {
        let offered = vec!["PLAIN".to_string()];
        assert_eq!(SaslMechanism::select(&offered), Some(SaslMechanism::Plain));
    }

    #[test]
    fn sasl_select_returns_none_when_nothing_matches() {
        let offered = vec!["GSSAPI".to_string(), "EXTERNAL".to_string()];
        assert_eq!(SaslMechanism::select(&offered), None);
    }

    #[test]
    fn sasl_select_empty_offered() {
        assert_eq!(SaslMechanism::select(&[]), None);
    }

    // --- parse_host_port ---

    #[test]
    fn parse_host_port_bare_domain() {
        let (host, port) = parse_host_port("example.com", 5222);
        assert_eq!(host, "example.com");
        assert_eq!(port, 5222);
    }

    #[test]
    fn parse_host_port_with_port() {
        let (host, port) = parse_host_port("example.com:5223", 5222);
        assert_eq!(host, "example.com");
        assert_eq!(port, 5223);
    }

    // --- Engine command channel ---

    #[tokio::test]
    async fn engine_idle_exits_when_channel_closed() {
        let (event_tx, _event_rx) = mpsc::channel::<XmppEvent>(8);
        let (_cmd_tx, cmd_rx) = mpsc::channel::<XmppCommand>(8);

        // Drop cmd_tx immediately — engine should return because the channel closed.
        drop(_cmd_tx);

        // Should return quickly (channel is already closed).
        run_engine(event_tx, cmd_rx, None).await;
    }

    // --- Carbon detection (XEP-0280) ---

    fn make_carbon_wrapper(direction: &str, inner_xml: &str) -> Element {
        let inner_msg: Element = inner_xml.parse().unwrap();
        let mut forwarded = Element::builder("forwarded", NS_FORWARD).build();
        forwarded.append_child(inner_msg);
        let mut carbon = Element::builder(direction, NS_CARBONS).build();
        carbon.append_child(forwarded);
        let mut wrapper = Element::builder("message", "jabber:client")
            .attr("from", "user@example.com")
            .attr("to", "user@example.com/res2")
            .build();
        wrapper.append_child(carbon);
        wrapper
    }

    #[test]
    fn extract_carbon_sent_returns_inner_message() {
        let inner = r#"<message xmlns="jabber:client" from="user@example.com/res1" to="alice@example.com" type="chat"><body>hello</body></message>"#;
        let wrapper = make_carbon_wrapper("sent", inner);
        let result = extract_carbon(&wrapper, "sent");
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.name(), "message");
        assert_eq!(msg.attr("to"), Some("alice@example.com"));
    }

    #[test]
    fn extract_carbon_received_returns_inner_message() {
        let inner = r#"<message xmlns="jabber:client" from="alice@example.com" to="user@example.com/res1" type="chat"><body>hi</body></message>"#;
        let wrapper = make_carbon_wrapper("received", inner);
        let result = extract_carbon(&wrapper, "received");
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.name(), "message");
        assert_eq!(msg.attr("from"), Some("alice@example.com"));
    }

    #[test]
    fn extract_carbon_wrong_direction_returns_none() {
        let inner = r#"<message xmlns="jabber:client" from="alice@example.com" to="user@example.com/res1" type="chat"><body>hi</body></message>"#;
        let wrapper = make_carbon_wrapper("received", inner);
        // Looking for "sent" in a "received" wrapper should return None.
        assert!(extract_carbon(&wrapper, "sent").is_none());
    }

    #[test]
    fn extract_carbon_plain_message_returns_none() {
        let plain: Element = r#"<message xmlns="jabber:client" from="alice@example.com" to="user@example.com" type="chat"><body>hello</body></message>"#.parse().unwrap();
        assert!(extract_carbon(&plain, "sent").is_none());
        assert!(extract_carbon(&plain, "received").is_none());
    }

    #[test]
    fn connect_config_fields() {
        let cfg = ConnectConfig {
            jid: "user@example.com".into(),
            password: "secret".into(),
            server: "xmpp.example.com:5222".into(),
            status_message: Some("In a meeting".into()),
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
            proxy_type: None,
            proxy_host: None,
            proxy_port: None,
            manual_srv: None,
            push_service_jid: None,
        };
        assert_eq!(cfg.jid, "user@example.com");
        assert_eq!(cfg.server, "xmpp.example.com:5222");
        assert_eq!(cfg.status_message, Some("In a meeting".into()));
        assert!(cfg.proxy_config().is_none());
    }

    #[test]
    fn connect_config_proxy_config_some() {
        let cfg = ConnectConfig {
            jid: "user@example.com".into(),
            password: "secret".into(),
            server: String::new(),
            status_message: None,
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
            proxy_type: Some("socks5".into()),
            proxy_host: Some("proxy.example.com".into()),
            proxy_port: Some(1080),
            manual_srv: None,
            push_service_jid: None,
        };
        let pc = cfg.proxy_config();
        assert!(pc.is_some());
        let (t, h, p) = pc.unwrap();
        assert_eq!(t, "socks5");
        assert_eq!(h, "proxy.example.com");
        assert_eq!(p, 1080);
    }

    #[test]
    fn connect_config_proxy_config_none_when_partial() {
        let cfg = ConnectConfig {
            jid: "user@example.com".into(),
            password: "secret".into(),
            server: String::new(),
            status_message: None,
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
            proxy_type: Some("http".into()),
            proxy_host: None,
            proxy_port: Some(8080),
            manual_srv: None,
            push_service_jid: None,
        };
        assert!(cfg.proxy_config().is_none());
    }

    // --- StanzaRateLimiter ---

    #[test]
    fn try_consume_succeeds_up_to_capacity() {
        let capacity = 5.0_f64;
        let mut limiter = StanzaRateLimiter::new(capacity, 1.0);
        for _ in 0..capacity as usize {
            assert!(limiter.try_consume());
        }
    }

    #[test]
    fn try_consume_fails_when_exhausted() {
        let capacity = 3.0_f64;
        let mut limiter = StanzaRateLimiter::new(capacity, 1.0);
        for _ in 0..capacity as usize {
            limiter.try_consume();
        }
        assert!(!limiter.try_consume());
    }

    #[tokio::test(start_paused = true)]
    async fn tokens_refill_over_time() {
        let mut limiter = StanzaRateLimiter::new(5.0, 5.0);
        // Drain all tokens.
        for _ in 0..5 {
            assert!(limiter.try_consume());
        }
        assert!(!limiter.try_consume());

        // Advance time by 1 second — refill rate is 5/s so at least one token should be available.
        tokio::time::advance(std::time::Duration::from_secs(1)).await;

        assert!(limiter.try_consume());
    }

    #[test]
    fn warn_if_high_depth_does_not_panic() {
        let limiter = StanzaRateLimiter::new(10.0, 10.0);
        limiter.warn_if_high_depth(0);
        limiter.warn_if_high_depth(MAX_OUTBOX_DEPTH - 1);
        limiter.warn_if_high_depth(MAX_OUTBOX_DEPTH);
        limiter.warn_if_high_depth(MAX_OUTBOX_DEPTH + 1);
        limiter.warn_if_high_depth(usize::MAX);
    }
}
