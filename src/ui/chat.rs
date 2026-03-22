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
}

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(sidebar::Message),
    Conversation(String, super::conversation::Message),
    CloseConversation(String), // G1: close a conversation by JID
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

            Message::Conversation(jid, cmsg) => {
                // G1: intercept Close to remove the conversation
                if let super::conversation::Message::Close = cmsg {
                    return self.update(Message::CloseConversation(jid));
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

    /// G6: Return the non-empty draft for a JID, or empty string.
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
                    convo
                        .view()
                        .map(move |m| Message::Conversation(jid2.clone(), m))
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
