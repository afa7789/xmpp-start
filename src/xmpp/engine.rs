// Task P1.1  — TCP + STARTTLS (via tokio-xmpp AsyncClient<ServerConfig>)
// Task P1.2  — SASL authentication (handled by tokio-xmpp)
// Task P1.3  — XML stream parser (xmpp-parsers)
// Task P1.4  — RFC 6121 Roster + presence
// Task P1.5  — Message send/receive + XEP-0280 Carbons
// Task P1.7  — DNS SRV (tokio-xmpp ServerConfig::UseSrv)
// Task P1.9  — Connection state machine

use std::collections::VecDeque;

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_xmpp::{
    jid::Jid,
    minidom::Element,
    parsers::{
        iq::Iq,
        message::{Body, Message as XmppMessage, MessageType},
        presence::{Presence, Type as PresenceType},
        roster::Roster,
    },
    starttls::ServerConfig,
    AsyncClient, AsyncConfig,
};

use super::{
    connection::ConnectConfig,
    modules::avatar::AvatarManager,
    modules::blocking::BlockingManager,
    modules::bookmarks::BookmarkManager,
    modules::catchup::CatchupManager,
    modules::disco::{DiscoIdentity, DiscoManager},
    modules::file_upload::FileUploadManager,
    modules::mam::{MamFilter, MamManager, MamQuery, RsmQuery},
    modules::muc::MucManager,
    modules::presence_machine::PresenceMachine,
    modules::stream_mgmt::StreamMgmt,
    IncomingMessage, RosterContact, XmppCommand, XmppEvent,
};

const NS_CARBONS: &str = "urn:xmpp:carbons:2";
const NS_FORWARD: &str = "urn:xmpp:forward:0";
const NS_MAM: &str = "urn:xmpp:mam:2";

// ---------------------------------------------------------------------------
// Connection state machine  (P1.9)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
enum EngineState {
    Idle,
    Running,
}

// ---------------------------------------------------------------------------
// Public engine entry-point
// ---------------------------------------------------------------------------

/// Runs the XMPP engine loop.
///
/// Waits for the first [`XmppCommand::Connect`] before dialling the server.
/// On disconnect the engine returns to the idle state and waits again.
pub async fn run_engine(
    event_tx: mpsc::Sender<XmppEvent>,
    mut cmd_rx: mpsc::Receiver<XmppCommand>,
) {
    let mut state = EngineState::Idle;

    loop {
        match state {
            EngineState::Idle => {
                // Wait for a Connect command.
                match cmd_rx.recv().await {
                    Some(XmppCommand::Connect(config)) => {
                        tracing::info!("engine: connecting as {}", config.jid);
                        run_session(config, &event_tx, &mut cmd_rx).await;
                        // stay Idle for the next iteration
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
            EngineState::Running => {
                // run_session returned; go back to idle.
                state = EngineState::Idle;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session loop
// ---------------------------------------------------------------------------

async fn run_session(
    config: ConnectConfig,
    event_tx: &mpsc::Sender<XmppEvent>,
    cmd_rx: &mut mpsc::Receiver<XmppCommand>,
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

    // Build the server connector.  If the user specified a manual host we use
    // that; otherwise tokio-xmpp does DNS SRV resolution automatically (P1.7).
    let server = if config.server.trim().is_empty() {
        ServerConfig::UseSrv
    } else {
        let (host, port) = parse_host_port(&config.server, 5222);
        ServerConfig::Manual { host, port }
    };

    let mut client = AsyncClient::new_with_config(AsyncConfig {
        jid,
        password: config.password.clone(),
        server,
    });
    client.set_reconnect(false); // we manage reconnect ourselves

    // Outbox for stanzas that need to be sent after a select! arm.
    let mut outbox: VecDeque<Element> = VecDeque::new();
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
        "https://github.com/xmpp-start",
        &[DiscoIdentity {
            category: "client".to_string(),
            kind: "pc".to_string(),
            name: "xmpp-start".to_string(),
        }],
        &["urn:xmpp:mam:2", "urn:xmpp:carbons:2"],
    );
    // E4: XEP-0363 file upload manager
    let mut file_upload_mgr = FileUploadManager::new();
    // H1: avatar manager (vCard-temp fallback)
    let mut avatar_mgr = AvatarManager::new();
    // D3: XEP-0045 multi-user chat manager
    let mut muc_mgr = MucManager::new();
    // D4: XEP-0048 bookmarks manager
    let mut bookmark_mgr = BookmarkManager::new();

    loop {
        // Drain outbox before blocking on the next event.
        while let Some(stanza) = outbox.pop_front() {
            // C1: record sent stanza and check for queue desync
            sm.on_stanza_sent(stanza.clone());
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

                        handle_client_event(ev, event_tx, &mut outbox, &mut reconnect_attempt, &mut sm, &mut blocking_mgr, &mut own_jid_str, &mut mam_mgr, &mut catchup_mgr, &mut presence_machine, &mut disco_mgr, &mut file_upload_mgr, &mut avatar_mgr, &mut muc_mgr, &mut bookmark_mgr).await;
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
                        // Already running; ignore.
                    }
                    Some(XmppCommand::SendMessage { to, body }) => {
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(make_message(to_jid, &body));
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
                    Some(XmppCommand::SendChatState { to, composing }) => {
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(make_chat_state_message(to_jid, composing));
                        }
                    }
                    Some(XmppCommand::AddContact(jid)) => {
                        outbox.push_back(make_roster_set(&jid));
                        tracing::info!("roster: sent add-contact IQ for {jid}");
                    }
                    Some(XmppCommand::RequestUploadSlot { filename, size, mime }) => {
                        // E4: request upload slot from server's upload service
                        // Use a well-known upload service JID pattern; in production, discover via disco.
                        let upload_jid = "upload.".to_string() + config.jid.split('@').nth(1).unwrap_or("example.com");
                        let (_, iq) = file_upload_mgr.request_slot(&filename, size, &mime, &upload_jid);
                        outbox.push_back(iq);
                        tracing::info!("file_upload: requested slot for {filename}");
                    }
                    Some(XmppCommand::FetchAvatar(jid)) => {
                        let (_, iq) = avatar_mgr.build_vcard_request(&jid);
                        outbox.push_back(iq);
                        tracing::debug!("avatar: fetching vCard for {jid}");
                    }
                    Some(XmppCommand::SendReaction { to, msg_id, emojis }) => {
                        // E3: Build XEP-0444 reaction stanza
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            let mut msg_el = Element::builder("message", "jabber:client")
                                .attr("to", to_jid.to_string())
                                .attr("type", "chat")
                                .build();
                            let mut reactions_el = Element::builder("reactions", "urn:xmpp:reactions:0")
                                .attr("id", &msg_id)
                                .build();
                            for emoji in &emojis {
                                let reaction_el = Element::builder("reaction", "urn:xmpp:reactions:0")
                                    .append(emoji.as_str())
                                    .build();
                                reactions_el.append_child(reaction_el);
                            }
                            msg_el.append_child(reactions_el);
                            outbox.push_back(msg_el);
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
                        // E1: XEP-0308 message correction
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(make_correction_message(to_jid, &original_id, &new_body));
                        }
                    }
                    Some(XmppCommand::SendRetraction { to, origin_id }) => {
                        // E2: XEP-0424 message retraction
                        if let Ok(to_jid) = to.parse::<Jid>() {
                            outbox.push_back(make_retraction_message(to_jid, &origin_id));
                        }
                    }
                    Some(XmppCommand::JoinRoom { jid, nick }) => {
                        // D3: XEP-0045 MUC join
                        outbox.push_back(muc_mgr.join_room(&jid, &nick));
                        tracing::info!("muc: joining room {jid} as {nick}");
                    }
                    Some(XmppCommand::LeaveRoom(jid)) => {
                        // D3: XEP-0045 MUC leave
                        if let Some(stanza) = muc_mgr.leave_room(&jid) {
                            outbox.push_back(stanza);
                            tracing::info!("muc: leaving room {jid}");
                        }
                    }
                    Some(XmppCommand::RemoveContact(_))
                    | Some(XmppCommand::RenameContact { .. })
                    | Some(XmppCommand::FetchVCard(_))
                    | Some(XmppCommand::FetchHistory { .. }) => {
                        // Not yet implemented — silently ignore.
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
    bookmark_mgr: &mut BookmarkManager,
) {
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

            let _ = event_tx
                .send(XmppEvent::Connected {
                    bound_jid: bound_jid.to_string(),
                })
                .await;
        }

        tokio_xmpp::Event::Disconnected(err) => {
            *reconnect_attempt += 1;
            presence_machine.on_disconnected();
            let unacked = sm.unacked_stanzas().len();
            tracing::warn!(
                "engine: disconnected — {err} ({unacked} unacked stanzas, h={})",
                sm.h()
            );
            // C1: reset stream management counters for the next session
            sm.reset();
            let _ = event_tx
                .send(XmppEvent::Reconnecting {
                    attempt: *reconnect_attempt,
                })
                .await;
        }

        tokio_xmpp::Event::Stanza(el) => {
            // C1: record inbound stanza and maybe send coalesced ack
            sm.on_stanza_received();
            if let Some(ack) = sm.maybe_send_ack() {
                outbox.push_back(ack);
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
                disco_mgr,
                file_upload_mgr,
                avatar_mgr,
                muc_mgr,
                bookmark_mgr,
            )
            .await;
        }
    }
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
    disco_mgr: &mut DiscoManager,
    file_upload_mgr: &mut FileUploadManager,
    avatar_mgr: &mut AvatarManager,
    muc_mgr: &mut MucManager,
    bookmark_mgr: &mut BookmarkManager,
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
                disco_mgr,
                file_upload_mgr,
                avatar_mgr,
                bookmark_mgr,
            )
            .await
        }
        "message" => {
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
                handle_message(inner, event_tx, blocking_mgr).await;
                return;
            }
            handle_message(el, event_tx, blocking_mgr).await;
        }
        "presence" => {
            // D3: update MUC occupant lists from room presence stanzas
            muc_mgr.on_presence(&el);
            handle_presence(el, event_tx).await;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// IQ handler — roster result (P1.4)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn handle_iq(
    el: Element,
    event_tx: &mpsc::Sender<XmppEvent>,
    outbox: &mut VecDeque<Element>,
    blocking_mgr: &mut BlockingManager,
    mam_mgr: &mut MamManager,
    catchup_mgr: &mut CatchupManager,
    disco_mgr: &mut DiscoManager,
    file_upload_mgr: &mut FileUploadManager,
    avatar_mgr: &mut AvatarManager,
    bookmark_mgr: &mut BookmarkManager,
) {
    // C5: respond to disco#info get requests with our feature list
    if el.attr("type") == Some("get") {
        let has_disco_info = el
            .children()
            .any(|c| c.name() == "query" && c.ns() == "http://jabber.org/protocol/disco#info");
        if has_disco_info {
            let iq_id = el.attr("id").unwrap_or("").to_string();
            let requester = el.attr("from").unwrap_or("").to_string();
            outbox.push_back(disco_mgr.build_info_response(&iq_id, &requester));
            tracing::debug!("disco: responded to disco#info get from {requester}");
            return;
        }
    }
    // C3: detect MAM <fin> stanza
    if el.attr("type") == Some("result") {
        let has_fin = el.children().any(|c| c.name() == "fin" && c.ns() == NS_MAM);
        if has_fin {
            if let Some((query_id, mam_result)) = mam_mgr.on_fin_iq(&el) {
                let fetched = mam_result.messages.len();
                // Use on_result to peek at conversation jid, then call on_fin
                let conv_jid = catchup_mgr
                    .on_result(&query_id, "__server__")
                    .unwrap_or("__server__")
                    .to_string();
                catchup_mgr.on_fin(&query_id);
                let _ = event_tx
                    .send(XmppEvent::CatchupFinished {
                        conversation_jid: conv_jid,
                        fetched,
                    })
                    .await;
            }
            return;
        }
    }
    // E4: detect upload slot result
    if let Some(slot) = file_upload_mgr.on_slot_result(&el) {
        let _ = event_tx
            .send(XmppEvent::UploadSlotReceived {
                put_url: slot.put_url,
                get_url: slot.get_url,
                headers: slot.put_headers,
            })
            .await;
        return;
    }
    // H1: detect vCard result and extract PHOTO/BINVAL
    if let Some(avatar_info) = avatar_mgr.on_vcard_result(&el) {
        if !avatar_info.data.is_empty() {
            let _ = event_tx
                .send(XmppEvent::AvatarReceived {
                    jid: avatar_info.jid,
                    png_bytes: avatar_info.data,
                })
                .await;
        }
        return;
    }
    // C5: parse disco#info results into cache
    if disco_mgr.on_info_result(&el).is_some() {
        return;
    }
    // C4: blocklist result (initial fetch)
    if el.attr("type") == Some("result") {
        let has_blocklist = el
            .children()
            .any(|c| c.name() == "blocklist" && c.ns() == "urn:xmpp:blocking");
        if has_blocklist {
            blocking_mgr.on_blocklist_result(&el);
            tracing::debug!(
                "blocking: loaded {} blocked JIDs",
                blocking_mgr.blocked_list().len()
            );
            return;
        }
    }

    // D4: detect bookmarks result (private XML storage, XEP-0048)
    if el.attr("type") == Some("result") {
        let has_private = el
            .children()
            .any(|c| c.name() == "query" && c.ns() == "jabber:iq:private");
        if has_private {
            let bookmarks = BookmarkManager::parse_bookmarks_from_iq(&el);
            if !bookmarks.is_empty() || el.children().any(|_| true) {
                bookmark_mgr.set_bookmarks(bookmarks.clone());
                tracing::info!("bookmarks: loaded {} bookmark(s)", bookmarks.len());
                let _ = event_tx
                    .send(XmppEvent::BookmarksReceived(bookmarks))
                    .await;
            }
            return;
        }
    }

    // C4: block/unblock push IQs from the server (type="set")
    if el.attr("type") == Some("set") {
        let first_child_name = el.children().next().map(Element::name);
        match first_child_name {
            Some("block") => {
                blocking_mgr.on_block_push(&el);
                tracing::debug!("blocking: block push received");
                return;
            }
            Some("unblock") => {
                blocking_mgr.on_unblock_push(&el);
                tracing::debug!("blocking: unblock push received");
                return;
            }
            _ => {}
        }
    }

    let iq = match Iq::try_from(el) {
        Ok(i) => i,
        Err(_) => return,
    };

    if let xmpp_parsers::iq::IqType::Result(Some(payload)) = iq.payload {
        if let Ok(roster) = Roster::try_from(payload) {
            let contacts = roster
                .items
                .into_iter()
                .map(|item| RosterContact {
                    jid: item.jid.to_string(),
                    name: item.name,
                    subscription: format!("{:?}", item.subscription),
                })
                .collect();
            let _ = event_tx.send(XmppEvent::RosterReceived(contacts)).await;
        }
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
// Message handler (P1.5)
// ---------------------------------------------------------------------------

const NS_REACTIONS: &str = "urn:xmpp:reactions:0";

async fn handle_message(
    el: Element,
    event_tx: &mpsc::Sender<XmppEvent>,
    blocking_mgr: &BlockingManager,
) {
    // E3: detect XEP-0444 reaction stanza before consuming el
    if let Some(reactions_el) = el
        .children()
        .find(|c| c.name() == "reactions" && c.ns() == NS_REACTIONS)
    {
        if let Some(msg_id) = reactions_el.attr("id") {
            let from = el.attr("from").unwrap_or("").to_string();
            let bare_from = from.split('/').next().unwrap_or(&from).to_string();
            if !blocking_mgr.is_blocked(&bare_from) {
                let emojis: Vec<String> = reactions_el
                    .children()
                    .filter(|c| c.name() == "reaction" && c.ns() == NS_REACTIONS)
                    .map(|c| c.text())
                    .collect();
                let _ = event_tx
                    .send(XmppEvent::ReactionReceived {
                        msg_id: msg_id.to_string(),
                        from: bare_from,
                        emojis,
                    })
                    .await;
            }
        }
        return;
    }

    // G2: detect XEP-0085 chat state notifications from the raw element
    // before consuming el into XmppMessage (which may drop unknown children)
    let has_composing = el
        .children()
        .any(|c| c.name() == "composing" && c.ns() == "jabber:x:chatstates");
    let has_paused = el.children().any(|c| {
        (c.name() == "paused" || c.name() == "inactive") && c.ns() == "jabber:x:chatstates"
    });
    let chat_state_from = el.attr("from").map(str::to_string);

    let msg = match XmppMessage::try_from(el) {
        Ok(m) => m,
        Err(_) => return,
    };

    // Only handle chat/normal messages with a body.
    if msg.type_ == MessageType::Error {
        return;
    }

    // G2: emit PeerTyping if we found a chat state
    if has_composing || has_paused {
        if let Some(from_str) = chat_state_from.as_deref() {
            let bare_jid = from_str.split('/').next().unwrap_or(from_str).to_string();
            let _ = event_tx
                .send(XmppEvent::PeerTyping {
                    jid: bare_jid,
                    composing: has_composing,
                })
                .await;
        }
    }

    let body = match msg.bodies.get("") {
        Some(Body(b)) => b.clone(),
        None => return,
    };

    let from = match msg.from {
        Some(ref f) => f.to_string(),
        None => return,
    };

    // C4: skip messages from blocked JIDs
    let bare_from = from.split('/').next().unwrap_or(&from);
    if blocking_mgr.is_blocked(bare_from) {
        tracing::debug!("blocking: dropped message from {bare_from}");
        return;
    }

    let id = msg.id.unwrap_or_default();

    let _ = event_tx
        .send(XmppEvent::MessageReceived(IncomingMessage {
            id,
            from,
            body,
            is_historical: false,
        }))
        .await;
}

// ---------------------------------------------------------------------------
// Presence handler (P1.4)
// ---------------------------------------------------------------------------

async fn handle_presence(el: Element, event_tx: &mpsc::Sender<XmppEvent>) {
    let presence = match Presence::try_from(el) {
        Ok(p) => p,
        Err(_) => return,
    };

    let jid = match presence.from {
        Some(ref f) => f.to_string(),
        None => return,
    };

    let available = !matches!(
        presence.type_,
        PresenceType::Unavailable | PresenceType::Error
    );

    let _ = event_tx
        .send(XmppEvent::PresenceUpdated { jid, available })
        .await;
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

fn make_presence() -> Element {
    Presence::new(PresenceType::None).into()
}

fn make_message(to: Jid, body: &str) -> Element {
    let mut msg = XmppMessage::new(Some(to));
    msg.type_ = MessageType::Chat;
    msg.id = Some(uuid::Uuid::new_v4().to_string());
    msg.bodies.insert(String::new(), Body(body.to_owned()));
    msg.into()
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

/// E1: Build a XEP-0308 message correction stanza.
fn make_correction_message(to: Jid, original_id: &str, new_body: &str) -> Element {
    let replace_el = Element::builder("replace", "urn:xmpp:message-correct:0")
        .attr("id", original_id)
        .build();
    let body_el = Element::builder("body", "jabber:client")
        .append(new_body)
        .build();
    let mut raw = Element::builder("message", "jabber:client")
        .attr("type", "chat")
        .attr("to", to.to_string())
        .attr("id", uuid::Uuid::new_v4().to_string())
        .build();
    raw.append_child(body_el);
    raw.append_child(replace_el);
    raw
}

/// E2: Build a XEP-0424 message retraction stanza.
fn make_retraction_message(to: Jid, origin_id: &str) -> Element {
    let apply_to_el = Element::builder("apply-to", "urn:xmpp:fasten:0")
        .attr("id", origin_id)
        .append(Element::builder("retract", "urn:xmpp:message-retract:1").build())
        .build();
    let mut raw = Element::builder("message", "jabber:client")
        .attr("type", "chat")
        .attr("to", to.to_string())
        .attr("id", uuid::Uuid::new_v4().to_string())
        .build();
    raw.append_child(apply_to_el);
    raw
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmpp::connection::sasl::SaslMechanism;

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
        run_engine(event_tx, cmd_rx).await;
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
        };
        assert_eq!(cfg.jid, "user@example.com");
        assert_eq!(cfg.server, "xmpp.example.com:5222");
        assert_eq!(cfg.status_message, Some("In a meeting".into()));
    }
}
