// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use iced::{Element, Subscription, Task};
use tokio::sync::mpsc;

mod login;
pub mod benchmark;
pub mod chat;
pub mod conversation;
pub mod muc_panel;
pub mod sidebar;
pub mod styling;

pub use login::LoginScreen;
pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;

use crate::config::{self, Settings, Theme};
use crate::xmpp::{self, XmppCommand, XmppEvent};

/// Top-level application state.
pub struct App {
    screen: Screen,
    xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
    settings: Settings,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    Chat(chat::Message),
    GoToBenchmark,
    ToggleTheme,
    XmppReady(mpsc::Sender<XmppCommand>),
    XmppEvent(XmppEvent),
}

enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    Chat(ChatScreen),
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        Self::new_with_settings(Settings::default())
    }

    pub fn new_with_settings(settings: Settings) -> (Self, Task<Message>) {
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
                        // Save password to keychain on first successful login.
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
                    }
                    XmppEvent::Reconnecting { attempt } => {
                        tracing::info!("XMPP: reconnecting (attempt {attempt})");
                    }
                    XmppEvent::RosterReceived(ref contacts) => {
                        tracing::info!("XMPP: roster ({} contacts)", contacts.len());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.set_roster(contacts.clone());
                        }
                    }
                    XmppEvent::MessageReceived(ref msg) => {
                        tracing::info!("XMPP: message from {}", msg.from);
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_received(msg.clone());
                        }
                    }
                    XmppEvent::PresenceUpdated { ref jid, available } => {
                        tracing::debug!("XMPP: presence {jid} available={available}");
                    }
                    XmppEvent::CatchupFinished { ref conversation_jid, fetched } => {
                        tracing::info!(
                            "XMPP: MAM catchup complete for {conversation_jid} ({fetched} messages)"
                        );
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Benchmark(bench) => bench.view().map(Message::Benchmark),
            Screen::Chat(chat) => chat.view().map(Message::Chat),
        }
    }

    pub fn subscription() -> Subscription<Message> {
        xmpp::subscription::xmpp_subscription()
    }
}
