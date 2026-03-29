// Task P2.1 — Port LoginScreen.tsx → iced login view
// Source reference: apps/fluux/src/components/LoginScreen.tsx

use iced::{
    widget::{button, checkbox, column, container, mouse_area, text, text_input},
    Alignment, Element, Length,
};

mod field_ids {
    pub const JID: &str = "login_jid";
    pub const PASSWORD: &str = "login_password";
    pub const SERVER: &str = "login_server";
}

use crate::ui::palette;
use crate::xmpp::ConnectConfig;

#[derive(Debug, Clone)]
pub struct LoginScreen {
    jid: String,
    password: String,
    server: String,
    state: LoginState,
    /// AUTH-1: remember me — if true, password stays in keychain and we auto-connect next launch.
    pub remember_me: bool,
}

#[derive(Debug, Clone)]
enum LoginState {
    Idle,
    Connecting,
    Registering,
    Error(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    JidChanged(String),
    PasswordChanged(String),
    ServerChanged(String),
    Connect,
    Register,
    /// Sent by App::update after dispatching the Connect command.
    Connecting,
    /// Sent by App::update after dispatching the Register command.
    Registering,
    GoToBenchmark,
    /// AUTH-1: toggled by the Remember Me checkbox.
    RememberMeToggled(bool),
}

pub enum Action {
    None,
    AttemptConnect(ConnectConfig),
    AttemptRegister(ConnectConfig),
    GoToBenchmark,
    RememberMeToggled(bool),
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
            remember_me: true,
        }
    }

    /// Pre-fill fields from saved settings + keychain.
    pub fn with_saved(jid: String, password: String, server: String, remember_me: bool) -> Self {
        Self {
            jid,
            password,
            server,
            state: LoginState::Idle,
            remember_me,
        }
    }

    /// Build a ConnectConfig from the current field values.
    pub fn connect_config(&self) -> ConnectConfig {
        let settings = crate::config::load();
        ConnectConfig {
            jid: self.jid.clone(),
            password: self.password.clone(),
            server: self.server.clone(),
            status_message: settings.status_message,
            send_receipts: settings.send_receipts,
            send_typing: settings.send_typing,
            send_read_markers: settings.send_read_markers,
            proxy_type: settings.proxy_type,
            proxy_host: settings.proxy_host,
            proxy_port: settings.proxy_port,
            manual_srv: settings.manual_srv,
            push_service_jid: settings.push_service_jid,
            allow_insecure_tls: !settings.force_tls,
        }
    }

    /// Called by App when XmppEvent::Disconnected arrives.
    pub fn on_error(&mut self, reason: String) {
        self.state = LoginState::Error(reason);
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::JidChanged(v) => self.jid = v,
            Message::PasswordChanged(v) => self.password = v,
            Message::ServerChanged(v) => self.server = v,
            Message::Connect => {
                let account = crate::config::account::AccountConfig::new(self.jid.clone());
                if let Err(e) = account.validate() {
                    self.state = LoginState::Error(e);
                    return Action::None;
                }
                self.state = LoginState::Connecting;
                return Action::AttemptConnect(self.connect_config());
            }
            Message::Register => {
                let account = crate::config::account::AccountConfig::new(self.jid.clone());
                if let Err(e) = account.validate() {
                    self.state = LoginState::Error(e);
                    return Action::None;
                }
                self.state = LoginState::Registering;
                return Action::AttemptRegister(self.connect_config());
            }
            Message::Connecting => {
                self.state = LoginState::Connecting;
            }
            Message::Registering => {
                self.state = LoginState::Registering;
            }
            Message::GoToBenchmark => {
                return Action::GoToBenchmark;
            }
            Message::RememberMeToggled(v) => {
                self.remember_me = v;
                return Action::RememberMeToggled(v);
            }
        }
        Action::None
    }

    pub fn view(&self) -> Element<'_, Message> {
        let status = match &self.state {
            LoginState::Idle => text(""),
            LoginState::Connecting => text("Connecting…"),
            LoginState::Registering => text("Registering account…"),
            LoginState::Error(e) => text(format!("Error: {e}")),
        };

        let connect_enabled = !self.jid.is_empty() && !self.password.is_empty();
        let connect_btn = if connect_enabled {
            button("Connect").on_press(Message::Connect)
        } else {
            button("Connect")
        };

        let register_btn = if connect_enabled {
            button("Register").on_press(Message::Register)
        } else {
            button("Register")
        };

        let remember_me_row =
            checkbox("Remember me", self.remember_me).on_toggle(Message::RememberMeToggled);

        let form = column![
            text("XMPP Messenger").size(28),
            text_input("JID (user@server.tld)", &self.jid)
                .id(text_input::Id::new(field_ids::JID))
                .on_input(Message::JidChanged)
                .on_submit(Message::Connect)
                .padding(10),
            text_input("Password", &self.password)
                .id(text_input::Id::new(field_ids::PASSWORD))
                .secure(true)
                .on_input(Message::PasswordChanged)
                .on_submit(Message::Connect)
                .padding(10),
            text_input("Server (optional)", &self.server)
                .id(text_input::Id::new(field_ids::SERVER))
                .on_input(Message::ServerChanged)
                .on_submit(Message::Connect)
                .padding(10),
            remember_me_row,
            iced::widget::row![
                connect_btn.padding([10, 24]),
                register_btn.padding([10, 24])
            ]
            .spacing(10),
            status,
            iced::widget::Space::with_height(8),
            mouse_area(text("Benchmark →").size(11).color(palette::MUTED_TEXT),)
                .on_press(Message::GoToBenchmark),
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

#[cfg(test)]
impl LoginScreen {
    pub fn jid(&self) -> &str {
        &self.jid
    }
    pub fn is_connecting(&self) -> bool {
        matches!(self.state, LoginState::Connecting)
    }
    pub fn error_message(&self) -> Option<&str> {
        if let LoginState::Error(ref e) = self.state {
            Some(e)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_connect_transitions_to_connecting_state() {
        let mut screen = LoginScreen::new();
        screen.jid = "user@example.com".into();
        screen.password = "secret".into();
        let action = screen.update(Message::Connect);
        assert!(screen.is_connecting());
        assert!(matches!(action, Action::AttemptConnect(_)));
    }

    #[test]
    fn login_on_error_sets_error_state() {
        let mut screen = LoginScreen::new();
        screen.on_error("Authentication failed".to_string());
        assert_eq!(screen.error_message(), Some("Authentication failed"));
    }

    #[test]
    fn login_jid_changed_updates_field() {
        let mut screen = LoginScreen::new();
        let action = screen.update(Message::JidChanged("alice@example.com".into()));
        assert_eq!(screen.jid(), "alice@example.com");
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn login_remember_me_toggle_returns_action() {
        let mut screen = LoginScreen::new();
        let action = screen.update(Message::RememberMeToggled(false));
        assert!(!screen.remember_me);
        assert!(matches!(action, Action::RememberMeToggled(false)));
    }
}
