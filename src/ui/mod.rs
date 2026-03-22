// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

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
pub mod sidebar;
pub mod styling;
pub mod toast;

pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;
pub use login::LoginScreen;

use crate::config::{self, Settings, Theme};
use crate::xmpp::{self, XmppCommand, XmppEvent};
use toast::{Toast, ToastKind};

/// Top-level application state.
pub struct App {
    screen: Screen,
    xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
    settings: Settings,
    db: Arc<SqlitePool>,
    // J1: toast notifications
    toasts: Vec<Toast>,
    next_toast_id: u64,
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
}

enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    Chat(ChatScreen),
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
                if let Screen::Chat(ref mut chat) = self.screen {
                    let task = chat.update(msg).map(Message::Chat);
                    let cmds = chat.drain_commands();
                    if !cmds.is_empty() {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            return Task::batch([
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
                    task
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
                        self.screen = Screen::Chat(ChatScreen::new(bound_jid.clone()));
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
                    }
                    XmppEvent::RosterReceived(ref contacts) => {
                        tracing::info!("XMPP: roster ({} contacts)", contacts.len());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.set_roster(contacts.clone());
                        }
                        // A3: persist roster to DB
                        let pool = self.db.clone();
                        let contacts = contacts.clone();
                        return Task::future(async move {
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
                        .discard();
                    }
                    XmppEvent::MessageReceived(ref msg) => {
                        tracing::info!("XMPP: message from {}", msg.from);
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_received(msg.clone());
                        }
                        // A5: desktop notification
                        if self.settings.notifications_enabled {
                            let from = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
                            let _ = crate::notifications::notify_message(&from, &msg.body);
                        }
                        // A2: persist message + conversation to DB
                        let pool = self.db.clone();
                        let from_jid = msg.from.clone();
                        let bare_jid = from_jid.split('/').next().unwrap_or(&from_jid).to_string();
                        let msg_id = msg.id.clone();
                        let body = msg.body.clone();
                        let ts = chrono::Utc::now().timestamp_millis();
                        return Task::future(async move {
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
                            chat.on_peer_typing(jid, composing);
                        }
                    }
                    XmppEvent::CatchupFinished {
                        ref conversation_jid,
                        fetched,
                    } => {
                        tracing::info!(
                            "XMPP: MAM catchup complete for {conversation_jid} ({fetched} messages)"
                        );
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        use iced::widget::{column, container, row, stack, text, button};
        use iced::{Alignment, Length, Color};

        let screen_view: Element<Message> = match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Benchmark(bench) => bench.view().map(Message::Benchmark),
            Screen::Chat(chat) => chat.view().map(Message::Chat),
        };

        if self.toasts.is_empty() {
            return screen_view;
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

        stack![screen_view, toast_overlay].into()
    }

    pub fn subscription() -> Subscription<Message> {
        xmpp::subscription::xmpp_subscription()
    }
}
