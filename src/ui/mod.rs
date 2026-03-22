// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use iced::{Element, Subscription, Task};

mod login;
pub mod benchmark;

pub use login::LoginScreen;
pub use benchmark::BenchmarkScreen;

use crate::xmpp::{self, XmppEvent};

/// Top-level application state.
/// iced 0.13 uses standalone update/view functions instead of Application trait.
pub struct App {
    screen: Screen,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    GoToBenchmark,
    XmppEvent(XmppEvent),
}

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
            Message::XmppEvent(event) => {
                match event {
                    XmppEvent::Connected => {
                        tracing::info!("XMPP: connected");
                    }
                    XmppEvent::Disconnected { reason } => {
                        tracing::warn!("XMPP: disconnected — {reason}");
                    }
                    XmppEvent::Reconnecting { attempt } => {
                        tracing::info!("XMPP: reconnecting (attempt {attempt})");
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
