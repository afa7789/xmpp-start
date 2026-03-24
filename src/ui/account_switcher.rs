// MULTI: Account switcher UI skeleton
//
// Displays the list of configured accounts with online/offline status
// indicators and lets the user switch between them or trigger "Add Account".

use iced::{
    widget::{button, column, container, row, text, Space},
    Alignment, Element, Length,
};

use crate::xmpp::AccountId;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// An entry in the account list as seen by the switcher.
#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub id: AccountId,
    /// Display label — usually the bare JID.
    pub label: String,
    /// Whether the account is currently connected.
    pub connected: bool,
    /// Optional accent colour (CSS hex, e.g. "#4A90D9").
    pub color: Option<String>,
}

/// State for the account switcher panel.
#[derive(Debug, Clone, Default)]
pub struct AccountSwitcherScreen {
    pub accounts: Vec<AccountEntry>,
    /// The currently active account (if any).
    pub active: Option<AccountId>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    /// User clicked an account row to make it active.
    SwitchTo(AccountId),
    /// User clicked "Add Account".
    AddAccount,
    /// User closed / dismissed the switcher.
    Close,
}

// ---------------------------------------------------------------------------
// Update + View
// ---------------------------------------------------------------------------

impl AccountSwitcherScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::SwitchTo(id) => {
                self.active = Some(id);
            }
            Message::AddAccount | Message::Close => {
                // Handled by the parent app; no local state change needed.
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text("Accounts").size(18);

        let account_rows: Vec<Element<'_, Message>> = self
            .accounts
            .iter()
            .map(|entry| self.account_row(entry))
            .collect();

        let accounts_list: Element<'_, Message> = if account_rows.is_empty() {
            column![text("No accounts configured.").size(13)]
                .spacing(4)
                .into()
        } else {
            let mut col = column![].spacing(4);
            for row_elem in account_rows {
                col = col.push(row_elem);
            }
            col.into()
        };

        let add_btn = button("Add Account")
            .on_press(Message::AddAccount)
            .padding([8, 16]);

        let close_btn = button("Close")
            .on_press(Message::Close)
            .padding([8, 16]);

        let footer = row![add_btn, Space::with_width(Length::Fill), close_btn]
            .align_y(Alignment::Center);

        let content = column![
            title,
            Space::with_height(Length::Fixed(12.0)),
            accounts_list,
            Space::with_height(Length::Fixed(16.0)),
            footer,
        ]
        .spacing(8)
        .padding(20);

        container(content)
            .width(Length::Fixed(320.0))
            .into()
    }

    fn account_row<'a>(&self, entry: &'a AccountEntry) -> Element<'a, Message> {
        let is_active = self.active.as_ref() == Some(&entry.id);

        let status_label = if entry.connected { "Online" } else { "Offline" };

        let label_col = column![
            text(entry.label.as_str()).size(14),
            text(status_label).size(11),
        ]
        .spacing(2);

        let active_marker: Element<'a, Message> = if is_active {
            text("*").size(14).into()
        } else {
            Space::with_width(Length::Fixed(12.0)).into()
        };

        let row_content = row![active_marker, label_col]
            .spacing(8)
            .align_y(Alignment::Center);

        let btn = button(row_content)
            .on_press(Message::SwitchTo(entry.id.clone()))
            .width(Length::Fill)
            .padding([6, 12]);

        btn.into()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_screen() -> AccountSwitcherScreen {
        AccountSwitcherScreen {
            accounts: vec![
                AccountEntry {
                    id: AccountId::new("alice@example.com"),
                    label: "alice@example.com".into(),
                    connected: true,
                    color: None,
                },
                AccountEntry {
                    id: AccountId::new("bob@example.com"),
                    label: "bob@example.com".into(),
                    connected: false,
                    color: Some("#FF5733".into()),
                },
            ],
            active: Some(AccountId::new("alice@example.com")),
        }
    }

    #[test]
    fn switch_account_updates_active() {
        let mut screen = make_screen();
        assert_eq!(screen.active.as_ref().unwrap().as_str(), "alice@example.com");

        screen.update(Message::SwitchTo(AccountId::new("bob@example.com")));
        assert_eq!(screen.active.as_ref().unwrap().as_str(), "bob@example.com");
    }

    #[test]
    fn default_screen_has_no_active() {
        let screen = AccountSwitcherScreen::default();
        assert!(screen.active.is_none());
        assert!(screen.accounts.is_empty());
    }
}
