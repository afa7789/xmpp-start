// Task P2.2 — Sidebar / contact list
// Source reference: apps/fluux/src/components/Sidebar.tsx
//                   apps/fluux/src/components/sidebar-components/ContactList.tsx

use std::collections::HashMap;

use iced::{
    widget::{button, column, container, row, scrollable, text},
    Element, Length, Task,
};

use crate::ui::avatar::{jid_color, jid_initial};

use crate::xmpp::RosterContact;

#[derive(Debug, Clone)]
pub struct SidebarScreen {
    contacts: Vec<RosterContact>,
    selected_jid: Option<String>,
    presence: HashMap<String, bool>,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectContact(String),
}

impl Default for SidebarScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarScreen {
    pub fn new() -> Self {
        Self {
            contacts: vec![],
            selected_jid: None,
            presence: HashMap::new(),
        }
    }

    pub fn on_presence(&mut self, jid: &str, available: bool) {
        self.presence.insert(jid.to_owned(), available);
    }

    pub fn set_contacts(&mut self, contacts: Vec<RosterContact>) {
        self.contacts = contacts;
    }

    #[allow(dead_code)]
    pub fn selected_jid(&self) -> Option<&str> {
        self.selected_jid.as_deref()
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::SelectContact(jid) => {
                self.selected_jid = Some(jid);
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = text("Contacts").size(16);

        let contact_rows: Vec<Element<Message>> = self
            .contacts
            .iter()
            .map(|c| {
                let available = self.presence.get(c.jid.as_str()).copied().unwrap_or(false);
                let indicator = if available { "●" } else { "○" };
                let display_name = c.name.as_deref().unwrap_or(&c.jid);

                // H5: colored avatar square with JID initial (32x32)
                let color = jid_color(&c.jid);
                let initial = jid_initial(&c.jid).to_string();
                let avatar = container(text(initial).size(14))
                    .width(32)
                    .height(32)
                    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(color)),
                        ..Default::default()
                    })
                    .align_x(iced::Alignment::Center)
                    .align_y(iced::Alignment::Center);

                let label_row = row![
                    avatar,
                    text(format!("{} {}", indicator, display_name)).size(13),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center);

                let btn = button(label_row).width(Length::Fill).padding([4, 8]);

                let btn = btn.on_press(Message::SelectContact(c.jid.clone()));

                btn.into()
            })
            .collect();

        let empty_note = if self.contacts.is_empty() {
            Some(text("(no contacts)").size(12))
        } else {
            None
        };

        let mut col = column![header].spacing(4).padding(8).width(Length::Fill);

        for row in contact_rows {
            col = col.push(row);
        }

        if let Some(note) = empty_note {
            col = col.push(note);
        }

        container(scrollable(col))
            .width(200)
            .height(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_empty_by_default() {
        let s = SidebarScreen::new();
        assert!(s.contacts.is_empty());
        assert!(s.selected_jid().is_none());
    }

    #[test]
    fn sidebar_select_contact() {
        let mut s = SidebarScreen::new();
        s.set_contacts(vec![RosterContact {
            jid: "alice@example.com".into(),
            name: Some("Alice".into()),
            subscription: "Both".into(),
        }]);
        let _ = s.update(Message::SelectContact("alice@example.com".into()));
        assert_eq!(s.selected_jid(), Some("alice@example.com"));
    }
}
