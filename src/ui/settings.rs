// F3: Settings panel screen

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input, toggler},
    Alignment, Element, Length, Task,
};

use crate::config::{self, Settings, Theme};
use super::account_details::AccountInfo;
use super::blocklist::{BlocklistCommand, BlocklistPanel};

// ---------------------------------------------------------------------------
// SettingsScreen — not Clone because it owns XmppCommands
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SettingsScreen {
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
    OpenAvatarPicker,
    AvatarFilePicked(Option<std::path::PathBuf>),
    AvatarSelected(Vec<u8>, String),
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
            settings,
        }
    }

    /// Replace the block list shown in the panel.
    #[allow(dead_code)]
    pub fn set_blocked_jids(&mut self, jids: Vec<String>) {
        self.blocklist = BlocklistPanel::new(jids);
    }

    /// Update the account info shown in the Account Details section.
    #[allow(dead_code)]
    pub fn set_account_info(&mut self, info: AccountInfo) {
        self.account_info = info;
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Drain any XmppCommands produced by this panel (e.g. block/unblock).
    pub fn drain_commands(&mut self) -> Vec<crate::xmpp::XmppCommand> {
        std::mem::take(&mut self.pending_commands)
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
            Message::OpenAvatarPicker => Task::future(async {
                let path = rfd::AsyncFileDialog::new()
                    .set_title("Select Avatar")
                    .add_filter("Images", &["png", "jpg", "jpeg", "gif"])
                    .pick_file()
                    .await
                    .map(|f| f.path().to_path_buf());
                Message::AvatarFilePicked(path)
            }),
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
                            match crate::store::avatar_crop::crop_to_avatar(&bytes, &state, 256) {
                                Ok(cropped) => {
                                    return Task::done(Message::AvatarSelected(
                                        cropped,
                                        mime.to_string(),
                                    ));
                                }
                                Err(e) => {
                                    tracing::warn!("Avatar crop failed: {e}");
                                }
                            }
                        } else {
                            tracing::warn!("Failed to decode avatar image");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read avatar file: {e}");
                    }
                }
                Task::none()
            }
            Message::AvatarFilePicked(None) => Task::none(),
            Message::AvatarSelected(data, _mime_type) => {
                self.settings.avatar_data = Some(data);
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::SortContactsSelected(sort) => {
                self.settings.contact_sort = sort.clone();
                let _ = config::save(&self.settings);
                Task::none()
            }
            // K6: chat preferences
            Message::ShowJoinLeaveToggled(v) => {
                self.settings.show_join_leave = v;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::ShowTypingIndicatorsToggled(v) => {
                self.settings.show_typing_indicators = v;
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::CompactLayoutToggled(v) => {
                self.settings.compact_layout = v;
                let _ = config::save(&self.settings);
                Task::none()
            }

            // M3: blocklist
            Message::Blocklist(bl_msg) => {
                if let Some(cmd) = self.blocklist.update(bl_msg) {
                    match cmd {
                        BlocklistCommand::Block(jid) => {
                            self.pending_commands
                                .push(crate::xmpp::XmppCommand::BlockJid(jid));
                        }
                        BlocklistCommand::Unblock(jid) => {
                            self.pending_commands
                                .push(crate::xmpp::XmppCommand::UnblockJid(jid));
                        }
                    }
                }
                Task::none()
            }

            // M6: MAM fetch limit
            Message::MamFetchLimitChanged(v) => {
                self.mam_fetch_limit_input = v;
                Task::none()
            }
            Message::MamFetchLimitConfirm => {
                if let Ok(n) = self.mam_fetch_limit_input.trim().parse::<u32>() {
                    if n > 0 && n <= 1000 {
                        self.settings.mam_fetch_limit = n;
                        let _ = config::save(&self.settings);
                    }
                }
                Task::none()
            }
            // M6: clear history confirmation flow
            Message::ClearHistoryRequest => {
                self.clear_history_confirm = true;
                Task::none()
            }
            Message::ClearHistoryConfirm => {
                self.clear_history_confirm = false;
                Task::none()
            }
            Message::ClearHistoryCancel => {
                self.clear_history_confirm = false;
                Task::none()
            }

            // AUTH-2: logout is handled by App::update intercepting this message.
            Message::Logout => Task::none(),
            // M7: OpenAbout is handled by App::update intercepting this message.
            Message::OpenAbout => Task::none(),
            // K2: OpenVCardEditor is handled by App::update intercepting this message.
            Message::OpenVCardEditor => Task::none(),
            Message::Back => Task::none(),

            // M5: network settings
            Message::ProxyTypeSelected(kind) => {
                self.settings.proxy_type = if kind == "none" {
                    None
                } else {
                    Some(kind)
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::ProxyHostChanged(v) => {
                self.proxy_host_input = v.clone();
                self.settings.proxy_host = if v.trim().is_empty() {
                    None
                } else {
                    Some(v.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::ProxyPortChanged(v) => {
                self.proxy_port_input = v.clone();
                self.settings.proxy_port = v.trim().parse::<u16>().ok();
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::ManualSrvChanged(v) => {
                self.manual_srv_input = v.clone();
                self.settings.manual_srv = if v.trim().is_empty() {
                    None
                } else {
                    Some(v.trim().to_string())
                };
                let _ = config::save(&self.settings);
                Task::none()
            }
            Message::ForceTlsToggled(v) => {
                self.settings.force_tls = v;
                let _ = config::save(&self.settings);
                Task::none()
            }
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

        let avatar_row = row![
            text("Profile Avatar").size(14).width(Length::Fill),
            button(text("Upload…").size(13))
                .on_press(Message::OpenAvatarPicker)
                .padding([4, 12]),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // K6: Chat Preferences section
        let chat_prefs_title = text("Chat Preferences").size(15);
        let join_leave_row = row![
            text("Show join/leave in rooms").size(14).width(Length::Fill),
            toggler(self.settings.show_join_leave).on_toggle(Message::ShowJoinLeaveToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let typing_indicators_row = row![
            text("Show typing indicators").size(14).width(Length::Fill),
            toggler(self.settings.show_typing_indicators)
                .on_toggle(Message::ShowTypingIndicatorsToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let compact_layout_row = row![
            text("Compact message layout").size(14).width(Length::Fill),
            toggler(self.settings.compact_layout).on_toggle(Message::CompactLayoutToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let contact_sort_label = if self.settings.contact_sort == "alphabetical" {
            "Alphabetical"
        } else {
            "Recent activity"
        };
        let sort_row: Element<Message> = row![
            text("Contact sort:").size(14).width(Length::Fill),
            button("Alphabetical")
                .on_press(Message::SortContactsSelected("alphabetical".into()))
                .padding([4, 8]),
            button("Recent")
                .on_press(Message::SortContactsSelected("recent".into()))
                .padding([4, 8]),
            text(contact_sort_label).size(13),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();
        let chat_prefs_section = column![
            chat_prefs_title,
            join_leave_row,
            typing_indicators_row,
            compact_layout_row,
            sort_row,
        ]
        .spacing(8);

        // M3: Blocked Users section
        let blocklist_section = self.blocklist.view().map(Message::Blocklist);

        // M4: Account Details section — rendered inline to avoid borrow/lifetime issues
        let account_section = self.view_account_details();

        // M6: Data & Storage section
        let data_section = self.view_data_storage();

        // M5: Network section
        let network_section = self.view_network();

        let back_btn = button("Back").on_press(Message::Back).padding([6, 14]);
        let edit_profile_btn = button("Edit Profile")
            .on_press(Message::OpenVCardEditor)
            .padding([6, 14]);
        let about_btn = button("About").on_press(Message::OpenAbout).padding([6, 14]);
        let logout_btn = button("Logout").on_press(Message::Logout).padding([6, 14]);
        let bottom_row = row![
            back_btn,
            iced::widget::Space::with_width(Length::Fill),
            edit_profile_btn,
            about_btn,
            logout_btn,
        ]
        .spacing(8)
        .align_y(Alignment::Center);

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
            avatar_row,
            chat_prefs_section,
            blocklist_section,
            account_section,
            network_section,
            data_section,
            bottom_row,
        ]
        .spacing(16)
        .padding(24)
        .width(420);

        container(scrollable(content))
            .center_x(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // M4: Account Details sub-view — rendered inline to avoid borrow/lifetime issues.
    fn view_account_details(&self) -> Element<'_, Message> {
        let info = &self.account_info;
        let header = text("Account Details").size(16);

        let bare_jid = info.bound_jid.split('/').next().unwrap_or("");
        let server = bare_jid.split('@').nth(1).unwrap_or("");
        let resource = info.bound_jid.split('/').nth(1).unwrap_or("");
        let status_str = if info.connected { "Connected" } else { "Offline" };

        let details = column![
            header,
            row![
                text("JID").size(13).width(Length::Fixed(140.0)),
                text(bare_jid.to_string()).size(13).width(Length::Fill),
            ]
            .spacing(8),
            row![
                text("Server").size(13).width(Length::Fixed(140.0)),
                text(server.to_string()).size(13).width(Length::Fill),
            ]
            .spacing(8),
            row![
                text("Resource").size(13).width(Length::Fixed(140.0)),
                text(resource.to_string()).size(13).width(Length::Fill),
            ]
            .spacing(8),
            row![
                text("Status").size(13).width(Length::Fixed(140.0)),
                text(status_str.to_string()).size(13).width(Length::Fill),
            ]
            .spacing(8),
            row![
                text("Auth").size(13).width(Length::Fixed(140.0)),
                text(if info.auth_method.is_empty() {
                    "—".to_string()
                } else {
                    info.auth_method.clone()
                })
                .size(13)
                .width(Length::Fill),
            ]
            .spacing(8),
            row![
                text("Server features").size(13).width(Length::Fixed(140.0)),
                text(if info.server_features.is_empty() {
                    "—".to_string()
                } else {
                    info.server_features.clone()
                })
                .size(13)
                .width(Length::Fill),
            ]
            .spacing(8),
        ]
        .spacing(6);

        container(details).padding(0).into()
    }

    // M6: Data & Storage sub-view.
    fn view_data_storage(&self) -> Element<'_, Message> {
        let header = text("Data & Storage").size(16);

        // MAM fetch limit
        let limit_row: Element<Message> = row![
            text("MAM fetch limit:").size(14).width(Length::Fill),
            text_input("50", &self.mam_fetch_limit_input)
                .on_input(Message::MamFetchLimitChanged)
                .on_submit(Message::MamFetchLimitConfirm)
                .width(Length::Fixed(70.0))
                .padding([4, 8]),
            button(text("Apply").size(13))
                .on_press(Message::MamFetchLimitConfirm)
                .padding([4, 10]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        // Clear chat history
        let clear_section: Element<Message> = if self.clear_history_confirm {
            row![
                text("Clear all chat history?").size(14).width(Length::Fill),
                button(text("Confirm").size(13))
                    .on_press(Message::ClearHistoryConfirm)
                    .padding([4, 10]),
                button(text("Cancel").size(13))
                    .on_press(Message::ClearHistoryCancel)
                    .padding([4, 10]),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                text("Chat history").size(14).width(Length::Fill),
                button(text("Clear…").size(13))
                    .on_press(Message::ClearHistoryRequest)
                    .padding([4, 10]),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        };

        // Export conversations — disabled placeholder (no on_press)
        let export_row: Element<Message> = row![
            text("Export conversations").size(14).width(Length::Fill),
            button(text("Export").size(13)).padding([4, 10]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        column![header, limit_row, clear_section, export_row]
            .spacing(8)
            .into()
    }

    // M5: Network settings sub-view.
    fn view_network(&self) -> Element<'_, Message> {
        let header = text("Network").size(16);

        let proxy_type_label = match self.settings.proxy_type.as_deref() {
            Some("socks5") => "SOCKS5",
            Some("http") => "HTTP",
            _ => "None",
        };
        let proxy_type_row: Element<Message> = row![
            text("Proxy type:").size(14).width(Length::Fill),
            button("None")
                .on_press(Message::ProxyTypeSelected("none".into()))
                .padding([4, 8]),
            button("SOCKS5")
                .on_press(Message::ProxyTypeSelected("socks5".into()))
                .padding([4, 8]),
            button("HTTP")
                .on_press(Message::ProxyTypeSelected("http".into()))
                .padding([4, 8]),
            text(proxy_type_label).size(13),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into();

        // Proxy host + port: only shown when a proxy type is selected
        let proxy_detail: Option<Element<Message>> =
            if self.settings.proxy_type.is_some() {
                let host_row: Element<Message> = row![
                    text("Proxy host:").size(14).width(Length::Fixed(120.0)),
                    text_input("hostname or IP", &self.proxy_host_input)
                        .on_input(Message::ProxyHostChanged)
                        .width(Length::Fill)
                        .padding([4, 8]),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into();
                let port_row: Element<Message> = row![
                    text("Proxy port:").size(14).width(Length::Fixed(120.0)),
                    text_input("1080", &self.proxy_port_input)
                        .on_input(Message::ProxyPortChanged)
                        .width(Length::Fixed(80.0))
                        .padding([4, 8]),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into();
                Some(
                    column![host_row, port_row]
                        .spacing(8)
                        .into(),
                )
            } else {
                None
            };

        let srv_row: Element<Message> = row![
            text("Manual SRV:").size(14).width(Length::Fixed(120.0)),
            text_input("_xmpp-client._tcp…", &self.manual_srv_input)
                .on_input(Message::ManualSrvChanged)
                .width(Length::Fill)
                .padding([4, 8]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into();

        let tls_row = row![
            text("Force TLS").size(14).width(Length::Fill),
            toggler(self.settings.force_tls).on_toggle(Message::ForceTlsToggled),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut col = column![header, proxy_type_row].spacing(8);
        if let Some(detail) = proxy_detail {
            col = col.push(detail);
        }
        col = col.push(srv_row);
        col = col.push(tls_row);
        col.into()
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
        use crate::ui::blocklist::{BlocklistPanel, Message as BMsg};
        let mut panel = BlocklistPanel::new(vec!["spam@example.com".to_string()]);
        // Stage a new JID then add it
        panel.update(BMsg::NewJidChanged("troll@example.org".into()));
        let cmd = panel.update(BMsg::AddJid);
        assert!(matches!(cmd, Some(BlocklistCommand::Block(_))));
        assert_eq!(panel.blocked.len(), 2);
        // Unblock it
        let cmd = panel.update(BMsg::Unblock("troll@example.org".into()));
        assert!(matches!(cmd, Some(BlocklistCommand::Unblock(_))));
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
        let _ =
            screen.update(Message::ManualSrvChanged("_xmpp-client._tcp.example.com".into()));
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
