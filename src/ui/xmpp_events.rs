//! XmppEvent handling extracted from `App::update()`.
//!
//! The single `handle()` function receives `&mut App` plus the event and returns
//! `Task<Message>`.  This keeps the giant match in `App::update()` thin while
//! preserving every bit of the original behaviour.

use iced::Task;

use super::toast::ToastKind;
use super::{
    adhoc, chat, config, login::LoginScreen, omemo_trust, vcard_editor, App, ChatScreen, Message,
    Screen,
};
use crate::xmpp::modules::presence_machine::PresenceStatus;
use crate::xmpp::{AccountId, XmppCommand, XmppEvent};

pub(crate) fn handle(app: &mut App, event: XmppEvent) -> Task<Message> {
    match event {
        XmppEvent::Connected { ref bound_jid } => {
            tracing::info!("XMPP: online as {bound_jid}");
            if let Screen::Login(ref login) = app.screen {
                let cfg = login.connect_config();
                if !cfg.password.is_empty() && login.remember_me {
                    if let Err(e) = config::save_password(&cfg.jid, &cfg.password) {
                        tracing::error!("failed to save password to keychain: {e}");
                    }
                }
            }
            // MULTI: register this account in the state manager so the
            // sidebar indicator bar is populated on first connect.
            // Use bare JID (without resource) so reconnects don't
            // create duplicate entries — the server assigns a fresh
            // resource on every bind.
            let bare_jid = bound_jid
                .split('/')
                .next()
                .unwrap_or(bound_jid.as_str())
                .to_string();
            let account_id = AccountId::new(bare_jid);
            app.account_state_mgr.add_account(account_id.clone());

            // DC-21: if this connection was triggered by AddAccount,
            // register the new account in the multi-engine manager
            // so future commands can be routed to it.
            if app.is_adding_account {
                app.is_adding_account = false;
                let cfg = config::AccountConfig::new(bound_jid.clone());
                app.multi_engine
                    .start_account(cfg, app.multi_event_tx.clone());
                app.multi_engine.switch_active(account_id.clone());
            }
            let mut chat_screen = ChatScreen::new(bound_jid.clone());
            // Pass the active account info into the chat screen so
            // view_with_drafts() can render the indicator bar.
            let unread = app
                .account_state_mgr
                .get_active()
                .map_or(0, |s| s.unread_total);
            // H2: restore cached own avatar so it displays immediately
            // before the server fetch completes.
            if let Some(ref avatar_bytes) = app.settings.avatar_data {
                app.avatar_cache
                    .insert(account_id.0.clone(), avatar_bytes.clone());
            }
            chat_screen.set_active_account(Some(account_id), unread);
            app.screen = Screen::Chat(Box::new(chat_screen));
            // A3: pre-populate sidebar from cached DB roster before server responds
            let pool = app.db.clone();
            let roster_prefill = Task::future(async move {
                let contacts = crate::store::roster_repo::get_all(&pool)
                    .await
                    .unwrap_or_default();
                let xmpp_contacts: Vec<crate::xmpp::RosterContact> = contacts
                    .into_iter()
                    .map(|c| crate::xmpp::RosterContact {
                        jid: c.jid,
                        name: c.name,
                        subscription: c.subscription,
                    })
                    .collect();
                Message::XmppEvent(XmppEvent::RosterReceived(xmpp_contacts))
            });
            // Also pre-populate known conversations from DB cache.
            let pool2 = app.db.clone();
            let conv_prefill = Task::future(async move {
                let convos: Vec<(String, bool)> = crate::store::conversation_repo::get_all(&pool2)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| c.archived == 0) // Skip archived conversations
                    .map(|c| (c.jid, c.encrypted != 0))
                    .collect();
                Message::ConversationsPrefill(convos)
            });
            let toast = app.update(Message::ShowToast(
                format!("Connected as {}", bound_jid),
                ToastKind::Success,
            ));
            return Task::batch([roster_prefill, conv_prefill, toast]);
        }
        XmppEvent::RegistrationFormReceived { .. } => {
            // For now, just show a toast. In a full impl, we'd show the Data Form.
            return app.update(Message::ShowToast(
                "Registration form received (XEP-0077)".into(),
                ToastKind::Info,
            ));
        }
        XmppEvent::RegistrationSuccess => {
            return app.update(Message::ShowToast(
                "Account registered successfully!".into(),
                ToastKind::Success,
            ));
        }
        XmppEvent::RegistrationFailure(reason) => {
            if let Screen::Login(ref mut login) = app.screen {
                login.on_error(reason.clone());
            }
            return app.update(Message::ShowToast(
                format!("Registration failed: {}", reason),
                ToastKind::Error,
            ));
        }
        XmppEvent::Disconnected { ref reason } => {
            tracing::warn!("XMPP: disconnected — {reason}");
            // BUG-7: auth failure while on Login screen — clear stale cfg so
            // the Reconnecting handler won't attempt a retry with bad creds.
            if matches!(app.screen, Screen::Login(_)) {
                app.last_connect_cfg = None;
            }
            if let Screen::Login(ref mut login) = app.screen {
                login.on_error(reason.clone());
            }
            if matches!(app.screen, Screen::Chat(_)) {
                let pw = if app.settings.remember_me {
                    config::load_password(&app.settings.last_jid).unwrap_or_default()
                } else {
                    String::new()
                };
                app.screen = Screen::Login(LoginScreen::with_saved(
                    app.settings.last_jid.clone(),
                    pw,
                    app.settings.last_server.clone(),
                    app.settings.remember_me,
                ));
            }
            // J1: show disconnect toast
            let msg = Message::ShowToast(format!("Disconnected: {}", reason), ToastKind::Error);
            return app.update(msg);
        }
        XmppEvent::Reconnecting { attempt } => {
            tracing::info!("XMPP: reconnecting (attempt {attempt})");
            app.reconnect_attempt = attempt;
            // BUG-7: if still on Login screen the user hasn't successfully
            // authenticated yet — don't auto-reconnect with stale credentials.
            if matches!(app.screen, Screen::Login(_)) {
                tracing::info!("XMPP: on Login screen, skipping auto-reconnect");
            } else {
                let delay_secs = 2u64.pow(attempt.min(6));
                if let (Some(cfg), Some(tx)) = (app.last_connect_cfg.clone(), app.xmpp_tx.clone()) {
                    return Task::future(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                        let _ = tx.send(XmppCommand::Connect(cfg)).await;
                        Message::XmppEvent(XmppEvent::Reconnecting { attempt: 0 })
                    })
                    .discard();
                }
            }
        }
        XmppEvent::RosterReceived(ref contacts) => {
            tracing::info!("XMPP: roster ({} contacts)", contacts.len());
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.set_roster(contacts.clone());
            }
            let toast = app.update(Message::ShowToast(
                format!("{} contacts loaded", contacts.len()),
                ToastKind::Info,
            ));
            // A3: persist roster to DB
            let pool = app.db.clone();
            let contacts = contacts.clone();
            // H1: fetch avatars for all roster contacts (fire-and-forget)
            // Skip JIDs already cached or already being fetched to avoid duplicates.
            if let Some(ref tx) = app.xmpp_tx {
                let tx = tx.clone();
                let jids: Vec<String> = contacts
                    .iter()
                    .map(|c| c.jid.clone())
                    .filter(|jid| {
                        !app.avatar_cache.contains_key(jid.as_str())
                            && app.avatar_fetching.insert(jid.clone())
                    })
                    .collect();
                tokio::spawn(async move {
                    for jid in jids {
                        let _ = tx.send(XmppCommand::FetchAvatar(jid)).await;
                    }
                });
            }
            // A3: persist roster to DB (fire-and-forget)
            tokio::spawn(async move {
                for c in &contacts {
                    let _ = crate::store::roster_repo::upsert(
                        &pool,
                        &crate::store::roster_repo::RosterContact {
                            jid: c.jid.clone(),
                            name: c.name.clone(),
                            subscription: c.subscription.clone(),
                            groups: None,
                        },
                    )
                    .await;
                }
            });
            return toast;
        }
        XmppEvent::MessageReceived(ref msg) => {
            tracing::info!("XMPP: message from {}", msg.from);
            let bare_from = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
            // A5: desktop notification — only for background conversations (J3: skip muted JIDs)
            let is_active = if let Screen::Chat(ref chat) = app.screen {
                chat.active_jid() == Some(bare_from.as_str())
            } else {
                false
            };
            // A5: fire desktop notification for background conversations (J3: skip muted, BUG-1: skip historical, O2: skip if DND)
            let notif_task: Task<Message> = if app.settings.notifications_enabled
                && !is_active
                && !msg.is_historical
                && !app.settings.muted_jids.contains(&bare_from)
                && app.own_presence != PresenceStatus::DoNotDisturb
            {
                let notif_from = bare_from.clone();
                let notif_body: String = msg.body.chars().take(100).collect();
                tokio::spawn(async move {
                    let _ = crate::notifications::notify_message(&notif_from, &notif_body);
                });
                Task::none()
            } else {
                Task::none()
            };
            // A2: persist message + conversation to DB (fire-and-forget)
            let pool = app.db.clone();
            let from_jid = msg.from.clone();
            let bare_jid = from_jid.split('/').next().unwrap_or(&from_jid).to_string();
            let msg_id = msg.id.clone();
            let body = msg.body.clone();
            let ts = chrono::Utc::now().timestamp_millis();
            tokio::spawn(async move {
                let _ = crate::store::conversation_repo::upsert(&pool, &bare_jid).await;
                let _ = crate::store::conversation_repo::update_last_activity(&pool, &bare_jid, ts)
                    .await;
                let _ = crate::store::message_repo::insert(
                    &pool,
                    &crate::store::message_repo::Message {
                        id: msg_id,
                        conversation_jid: bare_jid,
                        from_jid,
                        body: Some(body),
                        timestamp: ts,
                        stanza_id: None,
                        origin_id: None,
                        state: "received".into(),
                        edited_body: None,
                        retracted: 0,
                    },
                )
                .await;
            });

            if let Screen::Chat(ref mut chat) = app.screen {
                // MULTI: increment unread count for background conversations.
                if !is_active && !msg.is_historical {
                    if let Some(state) = app.account_state_mgr.get_active_mut() {
                        state.unread_total += 1;
                    }
                    // Sync updated unread total back into the chat screen.
                    let unread = app
                        .account_state_mgr
                        .get_active()
                        .map_or(0, |s| s.unread_total);
                    let active_id = app.account_state_mgr.active_id().cloned();
                    chat.set_active_account(active_id, unread);
                }
                if let Some(preview_task) = chat.on_message_received(msg.clone()) {
                    return Task::batch([notif_task, preview_task.map(Message::Chat)]);
                }
                return notif_task;
            }
            return notif_task;
        }
        XmppEvent::PresenceUpdated { ref jid, available } => {
            tracing::debug!("XMPP: presence {jid} available={available}");
            // A4: forward to sidebar
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.on_presence(jid, available);
            }
            // F5: fetch avatar for newly-available contacts not yet cached
            if available
                && !app.avatar_cache.contains_key(jid.as_str())
                && app.avatar_fetching.insert(jid.clone())
            {
                if let Some(ref tx) = app.xmpp_tx {
                    let tx = tx.clone();
                    let jid_owned = jid.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(XmppCommand::FetchAvatar(jid_owned)).await;
                    });
                }
            }
        }
        XmppEvent::PeerTyping { ref jid, composing } => {
            if let Screen::Chat(ref mut chat) = app.screen {
                let action = chat.update(chat::Message::PeerTyping(jid.clone(), composing));
                return app.handle_chat_action(action);
            }
        }
        XmppEvent::AvatarReceived {
            ref jid,
            ref png_bytes,
        } => {
            tracing::debug!("H1: avatar received for {jid} ({} bytes)", png_bytes.len());
            app.avatar_cache.insert(jid.clone(), png_bytes.clone());
            config::save_avatar(jid, png_bytes);
            if let Screen::Chat(ref mut chat) = app.screen {
                // H2: persist own avatar to settings for instant
                // restore on next login.
                let own_bare = chat.own_jid().split('/').next().unwrap_or(chat.own_jid());
                if jid == own_bare {
                    app.settings.avatar_data = Some(png_bytes.clone());
                    let _ = config::save(&app.settings);
                }
            }
        }
        XmppEvent::CatchupFinished {
            ref conversation_jid,
            fetched,
        } => {
            tracing::info!(
                "XMPP: MAM catchup complete for {conversation_jid} ({fetched} messages)"
            );
        }
        XmppEvent::UploadSlotReceived {
            put_url,
            get_url,
            headers,
        } => {
            tracing::info!("E4: upload slot received put={put_url} get={get_url}");
            // E4: perform HTTP PUT and send get_url as message (fire-and-forget)
            if let Some((target_jid, file_path)) = app.pending_upload.take() {
                if let Some(ref tx) = app.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let file_bytes = tokio::fs::read(&file_path).await;
                        match file_bytes {
                            Ok(bytes) => {
                                let client = reqwest::Client::new();
                                let mut req = client.put(&put_url).body(bytes);
                                for (k, v) in &headers {
                                    req = req.header(k.as_str(), v.as_str());
                                }
                                match req.send().await {
                                    Ok(resp) if resp.status().is_success() => {
                                        let _ = tx
                                            .send(XmppCommand::SendMessage {
                                                to: target_jid,
                                                body: get_url,
                                                id: uuid::Uuid::new_v4().to_string(),
                                            })
                                            .await;
                                    }
                                    Ok(resp) => {
                                        tracing::warn!("E4: PUT failed: {}", resp.status());
                                    }
                                    Err(e) => {
                                        tracing::warn!("E4: PUT error: {e}");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("E4: failed to read file {:?}: {e}", file_path);
                            }
                        }
                    });
                }
            }
        }
        XmppEvent::ConsoleEntry { direction, xml } => {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            if direction == "sent" {
                app.xmpp_console.push_sent(&xml, ts);
            } else {
                app.xmpp_console.push_received(&xml, ts);
            }
        }
        XmppEvent::ReactionReceived {
            ref msg_id,
            ref from,
            ref emojis,
        } => {
            tracing::debug!("E3: reaction from {from} on {msg_id}: {:?}", emojis);
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.on_reaction_received(msg_id.clone(), from.clone(), emojis.clone());
            }
        }
        XmppEvent::VCardReceived { jid, name, .. } => {
            tracing::debug!("H4: vCard received for {jid}: name={:?}", name);
        }
        // J6: XEP-0084 PubSub avatar — store alongside vCard avatar
        XmppEvent::AvatarUpdated { ref jid, ref data } => {
            tracing::debug!("J6: PubSub avatar updated for {jid} ({} bytes)", data.len());
            app.avatar_cache.insert(jid.clone(), data.clone());
            config::save_avatar(jid, data);
            if let Screen::Chat(ref mut chat) = app.screen {
                // H2: persist own avatar to settings for instant
                // restore on next login.
                let own_bare = chat.own_jid().split('/').next().unwrap_or(chat.own_jid());
                if jid == own_bare {
                    app.settings.avatar_data = Some(data.clone());
                    let _ = config::save(&app.settings);
                }
            }
        }
        // K4: delivery receipt — update message state in conversation
        XmppEvent::MessageDelivered { ref id, ref from } => {
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.on_message_delivered(from, id.clone());
            }
        }
        // K5: read marker — update message state in conversation
        XmppEvent::MessageRead { ref id, ref from } => {
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.on_message_read(from, id.clone());
            }
        }
        // J10: MAM prefs received — persist to settings and update UI state
        XmppEvent::MamPrefsReceived { ref default_mode } => {
            tracing::debug!("J10: MAM prefs default_mode={default_mode}");
            app.mam_default_mode = Some(default_mode.clone());
            app.settings.mam_default_mode = Some(default_mode.clone());
            let _ = config::save(&app.settings);
        }
        // K1: room config form received from server
        XmppEvent::RoomConfigFormReceived { room_jid, config } => {
            if let Screen::Chat(ref mut chat) = app.screen {
                let action = chat.update(chat::Message::RoomConfigFormReceived(room_jid, config));
                return app.handle_chat_action(action);
            }
        }
        // K1: room configuration accepted — room is now live
        XmppEvent::RoomConfigured { room_jid } => {
            if let Screen::Chat(ref mut chat) = app.screen {
                let action = chat.update(chat::Message::RoomConfigured(room_jid));
                return app.handle_chat_action(action);
            }
        }
        // K3: incoming room invitation received
        XmppEvent::RoomInvitationReceived {
            room_jid,
            from_jid,
            reason,
        } => {
            let chat_task = if let Screen::Chat(ref mut chat) = app.screen {
                let action = chat.update(chat::Message::RoomInvitationReceived {
                    room_jid: room_jid.clone(),
                    from_jid: from_jid.clone(),
                    reason: reason.clone(),
                });
                app.handle_chat_action(action)
            } else {
                Task::none()
            };
            let body = format!("{} invited you to {}", from_jid, room_jid);
            let toast_task = app.update(Message::ShowToast(body, ToastKind::Info));
            return Task::batch([chat_task, toast_task]);
        }
        // K2: room list received from MUC service
        XmppEvent::RoomListReceived(rooms) => {
            tracing::info!("k2: {} public rooms received", rooms.len());
            if let Screen::Chat(ref mut chat) = app.screen {
                let action = chat.update(chat::Message::RoomListReceived(rooms));
                return app.handle_chat_action(action);
            }
        }
        XmppEvent::BookmarksReceived(bookmarks) => {
            tracing::info!("D4: {} bookmark(s) received", bookmarks.len());
            // D4: autojoin rooms — send JoinRoom for each autojoin bookmark
            let autojoin: Vec<_> = bookmarks.iter().filter(|b| b.autojoin).collect();
            if !autojoin.is_empty() {
                if let Some(ref tx) = app.xmpp_tx {
                    let tx = tx.clone();
                    let cmds: Vec<XmppCommand> = autojoin
                        .into_iter()
                        .map(|b| XmppCommand::JoinRoom {
                            jid: b.jid.clone(),
                            nick: b.nick.clone().unwrap_or_else(|| {
                                // Default nick: local part of our JID
                                "me".to_string()
                            }),
                        })
                        .collect();
                    tokio::spawn(async move {
                        for cmd in cmds {
                            let _ = tx.send(cmd).await;
                        }
                    });
                }
            }
        }
        // L3: XEP-0425 — message was moderated in a MUC room
        XmppEvent::MessageModerated {
            ref room_jid,
            ref message_id,
        } => {
            tracing::info!("muc: message {} moderated in {}", message_id, room_jid);
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.on_message_moderated(room_jid, message_id);
            }
            // Persist the retraction so the message stays tombstoned after restart.
            let pool = app.db.clone();
            let mid = message_id.clone();
            tokio::spawn(async move {
                let _ = crate::store::message_repo::mark_retracted(&pool, &mid).await;
            });
        }
        // K2: own vCard received — populate the vCard editor if it's open
        XmppEvent::OwnVCardReceived(fields) => {
            if let Screen::VCardEditor(ref mut ve, _) = app.screen {
                let action = ve.update(vcard_editor::Message::VCardLoaded(fields));
                return app.handle_vcard_action(action);
            }
        }
        // K2: own vCard saved — confirm to the editor
        XmppEvent::OwnVCardSaved => {
            if let Screen::VCardEditor(ref mut ve, _) = app.screen {
                let action = ve.update(vcard_editor::Message::VCardSaved);
                return app.handle_vcard_action(action);
            }
        }
        // L4: ad-hoc commands discovered — forward to adhoc screen
        XmppEvent::AdhocCommandsDiscovered { from_jid, commands } => {
            if let Screen::Adhoc(ref mut adhoc, _) = app.screen {
                let action = adhoc.update(adhoc::Message::CommandsDiscovered {
                    _from_jid: from_jid,
                    commands,
                });
                return app.handle_adhoc_action(action);
            }
        }
        // L4: ad-hoc command response — forward to adhoc screen
        XmppEvent::AdhocCommandResult(resp) => {
            if let Screen::Adhoc(ref mut adhoc, _) = app.screen {
                let action = adhoc.update(adhoc::Message::CommandResponseReceived(resp));
                return app.handle_adhoc_action(action);
            }
        }
        // MULTI: account switched — sync indicator bar in the chat screen.
        XmppEvent::AccountSwitched(ref id) => {
            app.account_state_mgr.switch_to(id);
            let unread = app
                .account_state_mgr
                .get_active()
                .map_or(0, |s| s.unread_total);
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.set_active_account(Some(id.clone()), unread);
            }
        }
        // E1: XEP-0308 last message correction — persist the edited body.
        XmppEvent::CorrectionReceived {
            ref original_id,
            new_body: ref body,
            ..
        } => {
            let pool = app.db.clone();
            let oid = original_id.clone();
            let nb = body.clone();
            tokio::spawn(async move {
                let _ = crate::store::message_repo::update_body(&pool, &oid, &nb).await;
            });
        }
        // MEMO: OMEMO successfully enabled — store state and notify UI.
        XmppEvent::OmemoEnabled { device_id } => {
            app.omemo_enabled = true;
            app.omemo_device_id = Some(device_id);
            // Push state into the settings modal if it is currently open.
            if let Some(ref mut ss) = app.settings_modal {
                ss.set_omemo_active(device_id);
            }
            // Also handle legacy full-screen settings path.
            if let Screen::Settings(ref mut ss, _) = app.screen {
                ss.set_omemo_active(device_id);
            }
            // OMEMO Phase 2: propagate global flag to the chat screen so the
            // per-conversation lock toggle becomes visible.
            if let Screen::Chat(ref mut chat) = app.screen {
                chat.omemo_enabled = true;
            }
            return app.update(Message::ShowToast(
                format!("OMEMO enabled (device {device_id})"),
                ToastKind::Info,
            ));
        }
        // MEMO: cache peer device list so the trust dialog can populate itself.
        XmppEvent::OmemoDeviceListReceived { jid, devices } => {
            tracing::debug!("OMEMO: device list for {jid}: {} device(s)", devices.len());
            app.omemo_peer_devices.insert(jid.clone(), devices.clone());
            // If the trust dialog for this JID is open, refresh its device list.
            if let Some(ref mut modal) = app.omemo_trust_modal {
                if modal.contact_jid == jid {
                    let entries: Vec<omemo_trust::DeviceEntry> = devices
                        .into_iter()
                        .map(|id| omemo_trust::DeviceEntry {
                            device_id: id,
                            identity_key: vec![],
                            trust: crate::xmpp::modules::omemo::store::TrustState::Undecided,
                            label: None,
                            active: true,
                        })
                        .collect();
                    modal.devices = entries;
                }
            }
        }
        // MEMO / other agents: unhandled events from additional modules.
        XmppEvent::LocationReceived { .. }
        | XmppEvent::BobReceived(_)
        | XmppEvent::OmemoKeyExchangeNeeded { .. }
        | XmppEvent::StickerPackReceived(_)
        | XmppEvent::IgnoreListReceived { .. }
        | XmppEvent::ConversationsReceived(_)
        | XmppEvent::RetractionReceived { .. }
        | XmppEvent::PasswordChanged { .. }
        | XmppEvent::AccountDeleted { .. }
        | XmppEvent::UploadSlotError { .. } => {}
    }
    Task::none()
}
