// Task P2.1 — Port LoginScreen.tsx → iced login view
// Source reference: apps/fluux/src/components/LoginScreen.tsx

use iced::{
    widget::{button, column, container, text, text_input},
    Alignment, Element, Length, Task,
};

#[derive(Debug, Clone)]
pub struct LoginScreen {
    jid: String,
    password: String,
    server: String,
    state: LoginState,
}

#[derive(Debug, Clone)]
enum LoginState {
    Idle,
    Connecting,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    JidChanged(String),
    PasswordChanged(String),
    ServerChanged(String),
    Connect,
    GoToBenchmark,
    // TODO: Connected(XmppClient) — emitted after successful auth (Task P1.2)
}

impl LoginScreen {
    pub fn new() -> Self {
        Self {
            jid: String::new(),
            password: String::new(),
            server: String::new(),
            state: LoginState::Idle,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::JidChanged(v) => self.jid = v,
            Message::PasswordChanged(v) => self.password = v,
            Message::ServerChanged(v) => self.server = v,
            Message::Connect => {
                // TODO: Task P1.1 — spawn tokio task, connect via xmpp engine
                self.state = LoginState::Connecting;
            }
            Message::GoToBenchmark => {
                // Handled by App::update; nothing to mutate here.
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<Message> {
        let status = match &self.state {
            LoginState::Idle => text(""),
            LoginState::Connecting => text("Connecting..."),
            LoginState::Error(e) => text(format!("Error: {e}")),
        };

        let form = column![
            text("XMPP Messenger").size(28),
            text_input("JID (user@server.tld)", &self.jid)
                .on_input(Message::JidChanged)
                .padding(10),
            text_input("Password", &self.password)
                .secure(true)
                .on_input(Message::PasswordChanged)
                .padding(10),
            text_input("Server (optional)", &self.server)
                .on_input(Message::ServerChanged)
                .padding(10),
            button("Connect")
                .on_press(Message::Connect)
                .padding([10, 24]),
            status,
            button("Benchmark →")
                .on_press(Message::GoToBenchmark)
                .padding([6, 16]),
        ]
        .spacing(12)
        .max_width(400)
        .align_x(Alignment::Center);

        container(form)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    }
}
