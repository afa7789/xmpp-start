// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use std::collections::HashMap;
use std::sync::Arc;

use iced::{Element, Subscription, Task};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

pub mod avatar;
pub mod benchmark;
pub mod chat;
pub mod conversation;
mod login;
pub mod muc_panel;
pub mod settings;
pub mod sidebar;
pub mod styling;
pub mod toast;

pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;
pub use login::LoginScreen;

use crate::config::{self, Settings, Theme};
use crate::xmpp::{self, XmppCommand, XmppEvent};
use toast::{Toast, ToastKind};

// F2: hardcoded command palette entries
const PALETTE_COMMANDS: &[&str] = &[
    "Open Settings",
    "Toggle Console",
    "Add Contact",
    "Disconnect",
];

/// Top-level application state.
pub struct App {
    screen: Screen,
    xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
    settings: Settings,
    db: Arc<SqlitePool>,
    // J1: toast notifications
    toasts: Vec<Toast>,
    next_toast_id: u64,
    // F4: reconnect state
    reconnect_attempt: u32,
    last_connect_cfg: Option<crate::xmpp::ConnectConfig>,
    // H1: avatar cache (jid → png bytes)
    avatar_cache: HashMap<String, Vec<u8>>,
    // F2: command palette
    show_palette: bool,
    palette_query: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    Chat(chat::Message),
    GoToBenchmark,
    #[allow(dead_code)]
    ToggleTheme,
    XmppReady(mpsc::Sender<XmppCommand>),
    XmppEvent(XmppEvent),
    // J1: toast messages
    ShowToast(String, ToastKind),
    DismissToast(u64),
    // B4: messages loaded from DB for a conversation
    MessagesLoaded(String, Vec<crate::store::message_repo::Message>),
    // F3: settings panel
    GoToSettings,
    Settings(settings::Message),
    GoBack,
    // F2: command palette
    TogglePalette,
    PaletteQuery(String),
    PaletteExecute(usize),
}

enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    Chat(Box<ChatScreen>),
    Settings(Box<settings::SettingsScreen>, Box<Screen>),
}

impl App {
    pub fn new_with_settings(settings: Settings, db: Arc<SqlitePool>) -> (Self, Task<Message>) {
        let password = config::load_password(&settings.last_jid).unwrap_or_default();
        let login = LoginScreen::with_saved(
            settings.last_jid.clone(),
            password,
            settings.last_server.clone(),
        );
        (
            App {
                screen: Screen::Login(login),
                xmpp_tx: None,
                settings,
                db,
                toasts: vec![],
                next_toast_id: 0,
                reconnect_attempt: 0,
                last_connect_cfg: None,
                avatar_cache: HashMap::new(),
                show_palette: false,
                palette_query: String::new(),
            },
            Task::none(),
        )
    }

    pub fn iced_theme(&self) -> iced::Theme {
        match self.settings.theme {
            Theme::Dark => iced::Theme::Dark,
            Theme::Light => iced::Theme::Light,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // F2: command palette
            Message::TogglePalette => {
                self.show_palette = !self.show_palette;
                self.palette_query.clear();
                Task::none()
            }

            Message::PaletteQuery(q) => {
                self.palette_query = q;
                Task::none()
            }

            Message::PaletteExecute(i) => {
                self.show_palette = false;
                let filtered: Vec<&str> = PALETTE_COMMANDS
                    .iter()
                    .copied()
                    .filter(|cmd| {
                        cmd.to_lowercase().contains(&self.palette_query.to_lowercase())
                    })
                    .collect();
                if let Some(&label) = filtered.get(i) {
                    match label {
                        "Open Settings" => return self.update(Message::GoToSettings),
                        "Disconnect" => {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                return Task::future(async move {
                                    let _ = tx.send(crate::xmpp::XmppCommand::Disconnect).await;
                                    Message::GoToBenchmark
                                })
                                .discard();
                            }
                        }
                        _ => {}
                    }
                }
                self.palette_query.clear();
                Task::none()
            }

            Message::ShowToast(body, kind) => {
                let id = self.next_toast_id;
                self.next_toast_id += 1;
                self.toasts.push(Toast { id, body, kind });
                // J1: auto-dismiss after 3 seconds
                Task::future(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    Message::DismissToast(id)
                })
            }

            Message::DismissToast(id) => {
                self.toasts.retain(|t| t.id != id);
                Task::none()
            }

            Message::MessagesLoaded(jid, rows) => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    let own_jid = chat.own_jid().to_string();
                    if let Some(convo) = chat.get_conversation_mut(&jid) {
                        let display: Vec<crate::ui::conversation::DisplayMessage> = rows
                            .into_iter()
                            .map(|r| crate::ui::conversation::DisplayMessage {
                                id: r.id,
                                from: r.from_jid.clone(),
                                body: r.body.unwrap_or_default(),
                                own: r.from_jid == own_jid,
                                timestamp: r.timestamp,
                                reply_preview: None,
                            })
                            .collect();
                        convo.load_history(display);
                        return convo
                            .update(crate::ui::conversation::Message::ScrollToBottom)
                            .map(move |m| Message::Chat(chat::Message::Conversation(jid.clone(), m)));
                    }
                }
                Task::none()
            }

            Message::GoToSettings => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                self.screen = Screen::Settings(
                    Box::new(settings::SettingsScreen::new(self.settings.clone())),
                    Box::new(prev),
                );
                Task::none()
            }

            Message::GoBack => {
                if let Screen::Settings(ref ss, _) = self.screen {
                    self.settings = ss.settings().clone();
                }
                if let Screen::Settings(_, prev) = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new())) {
                    self.screen = *prev;
                }
                Task::none()
            }

            Message::Settings(smsg) => {
                let go_back = matches!(smsg, settings::Message::Back);
                if let Screen::Settings(ref mut ss, _) = self.screen {
                    let _ = ss.update(smsg);
                    self.settings = ss.settings().clone();
                }
                if go_back {
                    return self.update(Message::GoBack);
                }
                Task::none()
            }

            Message::ToggleTheme => {
                self.settings.theme = match self.settings.theme {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                };
                let _ = config::save(&self.settings);
                Task::none()
            }

            Message::GoToBenchmark => {
                self.screen = Screen::Benchmark(BenchmarkScreen::new());
                Task::none()
            }

            Message::Login(msg) => {
                if matches!(msg, login::Message::Connect) {
                    if let Screen::Login(ref mut login) = self.screen {
                        let cfg = login.connect_config();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            self.settings.last_jid = cfg.jid.clone();
                            self.settings.last_server = cfg.server.clone();
                            let _ = config::save(&self.settings);
                            self.last_connect_cfg = Some(cfg.clone());
                            self.reconnect_attempt = 0;
                            return Task::future(async move {
                                let _ = tx.send(XmppCommand::Connect(cfg)).await;
                                Message::Login(login::Message::Connecting)
                            });
                        }
                    }
                }

                if matches!(msg, login::Message::GoToBenchmark) {
                    self.screen = Screen::Benchmark(BenchmarkScreen::new());
                    return Task::none();
                }

                if let Screen::Login(login) = &mut self.screen {
                    login.update(msg).map(Message::Login)
                } else {
                    Task::none()
                }
            }

            Message::Benchmark(msg) => {
                if let Screen::Benchmark(bench) = &mut self.screen {
                    let go_back = matches!(msg, benchmark::Message::Back);
                    let task = bench.update(msg).map(Message::Benchmark);
                    if go_back {
                        self.screen = Screen::Login(LoginScreen::new());
                    }
                    task
                } else {
                    Task::none()
                }
            }

            Message::Chat(msg) => {
                // F3: intercept OpenSettings before delegating
                if let chat::Message::OpenSettings = msg {
                    return self.update(Message::GoToSettings);
                }
                // J3: intercept ToggleMute to persist muted_jids
                if let chat::Message::ToggleMute(ref jid) = msg {
                    let jid_str = jid.clone();
                    if self.settings.muted_jids.contains(&jid_str) {
                        self.settings.muted_jids.remove(&jid_str);
                    } else {
                        self.settings.muted_jids.insert(jid_str);
                    }
                    let _ = config::save(&self.settings);
                }
                if let Screen::Chat(ref mut chat) = self.screen {
                    // B4+B6: if SelectContact, fire history load and mark-read
                    let selected_jid: Option<String> =
                        if let chat::Message::Sidebar(
                            crate::ui::sidebar::Message::SelectContact(ref jid)
                        ) = msg {
                            Some(jid.clone())
                        } else {
                            None
                        };
                    let history_task: Task<Message> = if let Some(ref jid) = selected_jid {
                        let jid = jid.clone();
                        let pool = self.db.clone();
                        Task::future(async move {
                            let rows = crate::store::message_repo::find_by_conversation(&pool, &jid, 50)
                                .await
                                .unwrap_or_default();
                            Message::MessagesLoaded(jid, rows)
                        })
                    } else {
                        Task::none()
                    };
                    let mark_read_task: Task<Message> = if let Some(ref jid) = selected_jid {
                        if let Some(last_id) = chat.last_message_id(jid) {
                            let jid = jid.clone();
                            let pool = self.db.clone();
                            Task::future(async move {
                                let _ = crate::store::conversation_repo::mark_read(&pool, &jid, &last_id).await;
                                Message::GoToBenchmark
                            }).discard()
                        } else {
                            Task::none()
                        }
                    } else {
                        Task::none()
                    };
                    let task = chat.update(msg).map(Message::Chat);
                    let cmds = chat.drain_commands();
                    if !cmds.is_empty() {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            return Task::batch([
                                history_task,
                                mark_read_task,
                                task,
                                Task::future(async move {
                                    for cmd in cmds {
                                        let _ = tx.send(cmd).await;
                                    }
                                    Message::GoToBenchmark
                                })
                                .discard(),
                            ]);
                        }
                    }
                    Task::batch([history_task, mark_read_task, task])
                } else {
                    Task::none()
                }
            }

            Message::XmppReady(tx) => {
                tracing::debug!("xmpp command channel ready");
                self.xmpp_tx = Some(tx);
                Task::none()
            }

            Message::XmppEvent(event) => {
                match event {
                    XmppEvent::Connected { ref bound_jid } => {
                        tracing::info!("XMPP: online as {bound_jid}");
                        if let Screen::Login(ref login) = self.screen {
                            let cfg = login.connect_config();
                            if !cfg.password.is_empty() {
                                let _ = config::save_password(&cfg.jid, &cfg.password);
                            }
                        }
                        self.screen = Screen::Chat(Box::new(ChatScreen::new(bound_jid.clone())));
                        return self.update(Message::ShowToast(
                            format!("Connected as {}", bound_jid),
                            ToastKind::Success,
                        ));
                    }
                    XmppEvent::Disconnected { ref reason } => {
                        tracing::warn!("XMPP: disconnected — {reason}");
                        if let Screen::Login(ref mut login) = self.screen {
                            login.on_error(reason.clone());
                        }
                        if matches!(self.screen, Screen::Chat(_)) {
                            self.screen = Screen::Login(LoginScreen::new());
                        }
                        // J1: show disconnect toast
                        let msg = Message::ShowToast(
                            format!("Disconnected: {}", reason),
                            ToastKind::Error,
                        );
                        return self.update(msg);
                    }
                    XmppEvent::Reconnecting { attempt } => {
                        tracing::info!("XMPP: reconnecting (attempt {attempt})");
                        self.reconnect_attempt = attempt;
                        let delay_secs = 2u64.pow(attempt.min(6));
                        if let (Some(cfg), Some(tx)) = (self.last_connect_cfg.clone(), self.xmpp_tx.clone()) {
                            return Task::future(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                                let _ = tx.send(XmppCommand::Connect(cfg)).await;
                                Message::XmppEvent(XmppEvent::Reconnecting { attempt: 0 })
                            }).discard();
                        }
                    }
                    XmppEvent::RosterReceived(ref contacts) => {
                        tracing::info!("XMPP: roster ({} contacts)", contacts.len());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.set_roster(contacts.clone());
                        }
                        let toast = self.update(Message::ShowToast(
                            format!("{} contacts loaded", contacts.len()),
                            ToastKind::Info,
                        ));
                        // A3: persist roster to DB
                        let pool = self.db.clone();
                        let contacts = contacts.clone();
                        return Task::batch([toast, Task::future(async move {
                            for c in &contacts {
                                let _ = crate::store::roster_repo::upsert(
                                    &pool,
                                    &crate::store::roster_repo::RosterContact {
                                        jid: c.jid.clone(),
                                        name: c.name.clone(),
                                        subscription: c.subscription.clone(),
                                        groups: None,
                                    },
                                )
                                .await;
                            }
                            Message::GoToBenchmark
                        })
                        .discard()]);
                    }
                    XmppEvent::MessageReceived(ref msg) => {
                        tracing::info!("XMPP: message from {}", msg.from);
                        let bare_from = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
                        // A5: desktop notification — only for background conversations (J3: skip muted JIDs)
                        let is_active = if let Screen::Chat(ref chat) = self.screen {
                            chat.active_jid() == Some(bare_from.as_str())
                        } else {
                            false
                        };
                        // A5: fire desktop notification for background conversations (J3: skip muted)
                        let notif_task: Task<Message> = if self.settings.notifications_enabled
                            && !is_active
                            && !self.settings.muted_jids.contains(&bare_from)
                        {
                            let notif_from = bare_from.clone();
                            let notif_body: String = msg.body.chars().take(100).collect();
                            Task::future(async move {
                                let _ = crate::notifications::notify_message(&notif_from, &notif_body);
                                Message::GoToBenchmark
                            })
                        } else {
                            Task::none()
                        };
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_received(msg.clone());
                        }
                        // A2: persist message + conversation to DB
                        let pool = self.db.clone();
                        let from_jid = msg.from.clone();
                        let bare_jid = from_jid.split('/').next().unwrap_or(&from_jid).to_string();
                        let msg_id = msg.id.clone();
                        let body = msg.body.clone();
                        let ts = chrono::Utc::now().timestamp_millis();
                        let db_task = Task::future(async move {
                            let _ = crate::store::conversation_repo::upsert(&pool, &bare_jid).await;
                            let _ = crate::store::message_repo::insert(
                                &pool,
                                &crate::store::message_repo::Message {
                                    id: msg_id,
                                    conversation_jid: bare_jid,
                                    from_jid,
                                    body: Some(body),
                                    timestamp: ts,
                                    stanza_id: None,
                                    origin_id: None,
                                    state: "received".into(),
                                    edited_body: None,
                                    retracted: 0,
                                },
                            )
                            .await;
                            Message::GoToBenchmark
                        })
                        .discard();
                        return Task::batch([notif_task, db_task]);
                    }
                    XmppEvent::PresenceUpdated { ref jid, available } => {
                        tracing::debug!("XMPP: presence {jid} available={available}");
                        // A4: forward to sidebar
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_presence(jid, available);
                        }
                    }
                    XmppEvent::PeerTyping { ref jid, composing } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let _ = chat.update(chat::Message::PeerTyping(jid.clone(), composing));
                        }
                    }
                    XmppEvent::AvatarReceived { ref jid, ref png_bytes } => {
                        tracing::debug!("H1: avatar received for {jid} ({} bytes)", png_bytes.len());
                        self.avatar_cache.insert(jid.clone(), png_bytes.clone());
                    }
                    XmppEvent::CatchupFinished {
                        ref conversation_jid,
                        fetched,
                    } => {
                        tracing::info!(
                            "XMPP: MAM catchup complete for {conversation_jid} ({fetched} messages)"
                        );
                    }
                    XmppEvent::UploadSlotReceived { ref put_url, ref get_url, .. } => {
                        tracing::info!("E4: upload slot received put={put_url} get={get_url}");
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        use iced::widget::{column, container, row, stack, text, button, text_input, scrollable, Space};
        use iced::{Alignment, Length, Color};

        let screen_view: Element<Message> = match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Benchmark(bench) => bench.view().map(Message::Benchmark),
            Screen::Chat(chat) => chat.view().map(Message::Chat),
            Screen::Settings(ss, _) => ss.view().map(Message::Settings),
        };

        // F2: build layers list; palette overlay added when visible
        let mut layers: Vec<Element<Message>> = vec![screen_view];

        // F2: palette overlay
        if self.show_palette {
            let filtered: Vec<(usize, &str)> = PALETTE_COMMANDS
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, cmd)| {
                    cmd.to_lowercase().contains(&self.palette_query.to_lowercase())
                })
                .collect();

            let input = text_input("Search commands...", &self.palette_query)
                .id(iced::widget::text_input::Id::new("palette_input"))
                .on_input(Message::PaletteQuery)
                .on_submit(if filtered.is_empty() {
                    Message::TogglePalette
                } else {
                    Message::PaletteExecute(0)
                })
                .padding(10)
                .size(16);

            let cmd_buttons: Vec<Element<Message>> = filtered
                .iter()
                .map(|(i, label)| {
                    button(text(*label).size(14))
                        .on_press(Message::PaletteExecute(*i))
                        .width(Length::Fill)
                        .padding([8, 12])
                        .into()
                })
                .collect();

            let cmd_list = cmd_buttons
                .into_iter()
                .fold(column![].spacing(2), iced::widget::Column::push);

            let palette_box = container(
                column![input, scrollable(cmd_list).height(300)].spacing(8),
            )
            .width(480)
            .padding(16)
            .style(|theme: &iced::Theme| {
                let palette = theme.extended_palette();
                iced::widget::container::Style {
                    background: Some(iced::Background::Color(
                        palette.background.base.color,
                    )),
                    border: iced::Border {
                        color: palette.primary.base.color,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                        offset: iced::Vector::new(0.0, 4.0),
                        blur_radius: 16.0,
                    },
                    ..Default::default()
                }
            });

            // Dark semi-transparent backdrop + centered palette
            let backdrop = container(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.5,
                    ))),
                    ..Default::default()
                });

            let overlay = container(
                column![
                    Space::new(Length::Fill, Length::Fixed(80.0)),
                    row![
                        Space::new(Length::Fill, Length::Shrink),
                        palette_box,
                        Space::new(Length::Fill, Length::Shrink),
                    ]
                    .width(Length::Fill),
                ]
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill);

            let palette_layer: Element<Message> = stack![backdrop, overlay].into();
            layers.push(palette_layer);
        }

        // J1: build toast overlay
        if self.toasts.is_empty() {
            // Re-stack layers built so far
            return if layers.len() == 1 {
                layers.remove(0)
            } else {
                let base = layers.remove(0);
                let top = layers.remove(0);
                stack![base, top].into()
            };
        }

        // J1: build toast column at bottom-right
        let toast_items: Vec<Element<Message>> = self
            .toasts
            .iter()
            .map(|t| {
                let bg = match t.kind {
                    ToastKind::Error => Color::from_rgb(0.8, 0.2, 0.2),
                    ToastKind::Success => Color::from_rgb(0.2, 0.7, 0.3),
                    ToastKind::Info => Color::from_rgb(0.2, 0.4, 0.8),
                };
                let dismiss_btn = button(text("x").size(10))
                    .on_press(Message::DismissToast(t.id))
                    .padding([2, 4]);
                let toast_row = row![
                    text(t.body.clone()).size(12).color(Color::WHITE).width(Length::Fill),
                    dismiss_btn,
                ]
                .spacing(4)
                .align_y(Alignment::Center);
                container(toast_row)
                    .padding([6, 10])
                    .width(280)
                    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(bg)),
                        ..Default::default()
                    })
                    .into()
            })
            .collect();

        let toast_col = toast_items
            .into_iter()
            .fold(column![].spacing(4), iced::widget::Column::push);

        let toast_overlay = container(toast_col)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(12);

        // Build final stack: screen (+ optional palette) + toast
        let base: Element<Message> = if layers.len() == 1 {
            layers.remove(0)
        } else {
            let b = layers.remove(0);
            let t = layers.remove(0);
            stack![b, t].into()
        };
        stack![base, toast_overlay].into()
    }

    pub fn subscription() -> Subscription<Message> {
        let xmpp_sub = xmpp::subscription::xmpp_subscription();
        // F2: keyboard shortcut — Cmd+K / Ctrl+K to toggle palette, Escape to close
        let kb_sub = iced::keyboard::on_key_press(|key, modifiers| {
            use iced::keyboard::Key;
            if modifiers.command() {
                if key == Key::Character("k".into()) {
                    return Some(Message::TogglePalette);
                }
            }
            if key == Key::Named(iced::keyboard::key::Named::Escape) {
                return Some(Message::TogglePalette);
            }
            None
        });
        Subscription::batch([xmpp_sub, kb_sub])
    }
}
