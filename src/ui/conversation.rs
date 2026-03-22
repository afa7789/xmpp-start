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

use chrono::{TimeZone, Utc};

// G4: /me action message prefix (XEP-0245)
const ME_PREFIX: &str = "/me ";

fn is_me_action(body: &str) -> bool {
    body.len() >= ME_PREFIX.len()
        && body[..ME_PREFIX.len()].eq_ignore_ascii_case(ME_PREFIX)
}

use crate::ui::avatar::{jid_color, jid_initial};
use crate::ui::styling::{self, SpanStyle};

/// A single message shown in the conversation view.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    #[allow(dead_code)]
    pub id: String,
    pub from: String,
    pub body: String,
    pub own: bool,         // true if sent by this account
    pub timestamp: i64,   // unix milliseconds (G5)
}

#[derive(Debug, Clone)]
pub struct ConversationView {
    pub peer_jid: String,
    messages: Vec<DisplayMessage>,

    pub(crate) composer: String,

    scroll_id: Id,
    scroll_offset: AbsoluteOffset,
    #[allow(dead_code)]
    own_jid: String,
    /// J3: whether notifications are muted for this conversation
    pub is_muted: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ComposerChanged(String),
    Send,
    Scrolled(AbsoluteOffset),
    ScrollToBottom,
    CopyToClipboard(String), // G7: copy message body to clipboard
    Noop,                    // G7: no-op task return
    Close,                   // G1: close this conversation
    ToggleMute,              // J3: toggle notification mute
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
            is_muted: false,
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
            Message::CopyToClipboard(text) => iced::clipboard::write::<Message>(text),
            Message::Noop => Task::none(),
            Message::Close => Task::none(),      // handled by ChatScreen
            Message::ToggleMute => Task::none(), // handled by ChatScreen → App
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // ---- Message list (G5: grouping + date separators) ----
        let mut rows: Vec<Element<Message>> = Vec::new();
        let mut prev_date: Option<chrono::NaiveDate> = None;
        let mut prev_sender: Option<String> = None;
        let mut prev_ts: Option<i64> = None;

        for m in &self.messages {
            let sender = if m.own {
                "You".to_string()
            } else {
                m.from.split('/').next().unwrap_or(&m.from).to_string()
            };

            // G5: date separator when calendar date changes
            let msg_date = Utc
                .timestamp_millis_opt(m.timestamp)
                .single()
                .map(|dt| dt.date_naive());

            if let Some(date) = msg_date {
                if prev_date.map_or(true, |pd| pd != date) {
                    let label = date.format("%b %-d").to_string();
                    let sep = container(text(format!("── {} ──", label)).size(11))
                        .width(Length::Fill)
                        .align_x(Alignment::Center)
                        .padding([4, 0]);
                    rows.push(sep.into());
                    prev_date = Some(date);
                }
            }

            // G5: suppress sender label for consecutive same-sender within 120s
            let same_sender = prev_sender.as_deref() == Some(sender.as_str());
            let within_120s =
                prev_ts.map_or(false, |pt| (m.timestamp - pt).abs() < 120_000);
            let show_sender = !(same_sender && within_120s);

            // G4: /me action rendering
            let body_widget: Element<Message> = if is_me_action(&m.body) {
                let action_text = &m.body[ME_PREFIX.len()..];
                let action_str = format!("* {} {} *", sender, action_text);
                let italic_span: IcedSpan<'static, Message> = span(action_str).font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                });
                rich_text([italic_span]).size(14).into()
            } else {
                let styled_spans = styling::parse(&m.body);
                build_styled_text(&styled_spans)
            };

            // G7: copy button (hidden for /me actions)
            let copy_btn = button(text("Copy").size(10))
                .on_press(Message::CopyToClipboard(m.body.clone()))
                .padding([2, 6]);

            let align = if m.own { Alignment::End } else { Alignment::Start };

            let row_elem: Element<Message> = if is_me_action(&m.body) {
                // /me: centered italic, no avatar, no sender label
                container(
                    container(body_widget)
                        .padding([4, 12])
                )
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into()
            } else if !m.own {
                // H5: avatar + sender + body for incoming messages
                let from_bare = m.from.split('/').next().unwrap_or(&m.from);
                let color = jid_color(from_bare);
                let initial = jid_initial(from_bare).to_string();
                let avatar = container(text(initial).size(11))
                    .width(24)
                    .height(24)
                    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(color)),
                        ..Default::default()
                    })
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center);

                let text_col = if show_sender {
                    column![
                        row![text(sender.clone()).size(11), copy_btn]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        body_widget
                    ]
                    .spacing(2)
                    .padding([0, 6])
                } else {
                    column![body_widget].spacing(2).padding([0, 6])
                };

                let bubble = row![avatar, text_col].spacing(6).align_y(Alignment::Start);
                container(bubble)
                    .width(Length::Fill)
                    .align_x(align)
                    .padding([2, 8])
                    .into()
            } else {
                // Own message: right-aligned, no avatar
                let text_col = if show_sender {
                    column![
                        row![text(sender.clone()).size(11), copy_btn]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        body_widget
                    ]
                    .spacing(2)
                    .padding([6, 10])
                } else {
                    column![body_widget].spacing(2).padding([2, 10])
                };
                container(text_col)
                    .width(Length::Fill)
                    .align_x(align)
                    .into()
            };

            rows.push(row_elem);
            prev_sender = Some(sender);
            prev_ts = Some(m.timestamp);
        }

        let list_col = rows
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

        // G1: close button in header
        let close_btn = button("✕").on_press(Message::Close).padding([4, 8]);
        let mute_btn = if self.is_muted {
            button("🔕").on_press(Message::ToggleMute).padding([4, 8])
        } else {
            button("🔔").on_press(Message::ToggleMute).padding([4, 8])
        };
        let header = container(
            row![
                text(format!("Chat with {}", self.peer_jid))
                    .size(14)
                    .width(Length::Fill),
                mute_btn,
                close_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill);

        column![header, scroll_area, scroll_bar, composer_row].into()
    }
}

/// Map parsed `Span`s to an iced `rich_text` widget.
fn build_styled_text(spans: &[styling::Span]) -> Element<'static, Message> {
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

    fn make_msg(id: &str, from: &str, body: &str, own: bool) -> DisplayMessage {
        DisplayMessage {
            id: id.into(),
            from: from.into(),
            body: body.into(),
            own,
            timestamp: 0,
        }
    }

    #[test]
    fn conversation_view_empty() {
        let cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        assert!(cv.messages().is_empty());
    }

    #[test]
    fn push_message_increments_count() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.push_message(make_msg("1", "alice@example.com", "Hello", false));
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

    #[test]
    fn me_action_detection_case_insensitive() {
        assert!(is_me_action("/me waves"));
        assert!(is_me_action("/ME waves"));
        assert!(is_me_action("/Me waves"));
        assert!(!is_me_action("hello"));
        assert!(!is_me_action("/me")); // no trailing space + content
        assert!(!is_me_action("/menu"));
    }
}
