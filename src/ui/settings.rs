// F3: Settings panel screen

use iced::{
    widget::{
        button, column, container, horizontal_space, row, scrollable, text, text_input, toggler,
    },
    Alignment, Element, Length, Task,
};

use super::account_details::AccountInfo;
use super::blocklist::BlocklistPanel;
use crate::config::{self, Settings, Theme};

// ---------------------------------------------------------------------------
// Section card helper — wraps grouped settings in a themed card container
// ---------------------------------------------------------------------------

/// Build a card container with title, optional description, and content.
fn settings_section_with_desc<'a>(
    title: &str,
    description: Option<&str>,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let heading = text(title.to_string()).size(17).font(iced::Font {
        weight: iced::font::Weight::Bold,
        ..Default::default()
    });

    let mut header_col = column![heading].spacing(2);
    if let Some(desc) = description {
        header_col = header_col.push(
            text(desc.to_string())
                .size(12)
                .color(iced::Color::from_rgba(0.5, 0.5, 0.5, 0.8)),
        );
    }

    // Thin divider between header and content
    let divider = container(horizontal_space())
        .height(1)
        .width(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    a: 0.12,
                    ..palette.background.strong.color
                })),
                ..Default::default()
            }
        });

    container(column![header_col, divider, content].spacing(12))
        .padding([18, 20])
        .width(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(iced::Background::Color(palette.background.weak.color)),
                border: iced::Border {
                    color: iced::Color {
                        a: 0.25,
                        ..palette.background.strong.color
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.06),
                    offset: iced::Vector::new(0.0, 2.0),
                    blur_radius: 8.0,
                },
                ..Default::default()
            }
        })
        .into()
}

/// A subtle horizontal divider used between individual setting rows within a card.
fn setting_divider<'a>() -> Element<'a, Message> {
    container(horizontal_space())
        .height(1)
        .width(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    a: 0.08,
                    ..palette.background.strong.color
                })),
                ..Default::default()
            }
        })
        .into()
}

/// Category group heading displayed above related sections.
fn category_heading<'a>(label: &str) -> Element<'a, Message> {
    text(label.to_string())
        .size(13)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        })
        .color(iced::Color::from_rgba(0.45, 0.45, 0.45, 0.9))
        .into()
}

/// Build a segmented-control button: highlighted when active, transparent when inactive.
fn segmented_btn(
    label: &str,
    active: bool,
) -> button::Button<'_, Message, iced::Theme, iced::Renderer> {
    let btn_text = text(label.to_string()).size(13);
    let b = button(btn_text).padding([5, 12]);
    if active {
        b.style(|theme: &iced::Theme, status| {
            let palette = theme.extended_palette();
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => palette.primary.strong.color,
                _ => palette.primary.base.color,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: palette.primary.base.text,
                border: iced::Border {
                    color: palette.primary.base.color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        })
    } else {
        b.style(|theme: &iced::Theme, status| {
            let palette = theme.extended_palette();
            let bg = match status {
                button::Status::Hovered => palette.background.strong.color,
                _ => iced::Color::TRANSPARENT,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: palette.background.base.text,
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }
        })
    }
}

// ---------------------------------------------------------------------------
// SettingsTab — which tab is currently selected
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Chats,
    Account,
    Privacy,
    Advanced,
}

impl SettingsTab {
    fn label(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Chats => "Chats",
            SettingsTab::Account => "Account",
            SettingsTab::Privacy => "Privacy",
            SettingsTab::Advanced => "Advanced",
        }
    }
}

// ---------------------------------------------------------------------------
// SettingsScreen — not Clone because it owns XmppCommands
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SettingsScreen {
    /// Currently selected settings tab
    pub active_tab: SettingsTab,
    settings: Settings,
    status_input: String,
    // M3: blocklist panel state
    blocklist: BlocklistPanel,
    // M4: account details
    account_info: AccountInfo,
    // M6: MAM fetch limit draft (string so the text_input can hold partial input)
    mam_fetch_limit_input: String,
    // M6: clear history confirmation state
    clear_history_confirm: bool,
    // M6: commands to emit to the engine (drained by the App via drain_commands)
    pending_commands: Vec<crate::xmpp::XmppCommand>,
    // M5: network settings draft inputs
    proxy_host_input: String,
    proxy_port_input: String,
    manual_srv_input: String,
    // DC-17: interactive avatar crop state (raw bytes + mime + crop params)
    crop_state: Option<crate::store::avatar_crop::CropState>,
    crop_raw: Option<(Vec<u8>, String)>,
    // MEMO: OMEMO status
    omemo_enabled: bool,
    omemo_device_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(SettingsTab),
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
    OpenAvatarPicker,
    AvatarFilePicked(Option<std::path::PathBuf>),
    AvatarSelected(Vec<u8>, String),
    // DC-17: interactive crop adjustments — pan/zoom/radius then re-crop
    // These variants are defined for the DC-17 crop UI but not yet wired to controls.
    #[allow(dead_code)]
    AvatarCropPan(f32, f32),
    #[allow(dead_code)]
    AvatarCropZoom(f32),
    #[allow(dead_code)]
    AvatarCropRadius(f32),
    AvatarCropApply,
    // K6: contact sorting preference
    SortContactsSelected(String),
    // K6: chat preferences panel
    ShowJoinLeaveToggled(bool),
    ShowTypingIndicatorsToggled(bool),
    CompactLayoutToggled(bool),
    // M3: blocklist panel messages
    Blocklist(super::blocklist::Message),
    // M6: data & storage
    MamFetchLimitChanged(String),
    MamFetchLimitConfirm,
    ClearHistoryRequest,
    ClearHistoryConfirm,
    ClearHistoryCancel,
    // AUTH-2: logout from settings screen
    Logout,
    // M7: open About modal from settings
    OpenAbout,
    // K2: open vCard editor from settings
    OpenVCardEditor,
    Back,
    // M5: network settings
    ProxyTypeSelected(String),
    ProxyHostChanged(String),
    ProxyPortChanged(String),
    ManualSrvChanged(String),
    ForceTlsToggled(bool),
    // MEMO: enable OMEMO encryption
    EnableOmemo,
    // UX-5: copy account detail to clipboard
    CopyToClipboard(String),
}

/// Actions emitted by `SettingsScreen::update()` for the parent App to handle.
pub enum Action {
    /// No parent action needed.
    None,
    /// An async task that produces further Messages for this settings screen.
    Task(Task<Message>),
    /// Navigate back to the previous screen.
    GoBack,
    /// User requested logout.
    Logout,
    /// Open the About screen.
    OpenAbout,
    /// Open the vCard editor.
    OpenVCardEditor,
    /// Enable OMEMO encryption.
    EnableOmemo,
    /// Avatar data ready for upload (data, mime_type).
    AvatarSelected(Vec<u8>, String),
    /// Clear all chat history from the database.
    ClearHistory,
}

impl SettingsScreen {
    pub fn new(settings: Settings) -> Self {
        let mam_fetch_limit_input = settings.mam_fetch_limit.to_string();
        let proxy_host_input = settings.proxy_host.clone().unwrap_or_default();
        let proxy_port_input = settings
            .proxy_port
            .map(|p| p.to_string())
            .unwrap_or_default();
        let manual_srv_input = settings.manual_srv.clone().unwrap_or_default();
        Self {
            status_input: settings.status_message.clone().unwrap_or_default(),
            blocklist: BlocklistPanel::new(vec![]),
            account_info: AccountInfo::default(),
            mam_fetch_limit_input,
            clear_history_confirm: false,
            pending_commands: vec![],
            proxy_host_input,
            proxy_port_input,
            manual_srv_input,
            crop_state: None,
            crop_raw: None,
            omemo_enabled: false,
            omemo_device_id: None,
            active_tab: SettingsTab::General,
            settings,
        }
    }

    /// Update the account info shown in the Account Details section.
    pub fn set_account_info(&mut self, info: AccountInfo) {
        self.account_info = info;
    }

    /// Called by App when OMEMO becomes active so the settings panel can show the device ID.
    pub fn set_omemo_active(&mut self, device_id: u32) {
        self.omemo_enabled = true;
        self.omemo_device_id = Some(device_id);
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Drain any XmppCommands produced by this panel (e.g. block/unblock).
    pub fn drain_commands(&mut self) -> Vec<crate::xmpp::XmppCommand> {
        std::mem::take(&mut self.pending_commands)
    }

    pub fn update(&mut self, msg: Message) -> Action {
        match msg {
            Message::TabSelected(tab) => {
                self.active_tab = tab;
                Action::None
            }
            Message::ThemeToggled => {
                self.settings.theme = match self.settings.theme {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::NotificationsToggled(enabled) => {
                self.settings.notifications_enabled = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::SoundToggled(enabled) => {
                self.settings.sound_enabled = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::FontSizeIncreased => {
                if self.settings.font_size < 20 {
                    self.settings.font_size += 1;
                    let _ = config::save(&self.settings);
                }
                Action::None
            }
            Message::FontSizeDecreased => {
                if self.settings.font_size > 12 {
                    self.settings.font_size -= 1;
                    let _ = config::save(&self.settings);
                }
                Action::None
            }
            Message::StatusInputChanged(value) => {
                self.status_input = value.clone();
                self.settings.status_message = if self.status_input.trim().is_empty() {
                    None
                } else {
                    Some(self.status_input.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::SendReceiptsToggled(enabled) => {
                self.settings.send_receipts = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::SendTypingToggled(enabled) => {
                self.settings.send_typing = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::SendReadMarkersToggled(enabled) => {
                self.settings.send_read_markers = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::MamModeSelected(mode) => {
                self.settings.mam_default_mode = Some(mode.clone());
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::SystemThemeToggled(enabled) => {
                self.settings.use_system_theme = enabled;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::TimeFormatToggled(fmt) => {
                self.settings.time_format = if fmt == "12h" {
                    crate::config::TimeFormat::TwelveHour
                } else {
                    crate::config::TimeFormat::TwentyFourHour
                };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::OpenAvatarPicker => Action::Task(Task::future(async {
                let path = rfd::AsyncFileDialog::new()
                    .set_title("Select Avatar")
                    .add_filter("Images", &["png", "jpg", "jpeg", "gif"])
                    .pick_file()
                    .await
                    .map(|f| f.path().to_path_buf());
                Message::AvatarFilePicked(path)
            })),
            Message::AvatarFilePicked(Some(path)) => {
                let mime = if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_lowercase)
                    .as_deref()
                    == Some("png")
                {
                    "image/png"
                } else if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_lowercase)
                    .as_deref()
                    == Some("gif")
                {
                    "image/gif"
                } else {
                    "image/jpeg"
                };
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let state = crate::store::avatar_crop::CropState::new(
                                img.width(),
                                img.height(),
                            );
                            self.crop_state = Some(state);
                            self.crop_raw = Some((bytes, mime.to_string()));
                            return Action::Task(Task::done(Message::AvatarCropApply));
                        } else {
                            tracing::warn!("Failed to decode avatar image");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read avatar file: {e}");
                    }
                }
                Action::None
            }
            Message::AvatarFilePicked(None) => Action::None,
            // DC-17: interactive crop — adjust state then re-apply.
            Message::AvatarCropPan(dx, dy) => {
                if let Some(ref mut state) = self.crop_state {
                    state.pan(dx, dy);
                }
                Action::Task(Task::done(Message::AvatarCropApply))
            }
            Message::AvatarCropZoom(zoom) => {
                if let Some(ref mut state) = self.crop_state {
                    state.set_zoom(zoom);
                }
                Action::Task(Task::done(Message::AvatarCropApply))
            }
            Message::AvatarCropRadius(radius) => {
                if let Some(ref mut state) = self.crop_state {
                    state.set_radius(radius);
                }
                Action::Task(Task::done(Message::AvatarCropApply))
            }
            Message::AvatarCropApply => {
                if let (Some(ref state), Some((ref bytes, ref mime))) =
                    (&self.crop_state, &self.crop_raw)
                {
                    match crate::store::avatar_crop::crop_to_avatar(bytes, state, 256) {
                        Ok(cropped) => {
                            return Action::Task(Task::done(Message::AvatarSelected(
                                cropped,
                                mime.clone(),
                            )));
                        }
                        Err(e) => {
                            tracing::warn!("Avatar crop failed: {e}");
                        }
                    }
                }
                Action::None
            }
            Message::AvatarSelected(data, mime_type) => {
                // Persist own avatar to disk cache rather than in settings JSON.
                config::save_own_avatar(&data);
                Action::AvatarSelected(data, mime_type)
            }
            Message::SortContactsSelected(sort) => {
                self.settings.contact_sort = sort.clone();
                let _ = config::save(&self.settings);
                Action::None
            }
            // K6: chat preferences
            Message::ShowJoinLeaveToggled(v) => {
                self.settings.show_join_leave = v;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::ShowTypingIndicatorsToggled(v) => {
                self.settings.show_typing_indicators = v;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::CompactLayoutToggled(v) => {
                self.settings.compact_layout = v;
                let _ = config::save(&self.settings);
                Action::None
            }

            // M3: blocklist
            Message::Blocklist(bl_msg) => {
                match self.blocklist.update(bl_msg) {
                    super::blocklist::Action::Block(jid) => {
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::BlockJid(jid));
                    }
                    super::blocklist::Action::Unblock(jid) => {
                        self.pending_commands
                            .push(crate::xmpp::XmppCommand::UnblockJid(jid));
                    }
                    super::blocklist::Action::None => {}
                }
                Action::None
            }

            // M6: MAM fetch limit
            Message::MamFetchLimitChanged(v) => {
                self.mam_fetch_limit_input = v;
                Action::None
            }
            Message::MamFetchLimitConfirm => {
                if let Ok(n) = self.mam_fetch_limit_input.trim().parse::<u32>() {
                    if n > 0 && n <= 1000 {
                        self.settings.mam_fetch_limit = n;
                        let _ = config::save(&self.settings);
                    }
                }
                Action::None
            }
            // M6: clear history confirmation flow
            Message::ClearHistoryRequest => {
                self.clear_history_confirm = true;
                Action::None
            }
            Message::ClearHistoryConfirm => {
                self.clear_history_confirm = false;
                Action::ClearHistory
            }
            Message::ClearHistoryCancel => {
                self.clear_history_confirm = false;
                Action::None
            }

            Message::Logout => Action::Logout,
            Message::OpenAbout => Action::OpenAbout,
            Message::OpenVCardEditor => Action::OpenVCardEditor,
            Message::Back => Action::GoBack,

            // M5: network settings
            Message::ProxyTypeSelected(kind) => {
                self.settings.proxy_type = if kind == "none" { None } else { Some(kind) };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::ProxyHostChanged(v) => {
                self.proxy_host_input = v.clone();
                self.settings.proxy_host = if v.trim().is_empty() {
                    None
                } else {
                    Some(v.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::ProxyPortChanged(v) => {
                self.proxy_port_input = v.clone();
                self.settings.proxy_port = v.trim().parse::<u16>().ok();
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::ManualSrvChanged(v) => {
                self.manual_srv_input = v.clone();
                self.settings.manual_srv = if v.trim().is_empty() {
                    None
                } else {
                    Some(v.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::ForceTlsToggled(v) => {
                self.settings.force_tls = v;
                let _ = config::save(&self.settings);
                Action::None
            }
            Message::EnableOmemo => Action::EnableOmemo,
            // UX-5: copy account detail to clipboard
            Message::CopyToClipboard(content) => {
                Action::Task(iced::clipboard::write::<Message>(content))
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // Top bar: back button (left), title (center).
        // Logout moved to bottom of the settings panel.
        let back_btn = button("< Back").on_press(Message::Back).padding([6, 14]);
        let top_bar = row![
            back_btn,
            horizontal_space(),
            text("Settings").size(18),
            horizontal_space(),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([8, 16]);

        // === Appearance section ===
        let is_dark = self.settings.theme == Theme::Dark;
        let theme_row = row![
            text("Dark theme").size(14),
            horizontal_space(),
            toggler(is_dark).on_toggle(|_| Message::ThemeToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let system_theme_row = row![
            text("Use system theme").size(14),
            horizontal_space(),
            toggler(self.settings.use_system_theme).on_toggle(Message::SystemThemeToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let is_24h = matches!(
            self.settings.time_format,
            crate::config::TimeFormat::TwentyFourHour
        );
        let time_format_row: Element<Message> = row![
            text("Time format").size(14),
            horizontal_space(),
            segmented_btn("24h", is_24h).on_press(Message::TimeFormatToggled("24h".into())),
            segmented_btn("12h", !is_24h).on_press(Message::TimeFormatToggled("12h".into())),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        let font_row = row![
            text(format!("Font size: {}", self.settings.font_size)).size(14),
            horizontal_space(),
            button("-")
                .on_press(Message::FontSizeDecreased)
                .padding([4, 10]),
            button("+")
                .on_press(Message::FontSizeIncreased)
                .padding([4, 10]),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let appearance_content: Element<Message> = column![
            theme_row,
            setting_divider(),
            system_theme_row,
            setting_divider(),
            time_format_row,
            setting_divider(),
            font_row,
        ]
        .spacing(10)
        .into();
        let appearance_section = settings_section_with_desc(
            "Appearance",
            Some("Theme, fonts, and display preferences"),
            appearance_content,
        );

        // === Notifications section ===
        let notif_row = row![
            text("Notifications").size(14),
            horizontal_space(),
            toggler(self.settings.notifications_enabled).on_toggle(Message::NotificationsToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let sound_row = row![
            text("Sound").size(14),
            horizontal_space(),
            toggler(self.settings.sound_enabled).on_toggle(Message::SoundToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let notifications_content: Element<Message> =
            column![notif_row, setting_divider(), sound_row]
                .spacing(10)
                .into();
        let notifications_section = settings_section_with_desc(
            "Notifications",
            Some("Alerts and sound preferences"),
            notifications_content,
        );

        // === Messages section ===
        let receipts_row = row![
            text("Send delivery receipts").size(14),
            horizontal_space(),
            toggler(self.settings.send_receipts).on_toggle(Message::SendReceiptsToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let typing_row = row![
            text("Send typing indicators").size(14),
            horizontal_space(),
            toggler(self.settings.send_typing).on_toggle(Message::SendTypingToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let read_markers_row = row![
            text("Send read markers").size(14),
            horizontal_space(),
            toggler(self.settings.send_read_markers).on_toggle(Message::SendReadMarkersToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mam_current = self
            .settings
            .mam_default_mode
            .as_deref()
            .unwrap_or("roster");
        let mam_mode_row: Element<Message> = row![
            text("MAM archive mode").size(14),
            horizontal_space(),
            segmented_btn("roster", mam_current == "roster")
                .on_press(Message::MamModeSelected("roster".into())),
            segmented_btn("always", mam_current == "always")
                .on_press(Message::MamModeSelected("always".into())),
            segmented_btn("never", mam_current == "never")
                .on_press(Message::MamModeSelected("never".into())),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        let messages_content: Element<Message> = column![
            receipts_row,
            setting_divider(),
            typing_row,
            setting_divider(),
            read_markers_row,
            setting_divider(),
            mam_mode_row,
        ]
        .spacing(10)
        .into();
        let messages_section = settings_section_with_desc(
            "Messages",
            Some("Delivery receipts and archive settings"),
            messages_content,
        );

        // === Chat Preferences section ===
        let join_leave_row = row![
            text("Show join/leave in rooms").size(14),
            horizontal_space(),
            toggler(self.settings.show_join_leave).on_toggle(Message::ShowJoinLeaveToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let typing_indicators_row = row![
            text("Show typing indicators").size(14),
            horizontal_space(),
            toggler(self.settings.show_typing_indicators)
                .on_toggle(Message::ShowTypingIndicatorsToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let compact_layout_row = row![
            text("Compact message layout").size(14),
            horizontal_space(),
            toggler(self.settings.compact_layout).on_toggle(Message::CompactLayoutToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let is_alpha = self.settings.contact_sort == "alphabetical";
        let sort_row: Element<Message> = row![
            text("Contact sort").size(14),
            horizontal_space(),
            segmented_btn("Alphabetical", is_alpha)
                .on_press(Message::SortContactsSelected("alphabetical".into())),
            segmented_btn("Recent", !is_alpha)
                .on_press(Message::SortContactsSelected("recent".into())),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();
        let chat_prefs_content: Element<Message> = column![
            join_leave_row,
            setting_divider(),
            typing_indicators_row,
            setting_divider(),
            compact_layout_row,
            setting_divider(),
            sort_row,
        ]
        .spacing(10)
        .into();
        let chat_prefs_section = settings_section_with_desc(
            "Chat Preferences",
            Some("Room events, layout, and contact sorting"),
            chat_prefs_content,
        );

        // === Profile section ===
        let avatar_row = row![
            text("Profile avatar").size(14),
            horizontal_space(),
            button(text("Upload\u{2026}").size(13))
                .on_press(Message::OpenAvatarPicker)
                .padding([4, 12]),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let status_row = row![
            text("Status message").size(14).width(Length::Fixed(130.0)),
            text_input("e.g. In a meeting", &self.status_input)
                .on_input(Message::StatusInputChanged)
                .width(Length::Fill)
                .padding([4, 8]),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let edit_profile_btn = button("Edit Profile")
            .on_press(Message::OpenVCardEditor)
            .padding([6, 14]);
        let profile_content: Element<Message> = column![
            avatar_row,
            setting_divider(),
            status_row,
            setting_divider(),
            edit_profile_btn,
        ]
        .spacing(10)
        .into();
        let profile_section =
            settings_section_with_desc("Profile", Some("Avatar and status"), profile_content);

        // === Account section ===
        let account_section = self.view_account_details();

        // === OMEMO section ===
        let omemo_section = self.view_omemo();

        // === Blocked Users section ===
        let blocklist_section = self.blocklist.view().map(Message::Blocklist);

        // === Network section ===
        let network_section = self.view_network();

        // === Data & Storage section ===
        let data_section = self.view_data_storage();

        // === Bottom actions: About + Logout ===
        let about_btn = button("About")
            .on_press(Message::OpenAbout)
            .padding([6, 14]);
        let logout_btn = button("Logout")
            .on_press(Message::Logout)
            .padding([6, 14])
            .style(|theme: &iced::Theme, status| {
                let palette = theme.extended_palette();
                let bg = match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        palette.danger.strong.color
                    }
                    _ => palette.danger.base.color,
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: palette.danger.base.text,
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });
        let bottom_row = row![horizontal_space(), about_btn, logout_btn]
            .spacing(8)
            .align_y(Alignment::Center);

        // Tab bar: one segmented button per tab
        let tabs = [
            SettingsTab::General,
            SettingsTab::Chats,
            SettingsTab::Account,
            SettingsTab::Privacy,
            SettingsTab::Advanced,
        ];
        let tab_bar = tabs.iter().fold(row![].spacing(4), |r, tab| {
            let active = *tab == self.active_tab;
            r.push(segmented_btn(tab.label(), active).on_press(Message::TabSelected(tab.clone())))
        });
        let tab_bar_row = container(tab_bar).padding([6, 16]).width(Length::Fill);

        // Build tab content: only the sections belonging to the active tab
        let mut tab_col: iced::widget::Column<Message> =
            column![].spacing(16).padding(24u16).width(500);

        match self.active_tab {
            SettingsTab::General => {
                tab_col = tab_col
                    .push(category_heading("GENERAL"))
                    .push(appearance_section)
                    .push(notifications_section);
            }
            SettingsTab::Chats => {
                tab_col = tab_col
                    .push(category_heading("COMMUNICATION"))
                    .push(messages_section)
                    .push(chat_prefs_section);
            }
            SettingsTab::Account => {
                tab_col = tab_col
                    .push(category_heading("ACCOUNT & PROFILE"))
                    .push(profile_section)
                    .push(account_section);
            }
            SettingsTab::Privacy => {
                tab_col = tab_col
                    .push(category_heading("PRIVACY & SECURITY"))
                    .push(omemo_section)
                    .push(blocklist_section);
            }
            SettingsTab::Advanced => {
                tab_col = tab_col
                    .push(category_heading("ADVANCED"))
                    .push(network_section)
                    .push(data_section);
            }
        }
        tab_col = tab_col.push(bottom_row);

        let inner = column![top_bar, tab_bar_row, scrollable(tab_col)]
            .width(500)
            .height(Length::Fill);

        container(inner)
            .center_x(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // M4: Account Details sub-view — rendered inline to avoid borrow/lifetime issues.
    // UX-5: copyable text fields with copy-to-clipboard buttons.
    fn view_account_details(&self) -> Element<'_, Message> {
        let info = &self.account_info;

        let bare_jid = info.bound_jid.split('/').next().unwrap_or("").to_string();
        let server = bare_jid.split('@').nth(1).unwrap_or("").to_string();
        let resource = info.bound_jid.split('/').nth(1).unwrap_or("").to_string();
        let status_str = if info.connected {
            "Connected"
        } else {
            "Offline"
        };
        let auth_val = if info.auth_method.is_empty() {
            "\u{2014}".to_string()
        } else {
            info.auth_method.clone()
        };
        let features_val = if info.server_features.is_empty() {
            "\u{2014}".to_string()
        } else {
            info.server_features.clone()
        };

        // UX-5 helper: build a row with label, read-only text_input, and copy button.
        macro_rules! copyable_row {
            ($label:expr, $value:expr) => {{
                let val: String = $value.clone();
                let e: Element<'_, Message> = row![
                    text($label).size(13).width(Length::Fixed(130.0)),
                    text_input("", &val)
                        .size(13)
                        .width(Length::Fill)
                        .padding([3, 5]),
                    button(text("cp").size(10))
                        .on_press(Message::CopyToClipboard(val))
                        .padding([2, 6]),
                ]
                .spacing(5)
                .align_y(Alignment::Center)
                .into();
                e
            }};
        }

        let details: Element<Message> = column![
            copyable_row!("JID", bare_jid),
            setting_divider(),
            copyable_row!("Server", server),
            setting_divider(),
            copyable_row!("Resource", resource),
            setting_divider(),
            row![
                text("Status").size(13).width(Length::Fixed(130.0)),
                text(status_str).size(13).width(Length::Fill),
            ]
            .spacing(8),
            setting_divider(),
            copyable_row!("Auth", auth_val),
            setting_divider(),
            copyable_row!("Server features", features_val),
        ]
        .spacing(8)
        .into();

        settings_section_with_desc("Account", Some("Connection and server details"), details)
    }

    // M6: Data & Storage sub-view.
    fn view_data_storage(&self) -> Element<'_, Message> {
        // MAM fetch limit
        let limit_row: Element<Message> = row![
            text("MAM fetch limit").size(14),
            horizontal_space(),
            text_input("50", &self.mam_fetch_limit_input)
                .on_input(Message::MamFetchLimitChanged)
                .on_submit(Message::MamFetchLimitConfirm)
                .width(Length::Fixed(80.0))
                .padding([4, 8]),
            button(text("Apply").size(13))
                .on_press(Message::MamFetchLimitConfirm)
                .padding([4, 10]),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into();

        // Clear chat history
        let clear_section: Element<Message> = if self.clear_history_confirm {
            row![
                text("Clear all chat history?").size(14),
                horizontal_space(),
                button(text("Confirm").size(13))
                    .on_press(Message::ClearHistoryConfirm)
                    .padding([4, 10]),
                button(text("Cancel").size(13))
                    .on_press(Message::ClearHistoryCancel)
                    .padding([4, 10]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                text("Chat history").size(14),
                horizontal_space(),
                button(text("Clear\u{2026}").size(13))
                    .on_press(Message::ClearHistoryRequest)
                    .padding([4, 10]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        };

        // Export conversations — disabled placeholder (no on_press)
        let export_row: Element<Message> = row![
            text("Export conversations").size(14),
            horizontal_space(),
            button(text("Export").size(13)).padding([4, 10]),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into();

        let content: Element<Message> = column![
            limit_row,
            setting_divider(),
            clear_section,
            setting_divider(),
            export_row,
        ]
        .spacing(10)
        .into();
        settings_section_with_desc(
            "Data & Storage",
            Some("Message archive and export options"),
            content,
        )
    }

    // MEMO: OMEMO encryption sub-view.
    fn view_omemo(&self) -> Element<'_, Message> {
        let body: Element<Message> = if self.omemo_enabled {
            let id_str = self.omemo_device_id.map_or_else(
                || "Device ID: \u{2014}".to_string(),
                |id| format!("Device ID: {id}"),
            );
            column![
                row![
                    text("Status").size(14),
                    horizontal_space(),
                    text("Enabled").size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![text(id_str).size(13)].spacing(10),
            ]
            .spacing(10)
            .into()
        } else {
            row![
                text("End-to-end encryption").size(14),
                horizontal_space(),
                button(text("Enable OMEMO").size(13))
                    .on_press(Message::EnableOmemo)
                    .padding([4, 12]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
        };
        settings_section_with_desc(
            "OMEMO Encryption",
            Some("End-to-end encryption for private conversations"),
            body,
        )
    }

    // M5: Network settings sub-view.
    fn view_network(&self) -> Element<'_, Message> {
        let is_none = self.settings.proxy_type.is_none();
        let is_socks5 = self.settings.proxy_type.as_deref() == Some("socks5");
        let is_http = self.settings.proxy_type.as_deref() == Some("http");
        let proxy_type_row: Element<Message> = row![
            text("Proxy type").size(14),
            horizontal_space(),
            segmented_btn("None", is_none).on_press(Message::ProxyTypeSelected("none".into())),
            segmented_btn("SOCKS5", is_socks5)
                .on_press(Message::ProxyTypeSelected("socks5".into())),
            segmented_btn("HTTP", is_http).on_press(Message::ProxyTypeSelected("http".into())),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        // Proxy host + port: only shown when a proxy type is selected
        let proxy_detail: Option<Element<Message>> = if self.settings.proxy_type.is_some() {
            let host_row: Element<Message> = row![
                text("Proxy host").size(14).width(Length::Fixed(130.0)),
                text_input("hostname or IP", &self.proxy_host_input)
                    .on_input(Message::ProxyHostChanged)
                    .width(Length::Fill)
                    .padding([4, 8]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into();
            let port_row: Element<Message> = row![
                text("Proxy port").size(14).width(Length::Fixed(130.0)),
                text_input("1080", &self.proxy_port_input)
                    .on_input(Message::ProxyPortChanged)
                    .width(Length::Fixed(80.0))
                    .padding([4, 8]),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into();
            Some(column![host_row, port_row].spacing(15).into())
        } else {
            None
        };

        let srv_row: Element<Message> = row![
            text("Manual SRV").size(14).width(Length::Fixed(130.0)),
            text_input("_xmpp-client._tcp\u{2026}", &self.manual_srv_input)
                .on_input(Message::ManualSrvChanged)
                .width(Length::Fill)
                .padding([4, 8]),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into();

        let tls_row = row![
            text("Force TLS").size(14),
            horizontal_space(),
            toggler(self.settings.force_tls).on_toggle(Message::ForceTlsToggled),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mut col = column![proxy_type_row].spacing(10);
        if let Some(detail) = proxy_detail {
            col = col.push(setting_divider());
            col = col.push(detail);
        }
        col = col.push(setting_divider());
        col = col.push(srv_row);
        col = col.push(setting_divider());
        col = col.push(tls_row);

        let content: Element<Message> = col.into();
        settings_section_with_desc(
            "Network",
            Some("Proxy, TLS, and server connection settings"),
            content,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mam_fetch_limit_default_is_50() {
        let settings = Settings::default();
        assert_eq!(settings.mam_fetch_limit, 50);
    }

    #[test]
    fn settings_screen_updates_mam_fetch_limit() {
        let mut screen = SettingsScreen::new(Settings::default());
        let _ = screen.update(Message::MamFetchLimitChanged("75".into()));
        let _ = screen.update(Message::MamFetchLimitConfirm);
        assert_eq!(screen.settings.mam_fetch_limit, 75);
    }

    #[test]
    fn settings_screen_rejects_zero_limit() {
        let mut screen = SettingsScreen::new(Settings::default());
        let _ = screen.update(Message::MamFetchLimitChanged("0".into()));
        let _ = screen.update(Message::MamFetchLimitConfirm);
        // Should keep default 50 because 0 is not > 0
        assert_eq!(screen.settings.mam_fetch_limit, 50);
    }

    #[test]
    fn blocklist_panel_block_unblock_roundtrip() {
        use crate::ui::blocklist::{Action as BlAction, BlocklistPanel, Message as BMsg};
        let mut panel = BlocklistPanel::new(vec!["spam@example.com".to_string()]);
        // Stage a new JID then add it
        panel.update(BMsg::NewJidChanged("troll@example.org".into()));
        let action = panel.update(BMsg::AddJid);
        assert!(matches!(action, BlAction::Block(_)));
        assert_eq!(panel.blocked.len(), 2);
        // Unblock it
        let action = panel.update(BMsg::Unblock("troll@example.org".into()));
        assert!(matches!(action, BlAction::Unblock(_)));
        assert_eq!(panel.blocked.len(), 1);
    }

    #[test]
    fn settings_screen_proxy_type_none_clears_field() {
        let mut screen = SettingsScreen::new(Settings {
            proxy_type: Some("socks5".into()),
            ..Settings::default()
        });
        let _ = screen.update(Message::ProxyTypeSelected("none".into()));
        assert!(screen.settings.proxy_type.is_none());
    }

    #[test]
    fn settings_screen_proxy_host_roundtrip() {
        let mut screen = SettingsScreen::new(Settings::default());
        let _ = screen.update(Message::ProxyHostChanged("proxy.corp.com".into()));
        assert_eq!(screen.settings.proxy_host, Some("proxy.corp.com".into()));
        assert_eq!(screen.proxy_host_input, "proxy.corp.com");
    }

    #[test]
    fn settings_screen_proxy_port_parses() {
        let mut screen = SettingsScreen::new(Settings::default());
        let _ = screen.update(Message::ProxyPortChanged("8080".into()));
        assert_eq!(screen.settings.proxy_port, Some(8080));
    }

    #[test]
    fn settings_screen_proxy_port_invalid_clears() {
        let mut screen = SettingsScreen::new(Settings {
            proxy_port: Some(1080),
            ..Settings::default()
        });
        let _ = screen.update(Message::ProxyPortChanged("not_a_port".into()));
        assert!(screen.settings.proxy_port.is_none());
    }

    #[test]
    fn settings_screen_manual_srv_roundtrip() {
        let mut screen = SettingsScreen::new(Settings::default());
        let _ = screen.update(Message::ManualSrvChanged(
            "_xmpp-client._tcp.example.com".into(),
        ));
        assert_eq!(
            screen.settings.manual_srv,
            Some("_xmpp-client._tcp.example.com".into())
        );
    }

    #[test]
    fn settings_screen_force_tls_toggle() {
        let mut screen = SettingsScreen::new(Settings::default());
        assert!(screen.settings.force_tls);
        let _ = screen.update(Message::ForceTlsToggled(false));
        assert!(!screen.settings.force_tls);
    }
}
