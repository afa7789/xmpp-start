// Task P2.3 — ChatView: message list (virtual scroll)
// Task P2.4 — MessageComposer: text input + send button
// Source reference: apps/fluux/src/components/ChatView.tsx
//                   apps/fluux/src/components/MessageComposer.tsx
// Scroll strategy: docs/SCROLL_STRATEGY.md

use iced::{
    widget::{
        button, column, container, row, scrollable, text, text_input,
    },
    Alignment, Element, Length, Task,
};
use iced::widget::scrollable::{AbsoluteOffset, Id};

/// A single message shown in the conversation view.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub id: String,
    pub from: String,
    pub body: String,
    pub own: bool, // true if sent by this account
}

#[derive(Debug, Clone)]
pub struct ConversationView {
    pub peer_jid: String,
    messages: Vec<DisplayMessage>,
    composer: String,
    scroll_id: Id,
    scroll_offset: AbsoluteOffset,
    own_jid: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ComposerChanged(String),
    Send,
    Scrolled(AbsoluteOffset),
    ScrollToBottom,
}

impl ConversationView {
    pub fn new(peer_jid: String, own_jid: String) -> Self {
        Self {
            peer_jid,
            messages: vec![],
            composer: String::new(),
            scroll_id: Id::new("conversation"),
            scroll_offset: AbsoluteOffset::default(),
            own_jid,
        }
    }

    pub fn push_message(&mut self, msg: DisplayMessage) {
        self.messages.push(msg);
    }

    pub fn take_draft(&mut self) -> String {
        std::mem::take(&mut self.composer)
    }

    pub fn messages(&self) -> &[DisplayMessage] {
        &self.messages
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::ComposerChanged(v) => {
                self.composer = v;
                Task::none()
            }
            Message::Send => {
                // Caller handles actual send; we just clear the composer.
                self.composer.clear();
                Task::none()
            }
            Message::Scrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
            Message::ScrollToBottom => {
                let bottom = AbsoluteOffset { x: 0.0, y: f32::MAX };
                scrollable::scroll_to::<Message>(self.scroll_id.clone(), bottom)
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        // ---- Message list ----
        let messages: Vec<Element<Message>> = self
            .messages
            .iter()
            .map(|m| {
                let sender = if m.own {
                    "You".to_string()
                } else {
                    m.from
                        .split('/')
                        .next()
                        .unwrap_or(&m.from)
                        .to_string()
                };

                let bubble = column![
                    text(sender).size(11),
                    text(m.body.clone()).size(14),
                ]
                .spacing(2)
                .padding([6, 10]);

                let align = if m.own {
                    Alignment::End
                } else {
                    Alignment::Start
                };

                container(bubble)
                    .width(Length::Fill)
                    .align_x(align)
                    .into()
            })
            .collect();

        let list_col = messages
            .into_iter()
            .fold(column![].spacing(4).padding(8), |col, el| col.push(el));

        let scroll_area = scrollable(list_col)
            .id(self.scroll_id.clone())
            .on_scroll(|vp| Message::Scrolled(vp.absolute_offset()))
            .height(Length::Fill)
            .width(Length::Fill);

        // ---- Scroll position + jump-to-bottom button ----
        let scroll_info = text(format!("↕ {:.0}px", self.scroll_offset.y)).size(11);
        let jump_btn = button("↓ bottom").on_press(Message::ScrollToBottom).padding([4, 10]);
        let scroll_bar = row![scroll_info, jump_btn]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding([2, 8]);

        // ---- Composer ----
        let can_send = !self.composer.trim().is_empty();
        let send_btn = if can_send {
            button("Send").on_press(Message::Send)
        } else {
            button("Send")
        };

        let composer_row = row![
            text_input("Type a message…", &self.composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::Send)
                .padding(10)
                .width(Length::Fill),
            send_btn.padding([10, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([4, 8]);

        column![
            // Header
            container(
                text(format!("Chat with {}", self.peer_jid)).size(14)
            )
            .padding([8, 12])
            .width(Length::Fill),
            // Message list
            scroll_area,
            // Scroll position bar
            scroll_bar,
            // Composer
            composer_row,
        ]
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_view_empty() {
        let cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        assert!(cv.messages().is_empty());
    }

    #[test]
    fn push_message_increments_count() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.push_message(DisplayMessage {
            id: "1".into(),
            from: "alice@example.com".into(),
            body: "Hello".into(),
            own: false,
        });
        assert_eq!(cv.messages().len(), 1);
    }

    #[test]
    fn take_draft_clears_composer() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.composer = "hello world".into();
        let draft = cv.take_draft();
        assert_eq!(draft, "hello world");
        assert!(cv.composer.is_empty());
    }

    #[test]
    fn send_clears_composer() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.composer = "test message".into();
        cv.update(Message::Send);
        assert!(cv.composer.is_empty());
    }
}
