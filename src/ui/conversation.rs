// Task P2.3 — ChatView: message list (virtual scroll)
// Task P2.4 — MessageComposer: text input + send button
// Source reference: apps/fluux/src/components/ChatView.tsx
//                   apps/fluux/src/components/MessageComposer.tsx
// Scroll strategy: docs/SCROLL_STRATEGY.md

use iced::widget::scrollable::{AbsoluteOffset, Id};
use iced::widget::text::Span as IcedSpan;
use iced::{
    font,
    widget::{button, column, container, rich_text, row, scrollable, span, text, text_input},
    Alignment, Color, Element, Font, Length, Task,
};

use crate::ui::styling::{self, SpanStyle};

/// A single message shown in the conversation view.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    own_jid: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ComposerChanged(String),
    Send,
    Scrolled(AbsoluteOffset),
    ScrollToBottom,
    CopyToClipboard(String), // G7: copy message body
    Noop,                    // G7: no-op for task returns
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

    #[allow(dead_code)]
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
                let bottom = AbsoluteOffset {
                    x: 0.0,
                    y: f32::MAX,
                };
                scrollable::scroll_to::<Message>(self.scroll_id.clone(), bottom)
            }
            Message::CopyToClipboard(text) => {
                iced::clipboard::write::<Message>(text)
            }
            Message::Noop => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // ---- Message list ----
        let messages: Vec<Element<Message>> = self
            .messages
            .iter()
            .map(|m| {
                let sender = if m.own {
                    "You".to_string()
                } else {
                    m.from.split('/').next().unwrap_or(&m.from).to_string()
                };

                let styled_spans = styling::parse(&m.body);
                let body_widget = build_styled_text(&styled_spans);
                // G7: copy button per message
                let copy_btn = button(text("Copy").size(10))
                    .on_press(Message::CopyToClipboard(m.body.clone()))
                    .padding([2, 6]);
                let bubble = column![
                    row![text(sender).size(11), copy_btn].spacing(8).align_y(Alignment::Center),
                    body_widget
                ]
                .spacing(2)
                .padding([6, 10]);

                let align = if m.own {
                    Alignment::End
                } else {
                    Alignment::Start
                };

                container(bubble).width(Length::Fill).align_x(align).into()
            })
            .collect();

        let list_col = messages
            .into_iter()
            .fold(column![].spacing(4).padding(8), iced::widget::Column::push);

        let scroll_area = scrollable(list_col)
            .id(self.scroll_id.clone())
            .on_scroll(|vp| Message::Scrolled(vp.absolute_offset()))
            .height(Length::Fill)
            .width(Length::Fill);

        // ---- Scroll position + jump-to-bottom button ----
        let scroll_info = text(format!("↕ {:.0}px", self.scroll_offset.y)).size(11);
        let jump_btn = button("↓ bottom")
            .on_press(Message::ScrollToBottom)
            .padding([4, 10]);
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
            container(text(format!("Chat with {}", self.peer_jid)).size(14))
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

/// Map parsed `Span`s to an iced `rich_text` widget.
fn build_styled_text(spans: &[styling::Span]) -> Element<'static, Message> {
    // IcedSpan<'a, Link> — Link must match the Element's message type.
    let iced_spans: Vec<IcedSpan<'static, Message>> = spans
        .iter()
        .map(|s| {
            let t: IcedSpan<'static, Message> = span(s.text.clone());
            match s.style {
                SpanStyle::Plain => t,
                SpanStyle::Bold => t.font(Font {
                    weight: font::Weight::Bold,
                    ..Font::DEFAULT
                }),
                SpanStyle::Italic => t.font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                }),
                SpanStyle::Code => t
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.2, 0.8, 0.4)),
                SpanStyle::Strike => t.strikethrough(true),
                SpanStyle::Quote => t.color(Color::from_rgb(0.6, 0.6, 0.6)).font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                }),
            }
        })
        .collect();
    rich_text(iced_spans).size(14).into()
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
        let _ = cv.update(Message::Send);
        assert!(cv.composer.is_empty());
    }
}
