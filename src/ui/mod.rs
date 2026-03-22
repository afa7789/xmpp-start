// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use iced::{Element, Subscription, Task};
use tokio::sync::mpsc;

mod login;
pub mod benchmark;

pub use login::LoginScreen;
pub use benchmark::BenchmarkScreen;

use crate::xmpp::{self, XmppCommand, XmppEvent};

/// Top-level application state.
/// iced 0.13 uses standalone update/view functions instead of Application trait.
pub struct App {
    screen: Screen,
    /// Command channel to the XMPP engine (available after XmppReady).
    xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    GoToBenchmark,
    /// Subscription is ready; stores the command sender.
    XmppReady(mpsc::Sender<XmppCommand>),
    XmppEvent(XmppEvent),
}

// mpsc::Sender doesn't implement Debug by default; we just need it for Clone.
// XmppCommand also lacks Clone but Sender<T> only requires T: Send.
// The derive requires Clone on Message — delegate to a newtype isn't needed
// because Sender<T: Send> implements Clone unconditionally.

enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    // TODO: Chat(ChatScreen)   — Task P2.3
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        (
            App {
                screen: Screen::Login(LoginScreen::new()),
                xmpp_tx: None,
            },
            Task::none(),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoToBenchmark => {
                self.screen = Screen::Benchmark(BenchmarkScreen::new());
                Task::none()
            }

            Message::Login(msg) => {
                // Intercept Connect: extract credentials and tell the engine.
                if matches!(msg, login::Message::Connect) {
                    if let Screen::Login(ref mut login) = self.screen {
                        let config = login.connect_config();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            return Task::future(async move {
                                let _ = tx.send(XmppCommand::Connect(config)).await;
                                // No message to emit — fire and forget.
                                Message::Login(login::Message::Connecting)
                            });
                        }
                    }
                }

                // Lift login-level GoToBenchmark to the top-level variant.
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

            Message::XmppReady(tx) => {
                tracing::debug!("xmpp command channel ready");
                self.xmpp_tx = Some(tx);
                Task::none()
            }

            Message::XmppEvent(event) => {
                match &event {
                    XmppEvent::Connected { bound_jid } => {
                        tracing::info!("XMPP: online as {bound_jid}");
                        if let Screen::Login(ref mut login) = self.screen {
                            login.on_connected(bound_jid.clone());
                        }
                    }
                    XmppEvent::Disconnected { reason } => {
                        tracing::warn!("XMPP: disconnected — {reason}");
                        if let Screen::Login(ref mut login) = self.screen {
                            login.on_error(reason.clone());
                        }
                    }
                    XmppEvent::Reconnecting { attempt } => {
                        tracing::info!("XMPP: reconnecting (attempt {attempt})");
                    }
                    XmppEvent::RosterReceived(contacts) => {
                        tracing::info!("XMPP: roster received ({} contacts)", contacts.len());
                    }
                    XmppEvent::MessageReceived(msg) => {
                        tracing::info!("XMPP: message from {} — {}", msg.from, msg.body);
                    }
                    XmppEvent::PresenceUpdated { jid, available } => {
                        tracing::debug!("XMPP: presence {jid} available={available}");
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
        }
    }

    pub fn subscription() -> Subscription<Message> {
        xmpp::subscription::xmpp_subscription()
    }
}
