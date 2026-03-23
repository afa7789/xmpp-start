// M7: About modal — version, XEPs, license, GitHub link

use iced::{
    widget::{button, column, container, row, text},
    Element, Length,
};

#[derive(Debug, Clone)]
pub struct AboutScreen {
    pub version: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Back,
}

impl AboutScreen {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn update(&mut self, _msg: Message) {
        // No state changes needed
    }

    pub fn view(&self) -> Element<'static, Message> {
        let title = text("About xmpp-start").size(24);

        let version_text = format!("Version {}", self.version);
        let version_row = row![
            text("Version:").size(14).width(Length::Fixed(100.0)),
            text(version_text).size(14),
        ]
        .spacing(8);

        let xep_count = 26;
        let xeps_text = format!("{} XEPs implemented", xep_count);
        let xeps_row = row![
            text("XEPs:").size(14).width(Length::Fixed(100.0)),
            text(xeps_text).size(14),
        ]
        .spacing(8);

        let license_row = row![
            text("License:").size(14).width(Length::Fixed(100.0)),
            text("MIT").size(14),
        ]
        .spacing(8);

        let github_row = row![
            text("GitHub:").size(14).width(Length::Fixed(100.0)),
            text("github.com/xmpp-start/xmpp-start").size(14),
        ]
        .spacing(8);

        let description = text("Native XMPP desktop messenger built with Rust and iced.").size(13);

        let close_btn = button("Close").on_press(Message::Back).padding([8, 24]);

        let content = column![
            title,
            iced::widget::Space::with_height(Length::Fixed(16.0)),
            version_row,
            xeps_row,
            license_row,
            github_row,
            iced::widget::Space::with_height(Length::Fixed(16.0)),
            description,
            iced::widget::Space::with_height(Length::Fixed(16.0)),
            close_btn,
        ]
        .spacing(12)
        .padding(24)
        .align_x(iced::Alignment::Start);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
