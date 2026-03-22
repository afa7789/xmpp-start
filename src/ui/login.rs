// Task P2.1 — Port LoginScreen.tsx → iced login view
// Source reference: apps/fluux/src/components/LoginScreen.tsx

use iced::{
    widget::{button, column, container, text, text_input},
    Alignment, Element, Length, Task,
};

use crate::xmpp::ConnectConfig;

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
    #[allow(dead_code)]
    Connected(String), // bound JID
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    JidChanged(String),
    PasswordChanged(String),
    ServerChanged(String),
    Connect,
    /// Sent by App::update after dispatching the Connect command.
    Connecting,
    GoToBenchmark,
}

impl Default for LoginScreen {
    fn default() -> Self {
        Self::new()
    }
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

    /// Pre-fill fields from saved settings + keychain.
    pub fn with_saved(jid: String, password: String, server: String) -> Self {
        Self {
            jid,
            password,
            server,
            state: LoginState::Idle,
        }
    }

    /// Build a ConnectConfig from the current field values.
    pub fn connect_config(&self) -> ConnectConfig {
        ConnectConfig {
            jid: self.jid.clone(),
            password: self.password.clone(),
            server: self.server.clone(),
        }
    }

    /// Called by App when XmppEvent::Connected arrives.
    #[allow(dead_code)]
    pub fn on_connected(&mut self, bound_jid: String) {
        self.state = LoginState::Connected(bound_jid);
    }

    /// Called by App when XmppEvent::Disconnected arrives.
    pub fn on_error(&mut self, reason: String) {
        self.state = LoginState::Error(reason);
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::JidChanged(v) => self.jid = v,
            Message::PasswordChanged(v) => self.password = v,
            Message::ServerChanged(v) => self.server = v,
            Message::Connect => {
                // App::update handles sending the command; we just show spinner.
                self.state = LoginState::Connecting;
            }
            Message::Connecting => {
                self.state = LoginState::Connecting;
            }
            Message::GoToBenchmark => {
                // Handled by App::update; nothing to mutate here.
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let status = match &self.state {
            LoginState::Idle => text(""),
            LoginState::Connecting => text("Connecting…"),
            LoginState::Connected(jid) => text(format!("Connected as {jid}")),
            LoginState::Error(e) => text(format!("Error: {e}")),
        };

        let connect_enabled = !self.jid.is_empty() && !self.password.is_empty();
        let connect_btn = if connect_enabled {
            button("Connect").on_press(Message::Connect)
        } else {
            button("Connect")
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
            connect_btn.padding([10, 24]),
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
