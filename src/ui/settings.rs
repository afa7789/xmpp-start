// F3: Settings panel screen

use iced::{
    widget::{button, column, container, row, text, text_input, toggler},
    Alignment, Element, Length, Task,
};

use crate::config::{self, Settings, Theme};

#[derive(Debug, Clone)]
pub struct SettingsScreen {
    settings: Settings,
    status_input: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ThemeToggled,
    NotificationsToggled(bool),
    SoundToggled(bool),
    FontSizeIncreased,
    FontSizeDecreased,
    StatusInputChanged(String),
    // S6: privacy toggles
    SendReceiptsToggled(bool),
    SendTypingToggled(bool),
    SendReadMarkersToggled(bool),
    // J10: MAM archiving default mode selector
    MamModeSelected(String),
    // M1: system theme and time format
    SystemThemeToggled(bool),
    TimeFormatToggled(String),
    // H2: avatar upload
    AvatarSelected(Vec<u8>, String),
    // K6: contact sorting preference
    SortContactsSelected(String),
    Back,
}

impl SettingsScreen {
    pub fn new(settings: Settings) -> Self {
        Self {
            status_input: settings.status_message.clone().unwrap_or_default(),
            settings,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::ThemeToggled => {
                self.settings.theme = match self.settings.theme {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::NotificationsToggled(enabled) => {
                self.settings.notifications_enabled = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SoundToggled(enabled) => {
                self.settings.sound_enabled = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::FontSizeIncreased => {
                if self.settings.font_size < 20 {
                    self.settings.font_size += 1;
                    let _ = config::save(&self.settings);
                }
                Task::none()
            }
            Message::FontSizeDecreased => {
                if self.settings.font_size > 12 {
                    self.settings.font_size -= 1;
                    let _ = config::save(&self.settings);
                }
                Task::none()
            }
            Message::StatusInputChanged(value) => {
                self.status_input = value.clone();
                self.settings.status_message = if self.status_input.trim().is_empty() {
                    None
                } else {
                    Some(self.status_input.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SendReceiptsToggled(enabled) => {
                self.settings.send_receipts = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SendTypingToggled(enabled) => {
                self.settings.send_typing = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SendReadMarkersToggled(enabled) => {
                self.settings.send_read_markers = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::MamModeSelected(mode) => {
                self.settings.mam_default_mode = Some(mode.clone());
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SystemThemeToggled(enabled) => {
                self.settings.use_system_theme = enabled;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::TimeFormatToggled(fmt) => {
                self.settings.time_format = if fmt == "12h" {
                    crate::config::TimeFormat::TwelveHour
                } else {
                    crate::config::TimeFormat::TwentyFourHour
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            // H2: avatar upload - send to engine via config/command
            Message::AvatarSelected(data, mime_type) => {
                // The settings screen returns this to App, which forwards to ChatScreen → XmppCommand
                // For now, we just save it to config for persistence; the actual upload happens elsewhere
                self.settings.avatar_data = Some(data);
                let _ = config::save(&self.settings);
                Task::none()
            }
            // K6: contact sorting preference
            Message::SortContactsSelected(sort) => {
                self.settings.contact_sort = sort.clone();
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::Back => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Settings").size(20);

        let is_dark = self.settings.theme == Theme::Dark;
        let theme_row = row![
            text("Dark theme").size(14).width(Length::Fill),
            toggler(is_dark).on_toggle(|_| Message::ThemeToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // M1: system theme toggle
        let system_theme_row = row![
            text("Use system theme").size(14).width(Length::Fill),
            toggler(self.settings.use_system_theme).on_toggle(Message::SystemThemeToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // M1: time format selector
        let time_fmt = match self.settings.time_format {
            crate::config::TimeFormat::TwentyFourHour => "24h",
            crate::config::TimeFormat::TwelveHour => "12h",
        };
        let time_format_row: Element<Message> = row![
            text("Time format:").size(14).width(Length::Fill),
            button("24h")
                .on_press(Message::TimeFormatToggled("24h".into()))
                .padding([4, 8]),
            button("12h")
                .on_press(Message::TimeFormatToggled("12h".into()))
                .padding([4, 8]),
            text(time_fmt).size(14),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        let notif_row = row![
            text("Notifications").size(14).width(Length::Fill),
            toggler(self.settings.notifications_enabled).on_toggle(Message::NotificationsToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let sound_row = row![
            text("Sound").size(14).width(Length::Fill),
            toggler(self.settings.sound_enabled).on_toggle(Message::SoundToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let font_row = row![
            text(format!("Font size: {}", self.settings.font_size))
                .size(14)
                .width(Length::Fill),
            button("-")
                .on_press(Message::FontSizeDecreased)
                .padding([4, 10]),
            button("+")
                .on_press(Message::FontSizeIncreased)
                .padding([4, 10]),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let status_row = row![
            text("Status:").size(14).width(80),
            text_input("e.g. In a meeting", &self.status_input)
                .on_input(Message::StatusInputChanged)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // S6: privacy toggles
        let receipts_row = row![
            text("Send delivery receipts").size(14).width(Length::Fill),
            toggler(self.settings.send_receipts).on_toggle(Message::SendReceiptsToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let typing_row = row![
            text("Send typing indicators").size(14).width(Length::Fill),
            toggler(self.settings.send_typing).on_toggle(Message::SendTypingToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let read_markers_row = row![
            text("Send read markers").size(14).width(Length::Fill),
            toggler(self.settings.send_read_markers).on_toggle(Message::SendReadMarkersToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // J10: MAM archiving mode selector
        let mam_mode = self
            .settings
            .mam_default_mode
            .as_deref()
            .unwrap_or("roster");
        let mam_mode_row: Element<Message> = row![
            text("MAM:").size(14).width(Length::Fill),
            button("roster")
                .on_press(Message::MamModeSelected("roster".into()))
                .padding([4, 8]),
            button("always")
                .on_press(Message::MamModeSelected("always".into()))
                .padding([4, 8]),
            button("never")
                .on_press(Message::MamModeSelected("never".into()))
                .padding([4, 8]),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        let back_btn = button("Back").on_press(Message::Back).padding([6, 14]);

        let content = column![
            title,
            theme_row,
            system_theme_row,
            time_format_row,
            notif_row,
            sound_row,
            font_row,
            status_row,
            receipts_row,
            typing_row,
            read_markers_row,
            mam_mode_row,
            back_btn
        ]
        .spacing(16)
        .padding(24)
        .width(400);

        container(content)
            .center(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
