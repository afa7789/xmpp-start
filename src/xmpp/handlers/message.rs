// Message stanza handler (P1.5)
// Extracted from engine.rs to keep file size manageable.

use std::collections::VecDeque;

use tokio::sync::mpsc;
use tokio_xmpp::minidom::Element;
use tokio_xmpp::parsers::message::{Body, Message as XmppMessage, MessageType};

use crate::xmpp::{
    modules::blocking::BlockingManager,
    modules::ignore::IgnoreManager,
    modules::message_mutations::MutationManager,
    modules::{NS_MUC_USER, NS_X_CONFERENCE},
    IncomingMessage, XmppEvent,
};

use super::{NS_CHAT_MARKERS, NS_RECEIPTS};

pub(crate) async fn handle_message(
    el: Element,
    event_tx: &mpsc::Sender<XmppEvent>,
    blocking_mgr: &BlockingManager,
    ignore_mgr: &IgnoreManager,
    outbox: &mut VecDeque<Element>,
    privacy_flags: u8,
) {
    // DC-5: stateless manager for XEP-0308/0424/0444 parsing
    let mutation_mgr = MutationManager::new();

    // K3: XEP-0249 direct invitation
    if let Some(x_el) = el
        .children()
        .find(|c| c.name() == "x" && c.ns() == NS_X_CONFERENCE)
    {
        if let Some(room_jid) = x_el.attr("jid") {
            let from_jid = el.attr("from").unwrap_or("").to_string();
            let bare_from = from_jid.split('/').next().unwrap_or(&from_jid).to_string();
            let reason = x_el
                .children()
                .find(|c| c.name() == "reason")
                .map(tokio_xmpp::minidom::Element::text);
            let _ = event_tx
                .send(XmppEvent::RoomInvitationReceived {
                    room_jid: room_jid.to_string(),
                    from_jid: bare_from,
                    reason,
                })
                .await;
        }
        return;
    }

    // K3: XEP-0045 §7.8 mediated invitation
    if let Some(x_muc) = el
        .children()
        .find(|c| c.name() == "x" && c.ns() == NS_MUC_USER)
    {
        if let Some(invite_el) = x_muc.children().find(|c| c.name() == "invite") {
            let from_jid = invite_el.attr("from").unwrap_or("").to_string();
            let room_jid_full = el.attr("from").unwrap_or("").to_string();
            let bare_room = room_jid_full
                .split('/')
                .next()
                .unwrap_or(&room_jid_full)
                .to_string();
            let reason = invite_el
                .children()
                .find(|c| c.name() == "reason")
                .map(tokio_xmpp::minidom::Element::text);
            let _ = event_tx
                .send(XmppEvent::RoomInvitationReceived {
                    room_jid: bare_room,
                    from_jid,
                    reason,
                })
                .await;
        }
        return;
    }

    // E3: detect XEP-0444 reaction stanza before consuming el
    {
        let from = el.attr("from").unwrap_or("").to_string();
        let bare_from = from.split('/').next().unwrap_or(&from).to_string();
        if let Some(update) = mutation_mgr.parse_reaction(&bare_from, &el) {
            if !blocking_mgr.is_blocked(&bare_from) {
                let _ = event_tx
                    .send(XmppEvent::ReactionReceived {
                        msg_id: update.target_id,
                        from: update.from_jid,
                        emojis: update.emojis,
                    })
                    .await;
            }
            return;
        }
    }

    // E1: detect XEP-0308 last message correction before consuming el
    {
        let from = el.attr("from").unwrap_or("").to_string();
        let bare_from = from.split('/').next().unwrap_or(&from).to_string();
        if let Some(correction) = mutation_mgr.parse_correction(&bare_from, &el) {
            if !blocking_mgr.is_blocked(&bare_from) {
                let _ = event_tx
                    .send(XmppEvent::CorrectionReceived {
                        original_id: correction.target_id,
                        _from_jid: correction.from_jid,
                        new_body: correction.new_body,
                    })
                    .await;
            }
            return;
        }
    }

    // E2: detect XEP-0424 message retraction before consuming el
    {
        let from = el.attr("from").unwrap_or("").to_string();
        let bare_from = from.split('/').next().unwrap_or(&from).to_string();
        if let Some(retraction) = mutation_mgr.parse_retraction(&bare_from, &el) {
            if !blocking_mgr.is_blocked(&bare_from) {
                let _ = event_tx
                    .send(XmppEvent::RetractionReceived {
                        _origin_id: retraction.target_id,
                        _from_jid: retraction.from_jid,
                    })
                    .await;
            }
            return;
        }
    }

    // K4: XEP-0184 delivery receipt — <received xmlns='urn:xmpp:receipts' id='...'/>
    if let Some(received_el) = el
        .children()
        .find(|c| c.name() == "received" && c.ns() == NS_RECEIPTS)
    {
        if let Some(receipt_id) = received_el.attr("id") {
            let from = el.attr("from").unwrap_or("").to_string();
            let bare_from = from.split('/').next().unwrap_or(&from).to_string();
            let _ = event_tx
                .send(XmppEvent::MessageDelivered {
                    id: receipt_id.to_string(),
                    from: bare_from,
                })
                .await;
        }
        return;
    }

    // K5: XEP-0333 displayed marker — <displayed xmlns='urn:xmpp:chat-markers:0' id='...'/>
    if let Some(displayed_el) = el
        .children()
        .find(|c| c.name() == "displayed" && c.ns() == NS_CHAT_MARKERS)
    {
        if let Some(marker_id) = displayed_el.attr("id") {
            let from = el.attr("from").unwrap_or("").to_string();
            let bare_from = from.split('/').next().unwrap_or(&from).to_string();
            let _ = event_tx
                .send(XmppEvent::MessageRead {
                    id: marker_id.to_string(),
                    from: bare_from,
                })
                .await;
        }
        return;
    }

    // K4: if sender is requesting a receipt, remember message id for auto-reply below
    let receipt_request = el
        .children()
        .any(|c| c.name() == "request" && c.ns() == NS_RECEIPTS);
    let msg_from = el.attr("from").map(str::to_string);
    let msg_id_raw = el.attr("id").map(str::to_string);

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

    // DC-10: skip messages from ignored users in MUC rooms
    if let Some(room_jid) = from.split('/').next() {
        if from.contains('/') && ignore_mgr.is_ignored(room_jid, bare_from) {
            tracing::debug!("ignore: dropped message from {bare_from} in {room_jid}");
            return;
        }
    }

    let id = msg.id.unwrap_or_default();

    // K4: auto-reply with <received> if sender requested a delivery receipt
    // S6: respect user's privacy preference for delivery receipts
    if privacy_flags & 0b001 != 0 && receipt_request {
        if let (Some(reply_to), Some(orig_id)) = (msg_from, msg_id_raw) {
            let receipt = Element::builder("message", "jabber:client")
                .attr("to", reply_to)
                .append(
                    Element::builder("received", NS_RECEIPTS)
                        .attr("id", orig_id)
                        .build(),
                )
                .build();
            outbox.push_back(receipt);
        }
    }

    let _ = event_tx
        .send(XmppEvent::MessageReceived(IncomingMessage {
            id,
            from,
            body,
            is_historical: false,
            is_encrypted: false,
            is_trusted: false,
        }))
        .await;
}
