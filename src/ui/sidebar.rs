// Task P2.2 — Sidebar / contact list
// Source reference: apps/fluux/src/components/Sidebar.tsx
//                   apps/fluux/src/components/sidebar-components/ContactList.tsx

use std::collections::HashMap;

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input},
    Alignment, Element, Length, Task,
};

use crate::ui::avatar::{jid_color, jid_initial};

use crate::xmpp::RosterContact;

#[derive(Debug, Clone)]
pub struct SidebarScreen {
    contacts: Vec<RosterContact>,
    selected_jid: Option<String>,
    presence: HashMap<String, bool>,
    unread_counts: HashMap<String, u32>, // B5: unread message counts per JID
    // H3: add contact UI state
    show_add_contact: bool,
    add_contact_input: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectContact(String),
    ToggleAddContact,                 // H3: show/hide add-contact input
    AddContactInputChanged(String),   // H3: input field changed
    SubmitAddContact,                 // H3: submit add contact
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
            unread_counts: HashMap::new(),
            show_add_contact: false,
            add_contact_input: String::new(),
        }
    }

    /// B5: increment unread count for a JID.
    pub fn increment_unread(&mut self, jid: &str) {
        *self.unread_counts.entry(jid.to_owned()).or_insert(0) += 1;
    }

    /// B5: clear unread count for a JID.
    pub fn clear_unread(&mut self, jid: &str) {
        self.unread_counts.remove(jid);
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

    /// H3: Get the current add-contact input value.
    pub fn add_contact_jid(&self) -> &str {
        &self.add_contact_input
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::SelectContact(jid) => {
                self.selected_jid = Some(jid);
            }
            Message::ToggleAddContact => {
                self.show_add_contact = !self.show_add_contact;
                self.add_contact_input.clear();
            }
            Message::AddContactInputChanged(v) => {
                self.add_contact_input = v;
            }
            Message::SubmitAddContact => {
                // ChatScreen will intercept this
                self.show_add_contact = false;
                self.add_contact_input.clear();
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        self.view_with_drafts(&[])
    }

    /// G6: render sidebar with optional draft indicators.
    /// `drafts` is a list of JIDs that currently have a non-empty draft.
    pub fn view_with_drafts(&self, drafts: &[String]) -> Element<'_, Message> {
        let add_btn = button("+").on_press(Message::ToggleAddContact).padding([2, 6]);
        let header_row = row![
            text("Contacts").size(16).width(Length::Fill),
            add_btn,
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let add_contact_row: Option<Element<Message>> = if self.show_add_contact {
            let input = text_input("JID to add…", &self.add_contact_input)
                .on_input(Message::AddContactInputChanged)
                .on_submit(Message::SubmitAddContact)
                .padding(6);
            let submit_btn = button("Add")
                .on_press(Message::SubmitAddContact)
                .padding([4, 8]);
            Some(row![input, submit_btn].spacing(4).into())
        } else {
            None
        };

        let contact_rows: Vec<Element<Message>> = self
            .contacts
            .iter()
            .map(|c| {
                let available = self.presence.get(c.jid.as_str()).copied().unwrap_or(false);
                let indicator = if available { "●" } else { "○" };
                let display_name = c.name.as_deref().unwrap_or(&c.jid);
                // G6: append [draft] if this JID has a non-empty draft
                let has_draft = drafts.iter().any(|d| d == &c.jid);
                let name_label = if has_draft {
                    format!("{} {} [draft]", indicator, display_name)
                } else {
                    format!("{} {}", indicator, display_name)
                };

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
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center);

                // B5: unread badge
                let unread = self.unread_counts.get(c.jid.as_str()).copied().unwrap_or(0);
                let name_elem: Element<Message> = if unread > 0 {
                    row![
                        text(name_label).size(13).width(Length::Fill),
                        container(text(unread.to_string()).size(11))
                            .width(20)
                            .height(20)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    ]
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    text(name_label).size(13).into()
                };

                let label_row = row![avatar, name_elem]
                    .spacing(6)
                    .align_y(Alignment::Center);

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

        let mut col = column![header_row].spacing(4).padding(8).width(Length::Fill);
        if let Some(add_row) = add_contact_row {
            col = col.push(add_row);
        }

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
