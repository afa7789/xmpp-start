// Task P0.1 — Scroll benchmark screen (go/no-go spike for iced 0.13 with 10k items)
// Task P0.4 — Validate virtual scrolling strategy

use iced::{
    widget::{button, column, container, row, scrollable, text},
    Background, Color, Element, Length, Task,
};

const MESSAGE_COUNT: usize = 10_000;

/// Scrollable widget identifier — used for programmatic scroll_to().
fn scrollable_id() -> scrollable::Id {
    scrollable::Id::new("benchmark-list")
}

/// A single synthetic message in the benchmark list.
#[derive(Debug, Clone)]
pub struct BenchmarkMessage {
    pub id: usize,
    pub sender: String,
    pub body: String,
}

/// Screen that renders 10 000 message rows inside a scrollable to validate
/// iced 0.13 scroll performance with emoji text and avatar placeholders.
#[derive(Debug, Clone)]
pub struct BenchmarkScreen {
    messages: Vec<BenchmarkMessage>,
    scroll_offset: scrollable::AbsoluteOffset,
}

#[derive(Debug, Clone)]
pub enum Message {
    Back,
    Scrolled(scrollable::AbsoluteOffset),
    ScrollToBottom,
}

/// Cycle through a small set of avatar colors for visual variety.
fn avatar_color(id: usize) -> Color {
    const COLORS: &[Color] = &[
        Color::from_rgb(0.36, 0.58, 0.93), // blue
        Color::from_rgb(0.29, 0.69, 0.49), // green
        Color::from_rgb(0.86, 0.37, 0.37), // red
        Color::from_rgb(0.80, 0.60, 0.20), // amber
        Color::from_rgb(0.55, 0.40, 0.80), // purple
    ];
    COLORS[id % COLORS.len()]
}

/// Generate a varied body string; cycles through several templates so the
/// list is not monotonous and always includes emoji.
fn make_body(id: usize) -> String {
    match id % 8 {
        0 => format!("Hello 👋 — message #{id}"),
        1 => format!("Done ✅ — task #{id} complete"),
        2 => format!("Looking good 🔥 #{id}"),
        3 => format!("On my way 🚀 #{id}"),
        4 => format!("Great work 💪 #{id}"),
        5 => format!("See you soon 👀 #{id}"),
        6 => format!("Thanks! 🙏 #{id}"),
        _ => format!("Ok 👍 #{id}"),
    }
}

impl Default for BenchmarkScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkScreen {
    pub fn new() -> Self {
        let messages = (0..MESSAGE_COUNT)
            .map(|id| BenchmarkMessage {
                id,
                sender: format!("User {}", id % 50),
                body: make_body(id),
            })
            .collect();

        Self {
            messages,
            scroll_offset: scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Back => Task::none(), // caller handles screen transition
            Message::Scrolled(offset) => {
                self.scroll_offset = offset;
                Task::none()
            }
            Message::ScrollToBottom => {
                // Scroll to a very large y value; iced clamps to content height.
                let bottom = scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: f32::MAX,
                };
                scrollable::scroll_to(scrollable_id(), bottom)
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let back_btn = button("← Back").on_press(Message::Back).padding([6, 16]);
        let scroll_to_bottom_btn = button("Scroll to bottom")
            .on_press(Message::ScrollToBottom)
            .padding([6, 16]);

        let scroll_label = text(format!("Scroll: {:.0}px", self.scroll_offset.y)).size(14);

        let header = container(
            row![
                back_btn,
                text(format!(
                    "Scroll Benchmark — {} messages",
                    self.messages.len()
                ))
                .size(18),
                scroll_label,
                scroll_to_bottom_btn,
            ]
            .spacing(16)
            .align_y(iced::Alignment::Center),
        )
        .padding(12);

        let rows = self.messages.iter().map(|m| {
            let avatar = container(text(""))
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(40.0))
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(avatar_color(m.id))),
                    ..container::Style::default()
                });

            row![
                avatar,
                column![text(&m.sender).size(14), text(&m.body).size(13),].spacing(2),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .into()
        });

        let list = scrollable(column(rows).spacing(6).padding(8))
            .id(scrollable_id())
            .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset()))
            .width(Length::Fill)
            .height(Length::Fill);

        column![header, list]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_exactly_10_000_messages() {
        let screen = BenchmarkScreen::new();
        assert_eq!(screen.messages.len(), MESSAGE_COUNT);
    }

    #[test]
    fn message_bodies_contain_wave_emoji() {
        let screen = BenchmarkScreen::new();
        let has_wave = screen.messages.iter().any(|m| m.body.contains('👋'));
        assert!(has_wave, "expected at least one body containing 👋");
    }

    #[test]
    fn benchmark_message_is_debug_and_clone() {
        let msg = BenchmarkMessage {
            id: 0,
            sender: "Alice".to_string(),
            body: "Hello 👋".to_string(),
        };
        let cloned = msg.clone();
        // If Debug is not derived this line won't compile.
        let _ = format!("{:?}", cloned);
    }

    #[test]
    fn test_scroll_offset_updates() {
        let mut screen = BenchmarkScreen::new();
        assert_eq!(screen.scroll_offset.y, 0.0);

        let offset = scrollable::AbsoluteOffset { x: 0.0, y: 1234.5 };
        let _ = screen.update(Message::Scrolled(offset));

        assert_eq!(screen.scroll_offset.y, 1234.5);
        assert_eq!(screen.scroll_offset.x, 0.0);
    }

    #[test]
    fn test_generates_10k_messages() {
        let screen = BenchmarkScreen::new();
        assert_eq!(screen.messages.len(), 10_000);
    }
}
