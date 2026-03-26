// IQ stanza handler
// Extracted from engine.rs to keep file size manageable.

use std::collections::VecDeque;

use tokio::sync::mpsc;
use tokio_xmpp::minidom::Element;
use tokio_xmpp::parsers::{iq::Iq, roster::Roster};
use vodozemac::Curve25519PublicKey;

use crate::xmpp::modules::omemo::bundle::OmemoManager;
use crate::xmpp::{
    modules::account::{AccountIqKind, AccountManager},
    modules::adhoc::AdhocManager,
    modules::avatar::AvatarManager,
    modules::blocking::BlockingManager,
    modules::bookmarks::BookmarkManager,
    modules::catchup::CatchupManager,
    modules::conversation_sync::ConversationSyncManager,
    modules::disco::DiscoManager,
    modules::entity_time::EntityTimeManager,
    modules::file_upload::FileUploadManager,
    modules::ignore::IgnoreManager,
    modules::mam::MamManager,
    modules::muc_config::MucConfigManager,
    modules::omemo::{
        bundle::build_bundle_publish,
        bundle::{parse_bundle, OmemoBundle},
        device::DeviceManager,
        session::OmemoSessionManager,
        store::{OwnIdentity, TrustState},
        NS_OMEMO,
    },
    modules::sync::SyncOrchestrator,
    modules::vcard_edit::VCardEditManager,
    modules::{bob, NS_MAM, NS_MUC_OWNER},
    RosterContact, XmppEvent,
};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_iq(
    el: Element,
    event_tx: &mpsc::Sender<XmppEvent>,
    outbox: &mut VecDeque<Element>,
    blocking_mgr: &mut BlockingManager,
    mam_mgr: &mut MamManager,
    catchup_mgr: &mut CatchupManager,
    sync_orch: &mut SyncOrchestrator,
    disco_mgr: &mut DiscoManager,
    file_upload_mgr: &mut FileUploadManager,
    avatar_mgr: &mut AvatarManager,
    muc_config_mgr: &mut MucConfigManager,
    bookmark_mgr: &mut BookmarkManager,
    vcard_edit_mgr: &mut VCardEditManager,
    adhoc_mgr: &mut AdhocManager,
    ignore_mgr: &mut IgnoreManager,
    conv_sync_mgr: &ConversationSyncManager,
    omemo_mgr: &mut Option<OmemoManager>,
    account_mgr: &mut AccountManager,
    account_jid: &str,
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
    // DC-8: respond to entity-time get requests (XEP-0202)
    if el.attr("type") == Some("get") {
        let has_time = el
            .children()
            .any(|c| c.name() == "time" && c.ns() == "urn:xmpp:time");
        if has_time {
            let iq_id = el.attr("id").unwrap_or("").to_string();
            let requester = el.attr("from").unwrap_or("").to_string();
            outbox.push_back(EntityTimeManager::build_time_response(&iq_id, &requester));
            tracing::debug!("entity_time: responded to time get from {requester}");
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
                // P4.4: also notify the bulk sync orchestrator so it tracks completion
                sync_orch.on_fin(&query_id);
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
    // E4: detect upload slot error (e.g. 503 service-unavailable)
    if let Some((_iq_id, reason)) = file_upload_mgr.on_slot_error(&el) {
        tracing::warn!("file_upload: slot request failed: {reason}");
        let _ = event_tx
            .send(XmppEvent::UploadSlotError { reason })
            .await;
        return;
    }
    // J6: detect XEP-0084 avatar data result (PubSub items node='urn:xmpp:avatar:data')
    if el.attr("type") == Some("result") {
        let is_avatar_data = el.children().any(|c| {
            c.name() == "pubsub"
                && c.ns() == "http://jabber.org/protocol/pubsub"
                && c.children().any(|items| {
                    items.name() == "items" && items.attr("node") == Some("urn:xmpp:avatar:data")
                })
        });
        if is_avatar_data {
            let from_jid = el.attr("from").unwrap_or("").to_string();
            if let Some(avatar_info) = avatar_mgr.on_avatar_data_result(&from_jid, &el) {
                if !avatar_info.data.is_empty() {
                    let _ = event_tx
                        .send(XmppEvent::AvatarUpdated {
                            jid: avatar_info.jid,
                            data: avatar_info.data,
                        })
                        .await;
                }
            }
            return;
        }
    }
    // K2: detect own-vCard get result (must check before avatar manager to avoid consuming it)
    if let Some(fields) = vcard_edit_mgr.on_get_result(&el) {
        let _ = event_tx.send(XmppEvent::OwnVCardReceived(fields)).await;
        return;
    }
    // K2: detect own-vCard set result
    if vcard_edit_mgr.on_set_result(&el) {
        let _ = event_tx.send(XmppEvent::OwnVCardSaved).await;
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
    // L4: detect ad-hoc command result
    if let Some(cmd_response) = adhoc_mgr.on_result(&el) {
        let _ = event_tx
            .send(XmppEvent::AdhocCommandResult(cmd_response))
            .await;
        return;
    }
    // C5: parse disco#info results into cache
    if disco_mgr.on_info_result(&el).is_some() {
        return;
    }
    // K2/L4: parse disco#items results (room list or adhoc command list)
    if let Some((service_jid, items)) = disco_mgr.on_items_result(&el) {
        let count = items.len();
        // L4: if any item has a node resembling commands, treat as adhoc discovery result
        let has_adhoc_nodes = items.iter().any(|i| i.node.is_some());
        if has_adhoc_nodes {
            let commands: Vec<(String, String)> = items
                .into_iter()
                .filter_map(|i| i.node.map(|node| (node, i.name.unwrap_or_default())))
                .collect();
            let _ = event_tx
                .send(XmppEvent::AdhocCommandsDiscovered {
                    from_jid: service_jid.clone(),
                    commands,
                })
                .await;
            tracing::info!("l4: received {} adhoc commands from {}", count, service_jid);
        } else {
            let _ = event_tx.send(XmppEvent::RoomListReceived(items)).await;
            tracing::info!("k2: received {} rooms from {}", count, service_jid);
        }
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

    // J10: detect MAM prefs result
    if el.attr("type") == Some("result") {
        let prefs_default = el.children().find_map(|c| {
            if c.name() == "prefs" && c.ns() == NS_MAM {
                c.attr("default").map(str::to_string)
            } else {
                None
            }
        });
        if let Some(default_mode) = prefs_default {
            let _ = event_tx
                .send(XmppEvent::MamPrefsReceived { default_mode })
                .await;
            return;
        }
    }
    // K1: detect muc#owner config form result
    if el.attr("type") == Some("result") {
        let has_owner_query = el
            .children()
            .any(|c| c.name() == "query" && c.ns() == NS_MUC_OWNER);
        if has_owner_query {
            let room_jid = el.attr("from").unwrap_or("").to_string();
            if let Some(query) = el
                .children()
                .find(|c| c.name() == "query" && c.ns() == NS_MUC_OWNER)
            {
                if let Some(config) = muc_config_mgr.parse_config_form(query) {
                    let _ = event_tx
                        .send(XmppEvent::RoomConfigFormReceived { room_jid, config })
                        .await;
                }
            }
            return;
        }
    }
    // K1: detect muc config submit result
    if el.attr("type") == Some("result") {
        if let Some(id) = el.attr("id") {
            if id.starts_with("muc-config-submit-") {
                let room_jid = el.attr("from").unwrap_or("").to_string();
                let _ = event_tx.send(XmppEvent::RoomConfigured { room_jid }).await;
                return;
            }
        }
    }
    // MEMO: Detect OMEMO device list IQ result (key-exchange path) and trigger
    // bundle fetches for each device that has no Olm session yet.
    if el.attr("type") == Some("result") {
        if let Some(iq_id) = el.attr("id").map(str::to_string) {
            if let Some(ref mut mgr) = omemo_mgr {
                if let Some(peer_jid) = mgr.take_device_list_fetch(&iq_id) {
                    let devices = DeviceManager::parse_device_list(&el);
                    if !devices.is_empty() {
                        if let Err(e) = mgr
                            .store
                            .sync_device_list(account_jid, &peer_jid, &devices)
                            .await
                        {
                            tracing::warn!("omemo: sync_device_list failed for {peer_jid}: {e}");
                        }
                        for &device_id in &devices {
                            let has_session = mgr
                                .store
                                .load_session(account_jid, &peer_jid, device_id)
                                .await
                                .unwrap_or(None)
                                .is_some();
                            if !has_session {
                                let (bundle_iq_id, bundle_iq) =
                                    mgr.device_mgr.build_bundle_fetch(&peer_jid, device_id);
                                mgr.track_bundle_fetch(bundle_iq_id, peer_jid.clone(), device_id);
                                outbox.push_back(bundle_iq);
                                tracing::debug!(
                                    "omemo: fetching bundle for {peer_jid}/{device_id} (key exchange)"
                                );
                            }
                        }
                    } else {
                        tracing::warn!(
                            "omemo: device list IQ result for {peer_jid} had no devices"
                        );
                    }
                    return;
                }
            }
        }
    }
    // MEMO: Detect OMEMO bundle fetch result and create an outbound Olm session.
    if el.attr("type") == Some("result") {
        if let Some(iq_id) = el.attr("id").map(str::to_string) {
            if let Some(ref mut mgr) = omemo_mgr {
                if let Some((peer_jid, device_id)) = mgr.take_bundle_fetch(&iq_id) {
                    if let Some(bundle) = parse_bundle(&el) {
                        match mgr.store.load_own_identity(account_jid).await {
                            Ok(Some(identity)) => {
                                match OmemoSessionManager::unpickle_account(&identity.identity_key)
                                {
                                    Ok(own_account) => {
                                        let ik_result =
                                            Curve25519PublicKey::from_slice(&bundle.identity_key);
                                        let otk_result =
                                            bundle.pre_keys.first().and_then(|(_, k)| {
                                                Curve25519PublicKey::from_slice(k).ok()
                                            });
                                        match (ik_result, otk_result) {
                                            (Ok(their_ik), Some(their_otk)) => {
                                                let session = OmemoSessionManager::create_outbound_session(
                                                    &own_account,
                                                    their_ik,
                                                    their_otk,
                                                );
                                                match OmemoSessionManager::pickle_session(&session) {
                                                    Ok(pickled) => {
                                                        if let Err(e) = mgr.store
                                                            .save_session(account_jid, &peer_jid, device_id, &pickled)
                                                            .await
                                                        {
                                                            tracing::warn!("omemo: save_session failed for {peer_jid}/{device_id}: {e}");
                                                        } else {
                                                            let _ = mgr.store
                                                                .upsert_device(account_jid, &peer_jid, device_id, TrustState::Tofu, None, true)
                                                                .await;
                                                            tracing::info!("omemo: outbound session created for {peer_jid}/{device_id}");
                                                        }
                                                    }
                                                    Err(e) => tracing::warn!("omemo: pickle_session failed for {peer_jid}/{device_id}: {e}"),
                                                }
                                            }
                                            (Err(e), _) => tracing::warn!("omemo: bad identity key in bundle for {peer_jid}/{device_id}: {e}"),
                                            (_, None) => tracing::warn!("omemo: no one-time keys in bundle for {peer_jid}/{device_id}"),
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("omemo: failed to unpickle own account: {e}")
                                    }
                                }
                            }
                            Ok(None) => tracing::warn!(
                                "omemo: own identity not found when processing bundle"
                            ),
                            Err(e) => tracing::warn!("omemo: load_own_identity failed: {e}"),
                        }
                    } else {
                        tracing::warn!(
                            "omemo: failed to parse bundle IQ for {peer_jid}/{device_id}"
                        );
                    }
                    return;
                }
            }
        }
    }
    // Q2: detect Bits of Binary result (XEP-0231)
    if el.attr("type") == Some("result") {
        if let Some(bob_data) = bob::parse_bob_data(&el) {
            let _ = event_tx.send(XmppEvent::BobReceived(bob_data)).await;
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
                let _ = event_tx.send(XmppEvent::BookmarksReceived(bookmarks)).await;
            }
            return;
        }
    }

    // DC-10: detect ignore list result from PubSub (rexisce:ignore:{room_jid})
    if el.attr("type") == Some("result") {
        let ignore_node = el.children().find_map(|c| {
            if c.name() == "pubsub" && c.ns() == "http://jabber.org/protocol/pubsub" {
                c.children().find_map(|items| {
                    if items.name() == "items" {
                        items
                            .attr("node")
                            .and_then(|n| n.strip_prefix("rexisce:ignore:").map(str::to_string))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        if let Some(room_jid) = ignore_node {
            ignore_mgr.parse_result(&room_jid, &el);
            let ignored = ignore_mgr.list(&room_jid);
            let _ = event_tx
                .send(XmppEvent::IgnoreListReceived { room_jid, ignored })
                .await;
            return;
        }
    }

    // DC-10: detect conversation list result from PubSub (rexisce:conversations)
    if el.attr("type") == Some("result") {
        let is_conv_sync = el.children().any(|c| {
            c.name() == "pubsub"
                && c.ns() == "http://jabber.org/protocol/pubsub"
                && c.children().any(|items| {
                    items.name() == "items" && items.attr("node") == Some("rexisce:conversations")
                })
        });
        if is_conv_sync {
            let conversations = conv_sync_mgr.parse_result(&el);
            let _ = event_tx
                .send(XmppEvent::ConversationsReceived(conversations))
                .await;
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

    // DC-9: XEP-0077 account management IQ results (change-password / delete-account)
    if matches!(el.attr("type"), Some("result") | Some("error")) {
        if let Some(result) = account_mgr.on_iq_result(&el) {
            match result.kind {
                AccountIqKind::ChangePassword => {
                    tracing::info!(
                        "account: change-password {}",
                        if result.success {
                            "succeeded"
                        } else {
                            "failed"
                        }
                    );
                    let _ = event_tx
                        .send(XmppEvent::PasswordChanged {
                            success: result.success,
                        })
                        .await;
                }
                AccountIqKind::DeleteAccount => {
                    tracing::info!(
                        "account: delete-account {}",
                        if result.success {
                            "succeeded"
                        } else {
                            "failed"
                        }
                    );
                    let _ = event_tx
                        .send(XmppEvent::AccountDeleted {
                            success: result.success,
                        })
                        .await;
                }
            }
            return;
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
// MEMO: OMEMO encrypt/decrypt helpers (also used by engine.rs dispatch logic)
// ---------------------------------------------------------------------------

/// Error variants for outbound OMEMO encryption.
#[derive(Debug)]
pub(crate) enum OmemoEncryptError {
    /// No trusted devices found for the recipient — need device list + key exchange.
    NoTrustedDevices,
    /// Trusted devices exist but none have an Olm session yet — need bundle fetches.
    NoSessions {
        /// Device IDs that need bundle fetches before a session can be established.
        device_ids: Vec<u32>,
    },
    /// Any other failure.
    Other(anyhow::Error),
}

impl From<anyhow::Error> for OmemoEncryptError {
    fn from(e: anyhow::Error) -> Self {
        OmemoEncryptError::Other(e)
    }
}

use crate::xmpp::modules::omemo::message::{
    build_encrypted_message, EncryptedMessage, MessageHeader, MessageKey,
};

/// Encrypt `body` for all trusted devices of `to` and build the `<message>` stanza.
pub(crate) async fn omemo_encrypt_and_send(
    mgr: &mut OmemoManager,
    account_jid: &str,
    to: &str,
    body: &str,
) -> Result<Element, OmemoEncryptError> {
    // Load own identity so we know our device_id.
    let identity = mgr
        .store
        .load_own_identity(account_jid)
        .await
        .map_err(OmemoEncryptError::Other)?
        .ok_or_else(|| {
            OmemoEncryptError::Other(anyhow::anyhow!("OMEMO identity not initialised"))
        })?;

    let own_device_id = identity.device_id;

    // Load trusted devices for the recipient.
    let peer_devices = mgr
        .store
        .load_devices(account_jid, to)
        .await
        .map_err(OmemoEncryptError::Other)?;

    let trusted: Vec<_> = peer_devices
        .iter()
        .filter(|d| d.trust.is_encryptable())
        .collect();
    if trusted.is_empty() {
        return Err(OmemoEncryptError::NoTrustedDevices);
    }

    // AES-256-GCM encrypt the message body.
    let enc_payload =
        OmemoSessionManager::encrypt_payload(body).map_err(OmemoEncryptError::Other)?;

    // Build the key slots for each trusted device that has an established session.
    let mut key_slots: Vec<MessageKey> = Vec::new();
    // Track trusted devices that have no session yet — need bundle fetches.
    let mut missing_sessions: Vec<u32> = Vec::new();

    for device in &trusted {
        let stored_session = mgr
            .store
            .load_session(account_jid, to, device.device_id)
            .await
            .map_err(OmemoEncryptError::Other)?;

        let ss = match stored_session {
            Some(s) => s,
            None => {
                tracing::debug!(
                    "omemo: no session for {}/{}, queuing bundle fetch",
                    to,
                    device.device_id
                );
                missing_sessions.push(device.device_id);
                continue;
            }
        };

        let mut session = OmemoSessionManager::unpickle_session(&ss.session_data)
            .map_err(OmemoEncryptError::Other)?;
        let olm_msg = OmemoSessionManager::encrypt(&mut session, &enc_payload.key);

        // Persist updated session state.
        let pickled =
            OmemoSessionManager::pickle_session(&session).map_err(OmemoEncryptError::Other)?;
        mgr.store
            .save_session(account_jid, to, device.device_id, &pickled)
            .await
            .map_err(OmemoEncryptError::Other)?;

        use vodozemac::olm::OlmMessage;
        let (prekey, ciphertext) = match olm_msg {
            OlmMessage::PreKey(ref pk) => (true, pk.to_bytes()),
            OlmMessage::Normal(ref nm) => (false, nm.to_bytes()),
        };

        key_slots.push(MessageKey {
            rid: device.device_id,
            prekey,
            data: ciphertext,
        });
    }

    if key_slots.is_empty() {
        if !missing_sessions.is_empty() {
            return Err(OmemoEncryptError::NoSessions {
                device_ids: missing_sessions,
            });
        }
        return Err(OmemoEncryptError::NoTrustedDevices);
    }

    let header = MessageHeader {
        sid: own_device_id,
        keys: key_slots,
        iv: enc_payload.nonce,
    };

    let encrypted_msg = EncryptedMessage {
        header,
        payload: Some(enc_payload.ciphertext),
    };

    Ok(build_encrypted_message(to, own_device_id, &encrypted_msg))
}

/// Attempt to decrypt an incoming OMEMO `<message>` stanza.
pub(crate) async fn omemo_try_decrypt(
    mgr: &mut OmemoManager,
    account_jid: &str,
    el: &Element,
) -> anyhow::Result<Option<String>> {
    use vodozemac::olm::OlmMessage;

    use crate::xmpp::modules::omemo::message::parse_encrypted_message;

    let encrypted = parse_encrypted_message(el)
        .ok_or_else(|| anyhow::anyhow!("failed to parse <encrypted> element"))?;

    // Load own identity to find our device_id.
    let identity = mgr
        .store
        .load_own_identity(account_jid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("OMEMO identity not initialised"))?;

    let own_device_id = identity.device_id;

    // Find the key slot addressed to our device.
    let our_key = match encrypted
        .header
        .keys
        .iter()
        .find(|k| k.rid == own_device_id)
    {
        Some(k) => k.clone(),
        None => {
            return Err(anyhow::anyhow!(
                "no key slot for our device_id={own_device_id}"
            ));
        }
    };

    let sender_device_id = encrypted.header.sid;
    let from_jid = el
        .attr("from")
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();

    // Reconstruct the AES key via Olm.
    let aes_key: Vec<u8> = if our_key.prekey {
        // PreKey message — create an inbound session from the X3DH material.
        let account_bytes = &identity.identity_key;
        let mut own_account = OmemoSessionManager::unpickle_account(account_bytes)?;

        use vodozemac::olm::PreKeyMessage;
        let prekey_msg = PreKeyMessage::from_bytes(&our_key.data)
            .map_err(|e| anyhow::anyhow!("PreKeyMessage::from_bytes failed: {e}"))?;

        let sender_ik = prekey_msg.identity_key();

        let result =
            OmemoSessionManager::create_inbound_session(&mut own_account, sender_ik, &prekey_msg)?;

        // Persist the new inbound session.
        let pickled_session = OmemoSessionManager::pickle_session(&result.session)?;
        mgr.store
            .save_session(account_jid, &from_jid, sender_device_id, &pickled_session)
            .await?;

        // Persist the updated own account (one-time key consumed).
        let pickled_account = OmemoSessionManager::pickle_account(&own_account)?;
        let updated_identity = OwnIdentity {
            account_jid: account_jid.to_owned(),
            device_id: identity.device_id,
            identity_key: pickled_account,
            signed_prekey: identity.signed_prekey.clone(),
            spk_id: identity.spk_id,
        };
        mgr.store.save_own_identity(&updated_identity).await?;

        result.plaintext
    } else {
        // Normal message — use existing session.
        let stored_session = mgr
            .store
            .load_session(account_jid, &from_jid, sender_device_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no session for {from_jid}/{sender_device_id} and message is not PreKey"
                )
            })?;

        let mut session = OmemoSessionManager::unpickle_session(&stored_session.session_data)?;
        use vodozemac::olm::Message as NormalMessage;
        let normal_msg = NormalMessage::from_bytes(&our_key.data)
            .map_err(|e| anyhow::anyhow!("NormalMessage::from_bytes failed: {e}"))?;
        let key = OmemoSessionManager::decrypt(&mut session, &OlmMessage::Normal(normal_msg))?;

        // Persist updated session state.
        let pickled = OmemoSessionManager::pickle_session(&session)?;
        mgr.store
            .save_session(account_jid, &from_jid, sender_device_id, &pickled)
            .await?;
        key
    };

    // Decrypt the AES-256-GCM payload.
    match encrypted.payload {
        None => Ok(None), // key-transport, no body
        Some(ref ciphertext) => {
            let body =
                OmemoSessionManager::decrypt_payload(&aes_key, &encrypted.header.iv, ciphertext)?;
            Ok(Some(body))
        }
    }
}

// ---------------------------------------------------------------------------
// MEMO: Pre-key rotation helper
// ---------------------------------------------------------------------------

/// Minimum number of unconsumed one-time pre-keys to maintain.
const OMEMO_PREKEY_LOW_WATERMARK: u32 = 20;
/// Number of pre-keys to generate when replenishing.
const OMEMO_PREKEY_BATCH_SIZE: usize = 50;

/// Check the current one-time pre-key stock. If below `OMEMO_PREKEY_LOW_WATERMARK`,
/// generate a fresh batch and push a new bundle publish IQ into `outbox`.
pub(crate) async fn omemo_check_prekey_rotation(
    mgr: &mut OmemoManager,
    account_jid: &str,
    outbox: &mut VecDeque<Element>,
) {
    let count = match mgr.store.count_unconsumed_prekeys(account_jid).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!("omemo: count_unconsumed_prekeys failed: {e}");
            return;
        }
    };
    if count >= OMEMO_PREKEY_LOW_WATERMARK {
        return;
    }

    tracing::info!("omemo: pre-key stock low ({count}), replenishing");

    let identity = match mgr.store.load_own_identity(account_jid).await {
        Ok(Some(id)) => id,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!("omemo: load_own_identity failed in prekey rotation: {e}");
            return;
        }
    };

    let mut account = match OmemoSessionManager::unpickle_account(&identity.identity_key) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("omemo: unpickle_account failed in prekey rotation: {e}");
            return;
        }
    };

    account.generate_one_time_keys(OMEMO_PREKEY_BATCH_SIZE);

    // Derive new prekey IDs using a large offset to avoid collisions.
    let new_otks: Vec<(u32, Vec<u8>)> = {
        let offset = (identity.device_id as u64 * 1_000_000) as u32;
        account
            .one_time_keys()
            .iter()
            .enumerate()
            .map(|(i, (_kid, pk))| (offset.wrapping_add(i as u32 + 1), pk.to_bytes().to_vec()))
            .collect()
    };

    account.mark_keys_as_published();

    // Persist updated account (keys marked published).
    let pickled = match OmemoSessionManager::pickle_account(&account) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("omemo: pickle_account failed in prekey rotation: {e}");
            return;
        }
    };
    let updated_identity = OwnIdentity {
        account_jid: account_jid.to_owned(),
        device_id: identity.device_id,
        identity_key: pickled,
        signed_prekey: identity.signed_prekey.clone(),
        spk_id: identity.spk_id,
    };
    if let Err(e) = mgr.store.save_own_identity(&updated_identity).await {
        tracing::warn!("omemo: save_own_identity failed in prekey rotation: {e}");
        return;
    }
    if let Err(e) = mgr.store.insert_prekeys(account_jid, &new_otks).await {
        tracing::warn!("omemo: insert_prekeys failed in prekey rotation: {e}");
        return;
    }

    // Publish the updated bundle.
    let bundle = OmemoBundle {
        identity_key: account.identity_keys().curve25519.to_bytes().to_vec(),
        signed_pre_key: identity.signed_prekey.clone(),
        signed_pre_key_id: identity.spk_id,
        signed_pre_key_signature: account.sign(&identity.signed_prekey).to_bytes().to_vec(),
        pre_keys: new_otks,
    };
    outbox.push_back(build_bundle_publish(identity.device_id, &bundle));
    tracing::info!("omemo: published replenished bundle ({OMEMO_PREKEY_BATCH_SIZE} new keys)");
}

/// Check whether `el` carries an OMEMO `<encrypted>` payload.
pub(crate) fn has_omemo_encrypted(el: &Element) -> bool {
    el.get_child("encrypted", NS_OMEMO).is_some()
}
