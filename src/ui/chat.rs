// Task P2.3 — ChatScreen: sidebar + conversation view
// This is the main screen shown after a successful XMPP login.

use std::collections::HashMap;

use iced::{
    widget::{column, container, row, text},
    Element, Length, Task,
};

use crate::xmpp::{IncomingMessage, RosterContact, XmppCommand};

use super::{
    conversation::{ConversationView, DisplayMessage},
    sidebar::{self, SidebarScreen},
};

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
}

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(sidebar::Message),
    Conversation(String, super::conversation::Message),
    CloseConversation(String), // G1: close a conversation by JID
    PeerTyping(String, bool),  // G2: (jid, composing)
    OpenSettings,              // F3: open settings panel
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
        }
    }

    pub fn set_roster(&mut self, contacts: Vec<RosterContact>) {
        self.sidebar.set_contacts(contacts);
    }

    /// Route an incoming message to the right conversation bucket.
    pub fn on_message_received(&mut self, msg: IncomingMessage) {
        let bare_jid = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
        let own_jid = self.own_jid.clone();
        let convo = self
            .conversations
            .entry(bare_jid.clone())
            .or_insert_with(|| ConversationView::new(bare_jid.clone(), own_jid));

        convo.push_message(DisplayMessage {
            id: msg.id,
            from: msg.from,
            body: msg.body,
            own: false,
            timestamp: chrono::Utc::now().timestamp_millis(),
            reply_preview: None,
        });

        // B5: increment unread if not the currently active conversation
        if self.active_jid.as_deref() != Some(bare_jid.as_str()) {
            self.sidebar.increment_unread(&bare_jid);
        }
    }

    pub fn on_presence(&mut self, jid: &str, available: bool) {
        self.sidebar.on_presence(jid, available);
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

    /// B6: Get the ID of the last message in a conversation (for mark-read).
    pub fn last_message_id(&self, jid: &str) -> Option<String> {
        self.conversations
            .get(jid)
            .and_then(|cv| cv.messages().last())
            .map(|m| m.id.clone())
    }

    /// G2: update typing state from an engine event.
    pub fn on_peer_typing(&mut self, jid: &str, composing: bool) {
        if composing {
            self.typing_peers.insert(jid.to_owned(), std::time::Instant::now());
        } else {
            self.typing_peers.remove(jid);
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Sidebar(smsg) => {
                // When user selects a contact, open (or switch to) that conversation.
                let sidebar::Message::SelectContact(ref jid) = smsg;
                let jid = jid.clone();
                let own_jid = self.own_jid.clone();
                self.conversations
                    .entry(jid.clone())
                    .or_insert_with(|| ConversationView::new(jid.clone(), own_jid));
                self.active_jid = Some(jid.clone());
                // B5: clear unread count when conversation is opened
                self.sidebar.clear_unread(&jid);
                self.sidebar.update(smsg).map(Message::Sidebar)
            }

            Message::CloseConversation(jid) => {
                self.conversations.remove(&jid);
                if self.active_jid.as_deref() == Some(jid.as_str()) {
                    self.active_jid = None;
                }
                Task::none()
            }
            Message::OpenSettings => Task::none(), // handled by App

            Message::PeerTyping(jid, composing) => {
                if composing {
                    self.typing_peers.insert(jid, std::time::Instant::now());
                } else {
                    self.typing_peers.remove(&jid);
                }
                Task::none()
            }

            Message::Conversation(jid, cmsg) => {
                // G1: intercept Close to remove the conversation
                if let super::conversation::Message::Close = cmsg {
                    return self.update(Message::CloseConversation(jid));
                }

                // C4: intercept BlockPeer to queue a block command for the engine
                if let super::conversation::Message::BlockPeer = cmsg {
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::BlockJid(jid.clone()));
                    return self.update(Message::CloseConversation(jid));
                }

                // C4: intercept UnblockPeer to queue an unblock command
                if let super::conversation::Message::UnblockPeer = cmsg {
                    self.pending_commands
                        .push(crate::xmpp::XmppCommand::UnblockJid(jid.clone()));
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        convo.peer_blocked = false;
                    }
                    return Task::none();
                }

                // Intercept Send to queue a command for the engine.
                if let super::conversation::Message::Send = cmsg {
                    if let Some(convo) = self.conversations.get_mut(&jid) {
                        let body = convo.take_draft();
                        if !body.trim().is_empty() {
                            // Also push the message to our own view optimistically.
                            let own_jid = self.own_jid.clone();
                            convo.push_message(DisplayMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                from: own_jid.clone(),
                                body: body.clone(),
                                own: true,
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                reply_preview: None,
                            });
                            self.pending_commands.push(XmppCommand::SendMessage {
                                to: jid.clone(),
                                body,
                            });
                        }
                    }
                    return Task::none();
                }

                if let Some(convo) = self.conversations.get_mut(&jid) {
                    let jid2 = jid.clone();
                    convo
                        .update(cmsg)
                        .map(move |m| Message::Conversation(jid2.clone(), m))
                } else {
                    Task::none()
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn draft_for(&self, jid: &str) -> &str {
        self.conversations
            .get(jid)
            .map(|cv| cv.composer.as_str())
            .unwrap_or("")
    }

    pub fn view(&self) -> Element<'_, Message> {
        // G6: collect JIDs that have a non-empty draft
        let drafts: Vec<String> = self
            .conversations
            .iter()
            .filter(|(_, cv)| !cv.composer.trim().is_empty())
            .map(|(jid, _)| jid.clone())
            .collect();
        let sidebar_view = self.sidebar.view_with_drafts(&drafts).map(Message::Sidebar);

        let main_area: Element<Message> = match &self.active_jid {
            None => container(text("Select a contact to start chatting").size(14))
                .center(Length::Fill)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),

            Some(jid) => {
                if let Some(convo) = self.conversations.get(jid) {
                    let jid2 = jid.clone();
                    // G2: show "is typing" if peer typed in the last 5 seconds
                    let is_typing = self.typing_peers.get(jid)
                        .map(|t| t.elapsed().as_secs() < 5)
                        .unwrap_or(false);
                    let conv_view = convo.view().map(move |m| Message::Conversation(jid2.clone(), m));
                    if is_typing {
                        let indicator = container(
                            text(format!("{} is typing…", jid)).size(11)
                        )
                        .padding([2, 8]);
                        column![conv_view, indicator].into()
                    } else {
                        conv_view
                    }
                } else {
                    container(text("Loading…")).center(Length::Fill).into()
                }
            }
        };

        let own_label = text(format!("Signed in as {}", self.own_jid)).size(11);
        let settings_btn = iced::widget::button(text("Settings").size(11))
            .on_press(Message::OpenSettings)
            .padding([2, 8]);
        let status_bar = container(
            row![own_label, settings_btn].spacing(8).align_y(iced::Alignment::Center),
        )
        .padding([2, 8])
        .width(Length::Fill);

        column![
            row![sidebar_view, main_area]
                .height(Length::Fill)
                .width(Length::Fill),
            status_bar,
        ]
        .into()
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
        });

        assert!(s.conversations.contains_key("alice@example.com"));
        assert_eq!(s.conversations["alice@example.com"].messages().len(), 1);
    }

    #[test]
    fn drain_commands_empties_queue() {
        let mut s = ChatScreen::new("me@example.com".into());
        s.pending_commands.push(XmppCommand::SendMessage {
            to: "alice@example.com".into(),
            body: "hi".into(),
        });
        let drained = s.drain_commands();
        assert_eq!(drained.len(), 1);
        assert!(s.pending_commands.is_empty());
    }
}
