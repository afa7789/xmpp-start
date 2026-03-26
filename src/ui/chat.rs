// Task P2.3 — ChatScreen: sidebar + conversation view
// This is the main screen shown after a successful XMPP login.

use std::collections::HashMap;

use iced::{
    widget::text::Shaping,
    widget::{button, checkbox, column, container, row, text, text_input},
    Alignment, Element, Length, Task,
};

use crate::xmpp::{
    modules::presence_machine::PresenceStatus, AccountId, IncomingMessage, RosterContact,
    XmppCommand,
};

use super::{
    conversation::{self, ConversationView, DisplayMessage},
    muc_panel::{OccupantEntry, OccupantPanel},
    sidebar::{self, SidebarScreen},
};

/// Actions returned by `ChatScreen::update()` to signal the parent (App).
pub enum Action {
    /// No parent action needed.
    None,
    /// An async task that produces further Messages for this chat screen.
    Task(Task<Message>),
    /// Navigate to the settings screen.
    OpenSettings,
    /// Open the account switcher overlay.
    OpenAccountSwitcher,
    /// Open the OMEMO trust dialog for a peer JID.
    OpenOmemoTrust(String),
    /// User changed their presence status (App needs to track own_presence).
    SetPresence(PresenceStatus),
    /// User toggled mute on a JID (App needs to persist muted_jids).
    ToggleMute(String),
    /// A conversation was selected — App needs to fire history load + mark-read.
    ContactSelected(String),
}

/// K3: An incoming room invitation pending user action.
#[derive(Debug, Clone)]
struct PendingInvitation {
    room_jid: String,
    from_jid: String,
    reason: Option<String>,
}

/// Top-level chat screen state.
pub struct ChatScreen {
    own_jid: String,
    sidebar: SidebarScreen,
    /// Open conversations keyed by bare JID.
    conversations: HashMap<String, ConversationView>,
    /// Currently visible conversation JID.
    active_jid: Option<String>,
    /// Pending commands queued for the engine (drained by App).
    pending_commands: Vec<XmppCommand>,
    /// G2: peers currently typing: JID → instant they last sent composing
    typing_peers: HashMap<String, std::time::Instant>,
    /// E4: pending upload targets (target_jid, file_path) to be consumed by App
    pending_upload_targets: Vec<(String, std::path::PathBuf)>,
    /// D1: occupant panels for MUC rooms (room JID → panel)
    muc_panels: HashMap<String, OccupantPanel>,
    /// D1: set of room JIDs we have joined
    muc_jids: std::collections::HashSet<String>,
    /// D1: shadow occupant lists (room JID → vec) used to update panels
    muc_occupants: HashMap<String, Vec<OccupantEntry>>,
    /// K1: JID of a room waiting for config form submission.
    pending_room_config: Option<(String, crate::xmpp::modules::muc_config::MucRoomConfig)>,
    /// L2: user's own nick per MUC room (room_jid → nick)
    muc_own_nicks: HashMap<String, String>,
    /// K3: incoming invitations pending user action
    pending_invitations: Vec<PendingInvitation>,
    /// K3: invite dialog state: (room_jid, draft invitee JID, draft reason)
    pending_invite_dialog: Option<(String, String, String)>,
    /// K2: public rooms list received from MUC service (for browse dialog)
    public_rooms: Vec<crate::xmpp::modules::disco::DiscoItem>,
    /// MULTI: the currently active account ID — used to populate the sidebar indicator bar.
    active_account_id: Option<AccountId>,
    /// MULTI: aggregate unread count shown on the account indicator badge.
    account_unread: usize,
    /// OMEMO Phase 2: whether OMEMO has been enabled globally (from App state)
    pub omemo_enabled: bool,
    /// Auto-hide timer: set when connected so "Signed in as" hides after 5s.
    connected_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names, dead_code)]
pub enum Message {
    Sidebar(sidebar::Message),
    Conversation(String, super::conversation::Message),
    CloseConversation(String),        // G1: close a conversation by JID
    PeerTyping(String, bool),         // G2: (jid, composing)
    OpenSettings,                     // F3: open settings panel
    OpenAccountSwitcher,              // MULTI: open account switcher panel
    ToggleMute(String),               // J3: toggle mute for a JID
    SetPresence(PresenceStatus),      // C2: user changed their presence status
    MessageDelivered(String, String), // M2: (jid, msg_id) — K4 delivery receipt
    MessageRead(String, String),      // M2: (jid, msg_id) — K5 read marker
    // K1: room config flow
    RoomConfigFormReceived(String, crate::xmpp::modules::muc_config::MucRoomConfig),
    RoomConfigured(String),
    RoomConfigNameChanged(String),
    RoomConfigPublicChanged(bool),
    RoomConfigPersistentChanged(bool),
    SubmitRoomConfig,
    DismissRoomConfig,
    // K3: incoming invitation
    RoomInvitationReceived {
        room_jid: String,
        from_jid: String,
        reason: Option<String>,
    },
    AcceptInvitation(String),  // room_jid
    DeclineInvitation(String), // room_jid
    // K3: outgoing invite dialog
    OpenInviteDialog(String), // room_jid we want to invite someone into
    InviteJidChanged(String),
    InviteReasonChanged(String),
    SubmitInvite,
    DismissInviteDialog,
    // M4: periodic tick forwarded to the active conversation's voice state machine
    VoiceTick,
    // K2: public room list received from MUC service
    RoomListReceived(Vec<crate::xmpp::modules::disco::DiscoItem>),
    // R3: markdown keyboard shortcuts — Ctrl+B (bold) / Ctrl+I (italic).
    // To activate in mod.rs, add to the kb_sub handler:
    //   if modifiers.control() {
    //       if key == Key::Character("b".into()) { return Some(Message::Chat(chat::Message::ComposerBold)); }
    //       if key == Key::Character("i".into()) { return Some(Message::Chat(chat::Message::ComposerItalic)); }
    //   }
    ComposerBold,
    ComposerItalic,
    // OMEMO: open trust management UI for a JID (bubbled up to App)
    OpenOmemoTrust(String),
}

impl ChatScreen {
    pub fn new(own_jid: String) -> Self {
        Self {
            own_jid,
            sidebar: SidebarScreen::new(),
            conversations: HashMap::new(),
            active_jid: None,
            pending_commands: vec![],
            typing_peers: HashMap::new(),
            pending_upload_targets: vec![],
            muc_panels: HashMap::new(),
            muc_jids: std::collections::HashSet::new(),
            muc_occupants: HashMap::new(),
            pending_room_config: None,
            muc_own_nicks: HashMap::new(),
            pending_invitations: Vec::new(),
            pending_invite_dialog: None,
            public_rooms: Vec::new(),
            active_account_id: None,
            account_unread: 0,
            omemo_enabled: false,
            connected_at: Some(std::time::Instant::now()),
        }
    }

    /// MULTI: set the active account ID and aggregate unread count for the
    /// sidebar indicator bar.  Called by App whenever the active account changes
    /// or the unread total is updated.
    pub fn set_active_account(&mut self, id: Option<AccountId>, unread: usize) {
        self.active_account_id = id;
        self.account_unread = unread;
    }

    /// E4: drain pending upload targets (target_jid, file_path) queued by Send with attachments.
    pub fn drain_upload_targets(&mut self) -> Vec<(String, std::path::PathBuf)> {
        std::mem::take(&mut self.pending_upload_targets)
    }

    pub fn set_roster(&mut self, contacts: Vec<RosterContact>) {
        self.sidebar.set_contacts(contacts);
    }

    /// Pre-populate the conversation map from cached DB rows so the sidebar
    /// shows known conversations before any messages arrive from the server.
    pub fn prefill_conversations(&mut self, jids: Vec<String>) {
        let own_jid = self.own_jid.clone();
        for jid in jids {
            self.conversations
                .entry(jid.clone())
                .or_insert_with(|| ConversationView::new(jid, own_jid.clone()));
        }
    }

    /// D1: Register a MUC room join and create an occupant panel.
    pub fn on_join_room(&mut self, room_jid: &str) {
        self.muc_jids.insert(room_jid.to_string());
        self.muc_panels
            .entry(room_jid.to_string())
            .or_insert_with(|| OccupantPanel::new(room_jid.to_string()));
    }

    /// D1: Update the occupant panel for a room from a MUC presence.
    pub fn on_muc_presence(
        &mut self,
        room_jid: &str,
        nick: &str,
        role: &str,
        affiliation: &str,
        available: bool,
    ) {
        // Upsert into shadow occupant list
        let occupants = self.muc_occupants.entry(room_jid.to_string()).or_default();
        if available {
            // Upsert by nick
            if let Some(existing) = occupants.iter_mut().find(|o| o.nick == nick) {
                existing.role = role.to_string();
                existing.affiliation = affiliation.to_string();
                existing.available = true;
            } else {
                occupants.push(OccupantEntry {
                    nick: nick.to_string(),
                    role: role.to_string(),
                    affiliation: affiliation.to_string(),
                    available: true,
                });
            }
        } else {
            occupants.retain(|o| o.nick != nick);
        }

        // Sync shadow list to panel
        let snapshot = occupants.clone();
        let panel = self
            .muc_panels
            .entry(room_jid.to_string())
            .or_insert_with(|| OccupantPanel::new(room_jid.to_string()));
        panel.set_occupants(snapshot);
    }

    /// Route an incoming message to the right conversation bucket.
    /// Returns a Task for any link preview fetches that need to be spawned.
    pub fn on_message_received(&mut self, msg: IncomingMessage) -> Option<Task<Message>> {
        let bare_jid = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
        let own_jid = self.own_jid.clone();
        let convo = self
            .conversations
            .entry(bare_jid.clone())
            .or_insert_with(|| ConversationView::new(bare_jid.clone(), own_jid));

        // Update last-message preview in sidebar
        self.sidebar.set_last_message(&bare_jid, &msg.body);

        convo.push_message(DisplayMessage {
            id: msg.id.clone(),
            from: msg.from,
            body: msg.body,
            own: false,
            timestamp: chrono::Utc::now().timestamp_millis(),
            reply_preview: None,
            edited: false,
            retracted: false,
            is_encrypted: false,
        });

        // B5: increment unread if not the currently active conversation
        if self.active_jid.as_deref() != Some(bare_jid.as_str()) {
            self.sidebar.increment_unread(&bare_jid);
        }

        // I4: spawn image fetch tasks for image URL messages
        let pending_images = convo.take_pending_images();
        // E5: spawn link preview fetch tasks for any URLs in the message
        let pending = convo.take_pending_previews();

        let jid = bare_jid;

        // Combine tasks: images take priority (return early with image handle)
        if !pending_images.is_empty() {
            let jid2 = jid.clone();
            let image_task = Task::future(async move {
                for (msg_id, url) in pending_images {
                    let client = reqwest::Client::new();
                    match client
                        .get(&url)
                        .timeout(std::time::Duration::from_secs(15))
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if let Ok(bytes) = resp.bytes().await {
                                let handle =
                                    iced::widget::image::Handle::from_bytes(bytes.to_vec());
                                return Message::Conversation(
                                    jid2.clone(),
                                    super::conversation::Message::AttachmentLoaded(msg_id, handle),
                                );
                            }
                        }
                        Err(e) => {
                            tracing::debug!("I4: failed to fetch image for {}: {}", url, e);
                        }
                    }
                }
                Message::Conversation(jid2.clone(), super::conversation::Message::Send)
            });
            return Some(image_task);
        }

        if pending.is_empty() {
            return None;
        }

        Some(Task::future(async move {
            for (msg_id, url) in pending {
                let client = reqwest::Client::new();
                match client
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if let Ok(html) = resp.text().await {
                            let preview =
                                crate::xmpp::modules::link_preview::parse_preview(&url, &html);
                            if preview.title.is_some()
                                || preview.description.is_some()
                                || preview.image_url.is_some()
                            {
                                return Message::Conversation(
                                    jid.clone(),
                                    super::conversation::Message::LinkPreviewReady(msg_id, preview),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("E5: failed to fetch link preview for {}: {}", url, e);
                    }
                }
            }
            Message::Conversation(jid.clone(), super::conversation::Message::Send)
        }))
    }

    pub fn on_presence(&mut self, jid: &str, available: bool) {
        // D1: if this is a MUC presence (room@conf/nick), update occupant panel
        if let Some((room_jid, nick)) = jid.split_once('/') {
            if self.muc_jids.contains(room_jid) {
                self.on_muc_presence(room_jid, nick, "Participant", "None", available);
                return;
            }
        }
        self.sidebar.on_presence(jid, available);
    }

    /// E3: update the reactions map for a given conversation.
    pub fn on_reaction_received(&mut self, msg_id: String, from: String, emojis: Vec<String>) {
        // Find which conversation contains this msg_id
        for convo in self.conversations.values_mut() {
            if convo.messages().iter().any(|m| m.id == msg_id) {
                convo
                    .reactions
                    .entry(msg_id)
                    .or_default()
                    .insert(from, emojis);
                return;
            }
        }
    }

    /// M2: K4 delivery receipt — update message state to Delivered for the matching conversation.
    pub fn on_message_delivered(&mut self, jid: &str, msg_id: String) {
        if let Some(convo) = self.conversations.get_mut(jid) {
            let _ = convo.update(conversation::Message::MessageDelivered(msg_id));
        }
    }

    /// M2: K5 read marker — update message state to Read for the matching conversation.
    pub fn on_message_read(&mut self, jid: &str, msg_id: String) {
        if let Some(convo) = self.conversations.get_mut(jid) {
            let _ = convo.update(conversation::Message::MessageRead(msg_id));
        }
    }

    /// L3: XEP-0425 — apply local tombstone when a message is moderated in a MUC.
    pub fn on_message_moderated(&mut self, room_jid: &str, msg_id: &str) {
        if let Some(convo) = self.conversations.get_mut(room_jid) {
            convo.apply_retraction(msg_id);
        }
    }

    /// Drain pending outgoing engine commands; called by App::update.
    pub fn drain_commands(&mut self) -> Vec<XmppCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    /// B4: mutable access to a conversation for injecting loaded history.
    pub fn get_conversation_mut(&mut self, jid: &str) -> Option<&mut ConversationView> {
        self.conversations.get_mut(jid)
    }

    /// B4: get the bound JID for this session.
    pub fn own_jid(&self) -> &str {
        &self.own_jid
    }

    /// A5: get the JID of the currently active (foreground) conversation.
    pub fn active_jid(&self) -> Option<&str> {
        self.active_jid.as_deref()
    }

    /// B6: Get the ID of the last message in a conversation (for mark-read).
    pub fn last_message_id(&self, jid: &str) -> Option<String> {
        self.conversations
            .get(jid)
            .and_then(|cv| cv.messages().last())
            .map(|m| m.id.clone())
    }

    /// Set the last-message preview for a JID in the sidebar.
    /// Called by the parent (App) when loading history from the database.
    pub fn set_sidebar_last_message(&mut self, jid: &str, body: &str) {
        self.sidebar.set_last_message(jid, body);
    }

    pub fn update(&mut self, msg: Message) -> Action {
        match msg {
            Message::Sidebar(smsg) => {
                match self.sidebar.update(smsg) {
                    sidebar::Action::None => Action::None,
                    sidebar::Action::Task(t) => Action::Task(t.map(Message::Sidebar)),
                    sidebar::Action::SelectContact(jid) => {
                        let own_jid = self.own_jid.clone();
                        self.conversations
                            .entry(jid.clone())
                            .or_insert_with(|| ConversationView::new(jid.clone(), own_jid));
                        self.active_jid = Some(jid.clone());
                        // B5: clear unread count when conversation is opened
                        self.sidebar.clear_unread(&jid);
                        // L1: record seen count so new messages can be highlighted
                        if let Some(convo) = self.conversations.get_mut(&jid) {
                            convo.mark_seen();
                        }
                        Action::ContactSelected(jid)
                    }
                    sidebar::Action::AddContact(jid) => {
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::AddContact(jid));
                        Action::None
                    }
                    sidebar::Action::RemoveContact(jid) => {
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::RemoveContact(jid));
                        Action::None
                    }
                    sidebar::Action::JoinRoom { jid, nick } => {
                        self.on_join_room(&jid);
                        self.muc_own_nicks.insert(jid.clone(), nick.clone());
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::JoinRoom { jid, nick });
                        Action::None
                    }
                    sidebar::Action::CreateRoom {
                        local,
                        service,
                        nick,
                    } => {
                        let room_jid = format!("{}@{}", local, service);
                        self.on_join_room(&room_jid);
                        self.muc_own_nicks.insert(room_jid.clone(), nick.clone());
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::CreateRoom {
                                local,
                                service,
                                nick,
                            });
                        Action::None
                    }
                    sidebar::Action::RenameContact { jid, name } => {
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::RenameContact { jid, name });
                        Action::None
                    }
                    sidebar::Action::SetPresence(status) => {
                        self.pending_commands
                            .push(XmppCommand::SetPresence(status.clone()));
                        Action::SetPresence(status)
                    }
                    sidebar::Action::OpenSettings => Action::OpenSettings,
                    sidebar::Action::OpenAccountSwitcher => Action::OpenAccountSwitcher,
                }
            }

            Message::CloseConversation(jid) => {
                self.conversations.remove(&jid);
                if self.active_jid.as_deref() == Some(jid.as_str()) {
                    self.active_jid = None;
                }
                Action::None
            }
            Message::OpenSettings => Action::OpenSettings,
            Message::OpenAccountSwitcher => Action::OpenAccountSwitcher,

            Message::PeerTyping(jid, composing) => {
                if composing {
                    self.typing_peers.insert(jid, std::time::Instant::now());
                } else {
                    self.typing_peers.remove(&jid);
                }
                Action::None
            }

            Message::ToggleMute(jid) => {
                if let Some(convo) = self.conversations.get_mut(&jid) {
                    convo.is_muted = !convo.is_muted;
                }
                Action::ToggleMute(jid)
            }

            Message::SetPresence(status) => {
                // C2: queue SetPresence command for the engine (App drains pending_commands)
                self.pending_commands
                    .push(XmppCommand::SetPresence(status.clone()));
                Action::SetPresence(status)
            }

            // M2: K4 delivery receipt — update message state
            Message::MessageDelivered(jid, msg_id) => {
                if let Some(convo) = self.conversations.get_mut(&jid) {
                    let _ = convo.update(conversation::Message::MessageDelivered(msg_id));
                }
                Action::None
            }

            // M2: K5 read marker — update message state
            Message::MessageRead(jid, msg_id) => {
                if let Some(convo) = self.conversations.get_mut(&jid) {
                    let _ = convo.update(conversation::Message::MessageRead(msg_id));
                }
                Action::None
            }

            // K1: room config form received from server
            Message::RoomConfigFormReceived(room_jid, config) => {
                self.pending_room_config = Some((room_jid, config));
                Action::None
            }
            // K1: room configuration accepted — room is now live
            Message::RoomConfigured(room_jid) => {
                self.pending_room_config = None;
                let own = self.own_jid.clone();
                self.conversations
                    .entry(room_jid.clone())
                    .or_insert_with(|| ConversationView::new(room_jid.clone(), own));
                self.active_jid = Some(room_jid);
                Action::None
            }
            Message::RoomConfigNameChanged(v) => {
                if let Some((_, ref mut cfg)) = self.pending_room_config {
                    cfg.room_name = Some(v);
                }
                Action::None
            }
            Message::RoomConfigPublicChanged(v) => {
                if let Some((_, ref mut cfg)) = self.pending_room_config {
                    cfg.public = Some(v);
                }
                Action::None
            }
            Message::RoomConfigPersistentChanged(v) => {
                if let Some((_, ref mut cfg)) = self.pending_room_config {
                    cfg.persistent_room = Some(v);
                }
                Action::None
            }
            Message::SubmitRoomConfig => {
                if let Some((room_jid, config)) = self.pending_room_config.take() {
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::ConfigureRoom { room_jid, config });
                }
                Action::None
            }
            Message::DismissRoomConfig => {
                self.pending_room_config = None;
                Action::None
            }

            // K3: incoming invitation received from engine event
            Message::RoomInvitationReceived {
                room_jid,
                from_jid,
                reason,
            } => {
                // De-duplicate by room_jid
                self.pending_invitations.retain(|i| i.room_jid != room_jid);
                self.pending_invitations.push(PendingInvitation {
                    room_jid,
                    from_jid,
                    reason,
                });
                Action::None
            }

            // K3: user accepted an incoming invitation — join the room
            Message::AcceptInvitation(room_jid) => {
                self.pending_invitations.retain(|i| i.room_jid != room_jid);
                let nick = self.own_jid.split('@').next().unwrap_or("me").to_string();
                self.on_join_room(&room_jid);
                self.muc_own_nicks.insert(room_jid.clone(), nick.clone());
                self.pending_commands.push(XmppCommand::JoinRoom {
                    jid: room_jid,
                    nick,
                });
                Action::None
            }

            // K3: user declined an incoming invitation — just remove it, no XMPP stanza needed
            Message::DeclineInvitation(room_jid) => {
                self.pending_invitations.retain(|i| i.room_jid != room_jid);
                Action::None
            }

            // K3: open the outgoing invite dialog for a given room
            Message::OpenInviteDialog(room_jid) => {
                self.pending_invite_dialog = Some((room_jid, String::new(), String::new()));
                Action::None
            }

            Message::InviteJidChanged(v) => {
                if let Some((_, ref mut invitee, _)) = self.pending_invite_dialog {
                    *invitee = v;
                }
                Action::None
            }

            Message::InviteReasonChanged(v) => {
                if let Some((_, _, ref mut reason)) = self.pending_invite_dialog {
                    *reason = v;
                }
                Action::None
            }

            // K3: send the invitation and close the dialog
            Message::SubmitInvite => {
                if let Some((room_jid, invitee, reason)) = self.pending_invite_dialog.take() {
                    if !invitee.trim().is_empty() {
                        let reason_opt = if reason.trim().is_empty() {
                            None
                        } else {
                            Some(reason)
                        };
                        self.pending_commands.push(XmppCommand::SendRoomInvitation {
                            room: room_jid,
                            user: invitee,
                            reason: reason_opt,
                        });
                    }
                }
                Action::None
            }

            Message::DismissInviteDialog => {
                self.pending_invite_dialog = None;
                Action::None
            }

            // K2: room list received from MUC service — store for browse dialog
            Message::RoomListReceived(rooms) => {
                self.public_rooms = rooms;
                Action::None
            }

            Message::Conversation(jid, cmsg) => {
                // J3: intercept ToggleMute to bubble up to App
                if let super::conversation::Message::ToggleMute = cmsg {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        convo.is_muted = !convo.is_muted;
                    }
                    return Action::ToggleMute(jid);
                }

                // G1: intercept Close to remove the conversation
                if let super::conversation::Message::Close = cmsg {
                    self.conversations.remove(&jid);
                    if self.active_jid.as_deref() == Some(jid.as_str()) {
                        self.active_jid = None;
                    }
                    return Action::None;
                }

                // C4: intercept BlockPeer to queue a block command for the engine
                if let super::conversation::Message::BlockPeer = cmsg {
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::BlockJid(jid.clone()));
                    self.conversations.remove(&jid);
                    if self.active_jid.as_deref() == Some(jid.as_str()) {
                        self.active_jid = None;
                    }
                    return Action::None;
                }

                // C4: intercept UnblockPeer to queue an unblock command
                if let super::conversation::Message::UnblockPeer = cmsg {
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::UnblockJid(jid.clone()));
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        convo.peer_blocked = false;
                    }
                    return Action::None;
                }

                // G2: intercept ComposingStarted/Paused to send chat state to server
                if let super::conversation::Message::ComposingStarted = cmsg {
                    self.pending_commands.push(XmppCommand::SendChatState {
                        to: jid.clone(),
                        composing: true,
                    });
                    return Action::None;
                }
                if let super::conversation::Message::ComposingPaused = cmsg {
                    self.pending_commands.push(XmppCommand::SendChatState {
                        to: jid.clone(),
                        composing: false,
                    });
                    return Action::None;
                }

                // E3: intercept SendReaction to queue a reaction command for the engine.
                if let super::conversation::Message::SendReaction(ref msg_id, ref emoji) = cmsg {
                    // Optimistically add emoji to local state so the UI updates immediately.
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let own = self.own_jid.clone();
                        let by_jid = convo.reactions.entry(msg_id.clone()).or_default();
                        let emojis = by_jid.entry(own).or_default();
                        if !emojis.contains(emoji) {
                            emojis.push(emoji.clone());
                        }
                    }
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::SendReaction {
                            to: jid.clone(),
                            msg_id: msg_id.clone(),
                            emojis: vec![emoji.clone()],
                        });
                    return Action::None;
                }

                // R1: intercept RetractReaction — update local state + send empty reaction set
                if let super::conversation::Message::RetractReaction(ref msg_id, ref emoji) = cmsg {
                    let remaining: Vec<String> =
                        if let Some(convo) = self.conversations.get_mut(&jid) {
                            let own = &self.own_jid;
                            if let Some(by_jid) = convo.reactions.get_mut(msg_id) {
                                if let Some(emojis) = by_jid.get_mut(own) {
                                    emojis.retain(|e| e != emoji);
                                    emojis.clone()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        };
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::SendReaction {
                            to: jid.clone(),
                            msg_id: msg_id.clone(),
                            emojis: remaining,
                        });
                    return Action::None;
                }

                // E2: intercept RetractMessage to send retraction to engine and apply tombstone
                if let super::conversation::Message::RetractMessage(ref msg_id) = cmsg {
                    let mid = msg_id.clone();
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        convo.apply_retraction(&mid);
                    }
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::SendRetraction {
                            to: jid.clone(),
                            origin_id: mid,
                        });
                    return Action::None;
                }

                // L3: intercept ModerateMessage — apply local tombstone + send moderation stanza
                if let super::conversation::Message::ModerateMessage(ref msg_id, ref reason) = cmsg
                {
                    let mid = msg_id.clone();
                    let rsn = reason.clone();
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        convo.apply_retraction(&mid);
                    }
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::ModerateMessage {
                            room_jid: jid.clone(),
                            message_id: mid,
                            reason: rsn,
                        });
                    return Action::None;
                }

                // OMEMO: intercept OpenOmemoTrust to bubble up to App
                if let super::conversation::Message::OpenOmemoTrust(ref peer_jid) = cmsg {
                    return Action::OpenOmemoTrust(peer_jid.clone());
                }

                // E4/I3: intercept OpenFilePicker to spawn picker task
                if let super::conversation::Message::OpenFilePicker = cmsg {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let task = convo
                            .update(super::conversation::Message::OpenFilePicker)
                            .map(move |m| Message::Conversation(jid.clone(), m));
                        return Action::Task(task);
                    }
                    return Action::None;
                }

                // Intercept Send to queue a command for the engine.
                if let super::conversation::Message::Send = cmsg {
                    return Action::Task(self.handle_send(jid));
                }

                if let Some(convo) = self.conversations.get_mut(&jid) {
                    let jid2 = jid.clone();
                    let task = convo
                        .update(cmsg)
                        .map(move |m| Message::Conversation(jid2.clone(), m));
                    Action::Task(task)
                } else {
                    Action::None
                }
            }

            // M4: forward VoiceTick to the active conversation's voice state machine
            Message::VoiceTick => {
                if let Some(jid) = self.active_jid.clone() {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let task = convo
                            .update(super::conversation::Message::VoiceTick)
                            .map(move |m| Message::Conversation(jid.clone(), m));
                        return Action::Task(task);
                    }
                }
                Action::None
            }

            // R3: Ctrl+B — wrap composer text in bold markers (**text**)
            Message::ComposerBold => {
                if let Some(jid) = self.active_jid.clone() {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let new_text = apply_markdown_wrap(&convo.composer, "**");
                        let task = convo
                            .update(super::conversation::Message::ComposerChanged(new_text))
                            .map(move |m| Message::Conversation(jid.clone(), m));
                        return Action::Task(task);
                    }
                }
                Action::None
            }

            // R3: Ctrl+I — wrap composer text in italic markers (*text*)
            Message::ComposerItalic => {
                if let Some(jid) = self.active_jid.clone() {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let new_text = apply_markdown_wrap(&convo.composer, "*");
                        let task = convo
                            .update(super::conversation::Message::ComposerChanged(new_text))
                            .map(move |m| Message::Conversation(jid.clone(), m));
                        return Action::Task(task);
                    }
                }
                Action::None
            }
            // OMEMO: bubbled from conversation — handled by App
            Message::OpenOmemoTrust(peer_jid) => Action::OpenOmemoTrust(peer_jid),
        }
    }

    /// Handle Send — process attachments, edits, or plain text sends.
    fn handle_send(&mut self, jid: String) -> Task<Message> {
        if let Some(convo) = self.conversations.get_mut(&jid) {
            // E4/I3: if there are pending attachments, request upload slots first
            if !convo.pending_attachments.is_empty() {
                let attachments = std::mem::take(&mut convo.pending_attachments);
                for att in attachments {
                    let mime = if att.name.ends_with(".png") {
                        "image/png"
                    } else if att.name.ends_with(".jpg") || att.name.ends_with(".jpeg") {
                        "image/jpeg"
                    } else if att.name.ends_with(".gif") {
                        "image/gif"
                    } else if att.name.ends_with(".wav") {
                        "audio/wav"
                    } else {
                        "application/octet-stream"
                    };
                    self.pending_upload_targets.push((jid.clone(), att.path));
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::RequestUploadSlot {
                            filename: att.name,
                            size: att.size,
                            mime: mime.to_string(),
                        });
                }
                // BUG-6: reset the conversation's voice state machine so the
                // composer reappears after a voice note is queued for upload.
                let _ = convo.update(super::conversation::Message::Send);
                return Task::none();
            }

            // E1: if in edit mode, send correction instead
            if let Some((original_id, _)) = convo.take_edit_mode() {
                let new_body = convo.take_draft();
                if !new_body.trim().is_empty() {
                    convo.apply_correction(&original_id, &new_body);
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::SendCorrection {
                            to: jid.clone(),
                            original_id,
                            new_body,
                        });
                }
                return Task::none();
            }

            let body = convo.take_draft();
            if !body.trim().is_empty() {
                let msg_id = uuid::Uuid::new_v4().to_string();
                let own_jid = self.own_jid.clone();
                convo.push_message(DisplayMessage {
                    id: msg_id.clone(),
                    from: own_jid.clone(),
                    body: body.clone(),
                    own: true,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    reply_preview: None,
                    edited: false,
                    retracted: false,
                    is_encrypted: false,
                });

                // E5: spawn link preview fetch tasks for any URLs in the message
                let pending = convo.take_pending_previews();
                if !pending.is_empty() {
                    let jid_for_preview = jid.clone();
                    let jid_for_cmd = jid.clone();
                    let body_clone = body.clone();
                    let preview_task: Task<Message> = Task::future(async move {
                        for (msg_id, url) in pending {
                            let client = reqwest::Client::new();
                            match client
                                .get(&url)
                                .timeout(std::time::Duration::from_secs(10))
                                .send()
                                .await
                            {
                                Ok(resp) => {
                                    if let Ok(html) = resp.text().await {
                                        let preview =
                                            crate::xmpp::modules::link_preview::parse_preview(
                                                &url, &html,
                                            );
                                        if preview.title.is_some()
                                            || preview.description.is_some()
                                            || preview.image_url.is_some()
                                        {
                                            return Message::Conversation(
                                                jid_for_preview.clone(),
                                                super::conversation::Message::LinkPreviewReady(
                                                    msg_id, preview,
                                                ),
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        "E5: failed to fetch link preview for {}: {}",
                                        url,
                                        e
                                    );
                                }
                            }
                        }
                        Message::Conversation(jid_for_preview, super::conversation::Message::Send)
                    });
                    let use_omemo = self
                        .conversations
                        .get(&jid_for_cmd)
                        .is_some_and(|cv| cv.is_encryption_enabled);
                    if use_omemo {
                        self.pending_commands
                            .push(XmppCommand::OmemoEncryptMessage {
                                to: jid_for_cmd,
                                body: body_clone,
                            });
                    } else {
                        self.pending_commands.push(XmppCommand::SendMessage {
                            to: jid_for_cmd,
                            body: body_clone,
                            id: msg_id.clone(),
                        });
                    }
                    return preview_task;
                }

                let use_omemo = self
                    .conversations
                    .get(&jid)
                    .is_some_and(|cv| cv.is_encryption_enabled);
                if use_omemo {
                    self.pending_commands
                        .push(XmppCommand::OmemoEncryptMessage {
                            to: jid.clone(),
                            body,
                        });
                } else {
                    self.pending_commands.push(XmppCommand::SendMessage {
                        to: jid.clone(),
                        body,
                        id: msg_id,
                    });
                }
            }
        }
        Task::none()
    }

    #[allow(dead_code)]
    pub fn draft_for(&self, jid: &str) -> &str {
        self.conversations
            .get(jid)
            .map_or("", |cv| cv.composer.as_str())
    }

    pub fn view(&self, vctx: &super::ViewContext<'_>) -> Element<'_, Message> {
        // G6: collect JIDs that have a non-empty draft
        let drafts: Vec<String> = self
            .conversations
            .iter()
            .filter(|(_, cv)| !cv.composer.trim().is_empty())
            .map(|(jid, _)| jid.clone())
            .collect();
        // K1: derive default conference service from own JID domain
        let conference_service = self
            .own_jid
            .split('@')
            .nth(1)
            .map(|domain| format!("conference.{}", domain))
            .unwrap_or_default();
        // MULTI: pass active account info to sidebar for the indicator bar.
        let account_info = self
            .active_account_id
            .as_ref()
            .map(|id| (id, self.account_unread));
        let sidebar_view = self
            .sidebar
            .view_with_drafts(
                &drafts,
                &conference_service,
                account_info,
                vctx,
                &self.muc_jids,
            )
            .map(Message::Sidebar);

        // K3: if there is a pending invite dialog, show it instead of the conversation
        let main_area: Element<Message> =
            if let Some((ref room_jid, ref invitee, ref reason)) = self.pending_invite_dialog {
                let invitee_input = text_input("Invitee JID", invitee)
                    .on_input(Message::InviteJidChanged)
                    .padding(6);
                let reason_input = text_input("Reason (optional)", reason)
                    .on_input(Message::InviteReasonChanged)
                    .padding(6);
                let cancel_btn = button(text("Cancel").size(13))
                    .on_press(Message::DismissInviteDialog)
                    .padding([4, 12]);
                let invite_btn = button(text("Invite").size(13))
                    .on_press(Message::SubmitInvite)
                    .padding([4, 12]);
                let btn_row = row![cancel_btn, invite_btn]
                    .spacing(8)
                    .align_y(Alignment::Center);
                let modal_col = column![
                    text(format!("Invite to {}", room_jid)).size(16),
                    text("Invitee JID:").size(12),
                    invitee_input,
                    text("Reason:").size(12),
                    reason_input,
                    btn_row,
                ]
                .spacing(12)
                .padding(24)
                .width(Length::Fill);
                container(modal_col)
                    .center(Length::Fill)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            // K1: if there is a pending room config, show the config modal instead of the conversation
            } else if let Some((ref room_jid, ref cfg)) = self.pending_room_config {
                let name_val = cfg.room_name.clone().unwrap_or_default();
                let public_val = cfg.public.unwrap_or(true);
                let persistent_val = cfg.persistent_room.unwrap_or(true);
                let name_input = text_input("Room display name", &name_val)
                    .on_input(Message::RoomConfigNameChanged)
                    .padding(6);
                let public_check =
                    checkbox("Public room", public_val).on_toggle(Message::RoomConfigPublicChanged);
                let persistent_check = checkbox("Persistent room", persistent_val)
                    .on_toggle(Message::RoomConfigPersistentChanged);
                let cancel_btn = button(text("Cancel").size(13))
                    .on_press(Message::DismissRoomConfig)
                    .padding([4, 12]);
                let create_btn = button(text("Create Room").size(13))
                    .on_press(Message::SubmitRoomConfig)
                    .padding([4, 12]);
                let btn_row = row![cancel_btn, create_btn]
                    .spacing(8)
                    .align_y(Alignment::Center);
                let modal_col = column![
                    text(format!("Configure New Room: {}", room_jid)).size(16),
                    text("Room Name:").size(12),
                    name_input,
                    public_check,
                    persistent_check,
                    btn_row,
                ]
                .spacing(12)
                .padding(24)
                .width(Length::Fill);
                container(modal_col)
                    .center(Length::Fill)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            } else {
                // K3: render incoming invitation banners at the top of the main area
                let invite_banners: Vec<Element<Message>> = self
                    .pending_invitations
                    .iter()
                    .map(|inv| {
                        let label = if let Some(ref r) = inv.reason {
                            format!(
                                "{} invited you to {} — \"{}\"",
                                inv.from_jid, inv.room_jid, r
                            )
                        } else {
                            format!("{} invited you to {}", inv.from_jid, inv.room_jid)
                        };
                        let accept_btn = button(text("Accept").size(11))
                            .on_press(Message::AcceptInvitation(inv.room_jid.clone()))
                            .padding([2, 8]);
                        let decline_btn = button(text("Decline").size(11))
                            .on_press(Message::DeclineInvitation(inv.room_jid.clone()))
                            .padding([2, 8]);
                        container(
                            row![
                                text(label).size(12).shaping(Shaping::Advanced),
                                accept_btn,
                                decline_btn
                            ]
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                        )
                        .padding([4, 8])
                        .width(Length::Fill)
                        .into()
                    })
                    .collect();

                let conversation_area: Element<Message> = match &self.active_jid {
                    None => container(text("Select a contact to start chatting").size(14))
                        .center(Length::Fill)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into(),

                    Some(jid) => {
                        if let Some(convo) = self.conversations.get(jid) {
                            let jid2 = jid.clone();
                            // G2: show "is typing" if peer typed in the last 5 seconds
                            let is_typing = self
                                .typing_peers
                                .get(jid)
                                .is_some_and(|t| t.elapsed().as_secs() < 5);
                            // L2: pass occupant list and own nick for @mention autocomplete
                            let occupants: &[crate::ui::muc_panel::OccupantEntry] = self
                                .muc_occupants
                                .get(jid.as_str())
                                .map_or(&[], Vec::as_slice);
                            let own_nick = self
                                .muc_own_nicks
                                .get(jid.as_str())
                                .map_or("", String::as_str);
                            let conv_view = convo
                                .view(vctx, occupants, own_nick)
                                .map(move |m| Message::Conversation(jid2.clone(), m));
                            if is_typing {
                                let indicator = container(
                                    text(format!("{} is typing…", jid))
                                        .size(11)
                                        .shaping(Shaping::Advanced),
                                )
                                .padding([2, 8]);
                                column![conv_view, indicator]
                                    .height(iced::Length::Fill)
                                    .into()
                            } else {
                                conv_view
                            }
                        } else {
                            container(text("Loading…")).center(Length::Fill).into()
                        }
                    }
                };

                // K3: prepend invitation banners above the conversation area
                if invite_banners.is_empty() {
                    conversation_area
                } else {
                    let mut col = column(invite_banners).spacing(4);
                    col = col.push(conversation_area);
                    col.height(Length::Fill).width(Length::Fill).into()
                }
            };

        // Show "Signed in as" only for the first 5 seconds after connection.
        let show_jid_label = self.connected_at.is_some_and(|t| t.elapsed().as_secs() < 5);
        let settings_btn = iced::widget::button(text("Settings").size(11))
            .on_press(Message::OpenSettings)
            .padding([2, 8]);
        // C2: presence status picker buttons
        let available_btn =
            iced::widget::button(text("● Available").size(11).shaping(Shaping::Advanced))
                .on_press(Message::SetPresence(PresenceStatus::Available))
                .padding([2, 8]);
        let away_btn = iced::widget::button(text("○ Away").size(11).shaping(Shaping::Advanced))
            .on_press(Message::SetPresence(PresenceStatus::Away))
            .padding([2, 8]);
        let dnd_btn = iced::widget::button(text("⛔ DND").size(11).shaping(Shaping::Advanced))
            .on_press(Message::SetPresence(PresenceStatus::DoNotDisturb))
            .padding([2, 8]);
        let status_bar = if show_jid_label {
            let own_label = text(format!("Signed in as {}", self.own_jid)).size(11);
            container(
                row![own_label, available_btn, away_btn, dnd_btn, settings_btn]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
            )
            .padding([2, 8])
            .width(Length::Fill)
        } else {
            container(
                row![available_btn, away_btn, dnd_btn, settings_btn]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
            )
            .padding([2, 8])
            .width(Length::Fill)
        };

        // D1: if active JID is a MUC room, show the occupant panel on the right
        let content_row: Element<Message> = if let Some(ref jid) = self.active_jid {
            if self.muc_jids.contains(jid.as_str()) {
                if let Some(panel) = self.muc_panels.get(jid) {
                    // K3: map OccupantPanel messages into ChatScreen messages
                    let panel_view = panel.view().map(|msg| match msg {
                        super::muc_panel::Message::OpenInviteDialog(room_jid) => {
                            Message::OpenInviteDialog(room_jid)
                        }
                    });
                    row![sidebar_view, main_area, panel_view]
                        .height(Length::Fill)
                        .width(Length::Fill)
                        .into()
                } else {
                    row![sidebar_view, main_area]
                        .height(Length::Fill)
                        .width(Length::Fill)
                        .into()
                }
            } else {
                row![sidebar_view, main_area]
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .into()
            }
        } else {
            row![sidebar_view, main_area]
                .height(Length::Fill)
                .width(Length::Fill)
                .into()
        };

        column![content_row, status_bar].into()
    }
}

// R3: Wrap `text` in markdown `marker` characters.
//
// Behaviour:
//   - Non-empty text: wrap entire string → `{marker}{text}{marker}`
//   - Empty / whitespace-only: insert placeholder → `{marker}{marker}`
//
// Examples (marker = "**"):
//   "hello" → "**hello**"
//   ""      → "****"
fn apply_markdown_wrap(text: &str, marker: &str) -> String {
    if text.trim().is_empty() {
        format!("{marker}{marker}")
    } else {
        format!("{marker}{text}{marker}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmpp::IncomingMessage;

    #[test]
    fn chat_screen_new() {
        let s = ChatScreen::new("me@example.com".into());
        assert_eq!(s.own_jid, "me@example.com");
        assert!(s.active_jid.is_none());
        assert!(s.conversations.is_empty());
    }

    #[test]
    fn on_message_received_creates_conversation() {
        let mut s = ChatScreen::new("me@example.com".into());
        s.on_message_received(IncomingMessage {
            id: "1".into(),
            from: "alice@example.com/res".into(),
            body: "Hello!".into(),
            is_historical: false,
        });

        assert!(s.conversations.contains_key("alice@example.com"));
        assert_eq!(s.conversations["alice@example.com"].messages().len(), 1);
    }

    #[test]
    fn g6_draft_preserved_on_conversation_switch() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        // Open alice's conversation and type a draft
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        if let Some(convo) = s.conversations.get_mut("alice@example.com") {
            convo.composer = "half-typed message".into();
        }
        // Switch to bob's conversation
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "bob@example.com".into(),
        )));
        // Alice's draft should be preserved
        assert_eq!(s.draft_for("alice@example.com"), "half-typed message");
        // Bob's composer should be empty
        assert_eq!(s.draft_for("bob@example.com"), "");
    }

    #[test]
    fn drain_commands_empties_queue() {
        let mut s = ChatScreen::new("me@example.com".into());
        s.pending_commands.push(XmppCommand::SendMessage {
            to: "alice@example.com".into(),
            body: "hi".into(),
            id: "test-id".into(),
        });
        let drained = s.drain_commands();
        assert_eq!(drained.len(), 1);
        assert!(s.pending_commands.is_empty());
    }

    // R3: markdown wrap helper tests

    #[test]
    fn apply_markdown_wrap_bold_wraps_text() {
        assert_eq!(apply_markdown_wrap("hello", "**"), "**hello**");
    }

    #[test]
    fn apply_markdown_wrap_italic_wraps_text() {
        assert_eq!(apply_markdown_wrap("world", "*"), "*world*");
    }

    #[test]
    fn apply_markdown_wrap_empty_text_produces_placeholder() {
        assert_eq!(apply_markdown_wrap("", "**"), "****");
    }

    #[test]
    fn apply_markdown_wrap_whitespace_only_treated_as_empty() {
        assert_eq!(apply_markdown_wrap("   ", "**"), "****");
    }

    // R3: ComposerBold / ComposerItalic integration

    #[test]
    fn composer_bold_wraps_active_conversation() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        if let Some(convo) = s.conversations.get_mut("alice@example.com") {
            convo.composer = "hello".into();
        }
        let _ = s.update(Message::ComposerBold);
        assert_eq!(s.draft_for("alice@example.com"), "**hello**");
    }

    #[test]
    fn composer_italic_wraps_active_conversation() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        if let Some(convo) = s.conversations.get_mut("alice@example.com") {
            convo.composer = "hi".into();
        }
        let _ = s.update(Message::ComposerItalic);
        assert_eq!(s.draft_for("alice@example.com"), "*hi*");
    }

    #[test]
    fn composer_bold_no_active_conversation_is_noop() {
        let mut s = ChatScreen::new("me@example.com".into());
        // No active conversation set — should not panic
        let _ = s.update(Message::ComposerBold);
    }

    /// BUG-13: VoiceEncodingDone processed through ChatScreen stages
    /// an attachment in the conversation, so the subsequent Send
    /// intercept can find it.
    #[test]
    fn voice_encoding_done_stages_attachment_via_chatscreen() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        // Dispatch VoiceEncodingDone through ChatScreen — it should
        // fall through to convo.update and stage the attachment.
        let _ = s.update(Message::Conversation(
            "alice@example.com".into(),
            super::conversation::Message::VoiceEncodingDone(
                std::path::PathBuf::from("/tmp/voice_test.wav"),
                44100,
            ),
        ));
        // The attachment should now be staged in the conversation
        let convo = s.conversations.get("alice@example.com").unwrap();
        assert_eq!(
            convo.pending_attachments.len(),
            1,
            "VoiceEncodingDone should stage a voice attachment"
        );
        assert_eq!(convo.pending_attachments[0].name, "voice_message.wav");
    }

    /// BUG-13: simulate VoiceEncodingDone → Send flow and verify that
    /// the upload target and RequestUploadSlot command are queued.
    #[test]
    fn voice_encoding_done_triggers_upload() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        // Open a conversation
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        // Simulate VoiceEncodingDone by manually staging the attachment
        // and setting voice state, then sending Send.
        if let Some(convo) = s.conversations.get_mut("alice@example.com") {
            convo
                .pending_attachments
                .push(super::conversation::Attachment {
                    name: "voice_message.wav".into(),
                    path: std::path::PathBuf::from("/tmp/voice_test.wav"),
                    size: 44100,
                    progress: 0,
                    thumbnail: None,
                });
        }
        // Dispatch Send — this should be intercepted by ChatScreen,
        // which should take the pending_attachments and queue an upload.
        let _ = s.update(Message::Conversation(
            "alice@example.com".into(),
            super::conversation::Message::Send,
        ));
        // Verify: upload target should be queued
        let targets = s.drain_upload_targets();
        assert_eq!(
            targets.len(),
            1,
            "expected 1 upload target for voice message"
        );
        assert_eq!(targets[0].0, "alice@example.com");
        // Verify: RequestUploadSlot command should be queued
        let cmds = s.drain_commands();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, XmppCommand::RequestUploadSlot { .. })),
            "expected RequestUploadSlot command"
        );
    }

    /// BUG-13: full ChatScreen flow — VoiceEncodingDone followed by
    /// the Task::done(Send) that it produces.  After both steps the
    /// upload target and slot request must be queued.
    #[test]
    fn voice_full_flow_encoding_then_send() {
        use crate::ui::sidebar;
        let mut s = ChatScreen::new("me@example.com".into());
        let _ = s.update(Message::Sidebar(sidebar::Message::SelectContact(
            "alice@example.com".into(),
        )));
        // Step 1: dispatch VoiceEncodingDone — this stages the attachment
        // and returns a Task that would produce Message::Send.
        let _ = s.update(Message::Conversation(
            "alice@example.com".into(),
            super::conversation::Message::VoiceEncodingDone(
                std::path::PathBuf::from("/tmp/voice_test.wav"),
                44100,
            ),
        ));
        // At this point the attachment should be staged
        assert_eq!(
            s.conversations["alice@example.com"]
                .pending_attachments
                .len(),
            1,
            "VoiceEncodingDone should stage 1 attachment"
        );
        // No upload target yet (Send hasn't fired)
        assert!(
            s.drain_upload_targets().is_empty(),
            "no upload target until Send is processed"
        );

        // Step 2: simulate the Task::done(Send) by dispatching Send.
        let _ = s.update(Message::Conversation(
            "alice@example.com".into(),
            super::conversation::Message::Send,
        ));
        // Now the attachment should be consumed and upload queued
        assert!(
            s.conversations["alice@example.com"]
                .pending_attachments
                .is_empty(),
            "pending_attachments should be empty after Send"
        );
        let targets = s.drain_upload_targets();
        assert_eq!(
            targets.len(),
            1,
            "expected 1 upload target after voice Send"
        );
        let cmds = s.drain_commands();
        assert!(
            cmds.iter()
                .any(|c| matches!(c, XmppCommand::RequestUploadSlot { .. })),
            "expected RequestUploadSlot command after voice Send"
        );
    }
}
