// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use iced::{Element, Task};

mod login;

pub use login::LoginScreen;

/// Top-level application state.
/// iced 0.13 uses standalone update/view functions instead of Application trait.
pub struct App {
    screen: Screen,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
}

enum Screen {
    Login(LoginScreen),
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
        match (&mut self.screen, message) {
            (Screen::Login(login), Message::Login(msg)) => {
                login.update(msg).map(Message::Login)
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
        }
    }
}
