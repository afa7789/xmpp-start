// Task P2.2 — Sidebar / contact list
// Source reference: apps/fluux/src/components/Sidebar.tsx
//                   apps/fluux/src/components/sidebar-components/ContactList.tsx

use std::collections::HashMap;

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input, tooltip},
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
    // H3: rename state — (jid, new_name_input)
    rename_state: Option<(String, String)>,
    // H4: profile popover — selected JID for profile view
    selected_profile: Option<String>,
    // D3: join MUC room UI state
    show_join_room: bool,
    join_room_jid: String,
    join_room_nick: String,
    // K1: create MUC room UI state
    show_create_room: bool,
    create_room_local: String,
    create_room_service: String,
    create_room_nick: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectContact(String),
    ToggleAddContact,               // H3: show/hide add-contact input
    AddContactInputChanged(String), // H3: input field changed
    SubmitAddContact,               // H3: submit add contact
    RemoveContact(String),          // H3: remove a contact
    StartRename(String, String),    // H3: (jid, current_name) begin inline rename
    RenameInputChanged(String),     // H3: rename text input changed
    SubmitRename,                   // H3: confirm rename
    CancelRename,                   // H3: cancel rename
    ShowProfile(String),            // H4: show contact profile popover
    CloseProfile,                   // H4: close profile popover
    ToggleJoinRoom,                 // D3: show/hide join-room input
    JoinRoomJidChanged(String),     // D3: room JID input changed
    JoinRoomNickChanged(String),    // D3: nick input changed
    SubmitJoinRoom,                 // D3: submit join room
    // K1: create room
    ToggleCreateRoom,
    CreateRoomLocalChanged(String),
    CreateRoomServiceChanged(String),
    CreateRoomNickChanged(String),
    SubmitCreateRoom,
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
            rename_state: None,
            selected_profile: None,
            show_join_room: false,
            join_room_jid: String::new(),
            join_room_nick: String::new(),
            show_create_room: false,
            create_room_local: String::new(),
            create_room_service: String::new(),
            create_room_nick: String::new(),
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

    /// D3: Get the current join-room JID and nick inputs.
    pub fn join_room_jid(&self) -> &str {
        &self.join_room_jid
    }

    pub fn join_room_nick(&self) -> &str {
        &self.join_room_nick
    }

    /// K1: Get the create-room form fields.
    pub fn create_room_local(&self) -> &str {
        &self.create_room_local
    }

    pub fn create_room_service(&self) -> &str {
        &self.create_room_service
    }

    pub fn create_room_nick(&self) -> &str {
        &self.create_room_nick
    }

    /// H3: Returns the pending rename (jid, new_name) if SubmitRename was triggered.
    pub fn pending_rename(&self) -> Option<(&str, &str)> {
        self.rename_state
            .as_ref()
            .map(|(jid, name)| (jid.as_str(), name.as_str()))
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::SelectContact(jid) => {
                self.selected_jid = Some(jid);
                self.selected_profile = None;
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
            Message::RemoveContact(_jid) => {
                // ChatScreen intercepts this to send command to engine
            }
            Message::StartRename(jid, current_name) => {
                self.rename_state = Some((jid, current_name));
            }
            Message::RenameInputChanged(v) => {
                if let Some((_, name)) = self.rename_state.as_mut() {
                    *name = v;
                }
            }
            Message::SubmitRename => {
                // ChatScreen will intercept this; clear state after
                self.rename_state = None;
            }
            Message::CancelRename => {
                self.rename_state = None;
            }
            Message::ShowProfile(jid) => {
                self.selected_profile = Some(jid);
            }
            Message::CloseProfile => {
                self.selected_profile = None;
            }
            Message::ToggleJoinRoom => {
                self.show_join_room = !self.show_join_room;
                self.join_room_jid.clear();
                self.join_room_nick.clear();
            }
            Message::JoinRoomJidChanged(v) => {
                self.join_room_jid = v;
            }
            Message::JoinRoomNickChanged(v) => {
                self.join_room_nick = v;
            }
            Message::SubmitJoinRoom => {
                // ChatScreen intercepts this to send JoinRoom command to engine
                self.show_join_room = false;
                self.join_room_jid.clear();
                self.join_room_nick.clear();
            }
            // K1: create room
            Message::ToggleCreateRoom => {
                self.show_create_room = !self.show_create_room;
                self.create_room_local.clear();
                self.create_room_service.clear();
                self.create_room_nick.clear();
            }
            Message::CreateRoomLocalChanged(v) => self.create_room_local = v,
            Message::CreateRoomServiceChanged(v) => self.create_room_service = v,
            Message::CreateRoomNickChanged(v) => self.create_room_nick = v,
            Message::SubmitCreateRoom => {
                // ChatScreen intercepts this
                self.show_create_room = false;
                self.create_room_local.clear();
                self.create_room_service.clear();
                self.create_room_nick.clear();
            }
        }
        Task::none()
    }

    /// G6: render sidebar with optional draft indicators.
    /// `drafts` is a list of JIDs that currently have a non-empty draft.
    /// `default_conference_service` is pre-filled in the create-room form.
    pub fn view_with_drafts(
        &self,
        drafts: &[String],
        default_conference_service: &str,
    ) -> Element<'_, Message> {
        let add_btn = button("+")
            .on_press(Message::ToggleAddContact)
            .padding([2, 6]);
        let join_btn = button("#")
            .on_press(Message::ToggleJoinRoom)
            .padding([2, 6]);
        let create_btn = button("*")
            .on_press(Message::ToggleCreateRoom)
            .padding([2, 6]);
        let _ = default_conference_service; // used for default pre-fill (set via ToggleCreateRoom caller)
        let header_row = row![
            text("Contacts").size(16).width(Length::Fill),
            add_btn,
            join_btn,
            create_btn,
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

        // D3: join room input row
        let join_room_row: Option<Element<Message>> = if self.show_join_room {
            let jid_input = text_input("room@conf…", &self.join_room_jid)
                .on_input(Message::JoinRoomJidChanged)
                .on_submit(Message::SubmitJoinRoom)
                .padding(4);
            let nick_input = text_input("nick", &self.join_room_nick)
                .on_input(Message::JoinRoomNickChanged)
                .on_submit(Message::SubmitJoinRoom)
                .padding(4);
            let join_submit_btn = button("Join")
                .on_press(Message::SubmitJoinRoom)
                .padding([4, 8]);
            Some(
                column![jid_input, row![nick_input, join_submit_btn].spacing(4),]
                    .spacing(4)
                    .into(),
            )
        } else {
            None
        };

        // K1: create room input row
        let create_room_row: Option<Element<Message>> = if self.show_create_room {
            let local_input = text_input("room-name", &self.create_room_local)
                .on_input(Message::CreateRoomLocalChanged)
                .on_submit(Message::SubmitCreateRoom)
                .padding(4);
            let service_input = text_input("conference.example.com", &self.create_room_service)
                .on_input(Message::CreateRoomServiceChanged)
                .on_submit(Message::SubmitCreateRoom)
                .padding(4);
            let nick_input = text_input("nick", &self.create_room_nick)
                .on_input(Message::CreateRoomNickChanged)
                .on_submit(Message::SubmitCreateRoom)
                .padding(4);
            let create_submit_btn = button("Create")
                .on_press(Message::SubmitCreateRoom)
                .padding([4, 8]);
            Some(
                column![
                    local_input,
                    service_input,
                    row![nick_input, create_submit_btn].spacing(4),
                ]
                .spacing(4)
                .into(),
            )
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
                    .align_y(Alignment::Center)
                    .width(Length::Fill);

                let contact_btn = button(label_row)
                    .width(Length::Fill)
                    .padding([4, 8])
                    .on_press(Message::SelectContact(c.jid.clone()));

                // H4: profile info button
                let jid_for_profile = c.jid.clone();
                let profile_btn = tooltip(
                    button(text("i").size(10))
                        .on_press(Message::ShowProfile(jid_for_profile))
                        .padding([2, 5]),
                    "View profile",
                    tooltip::Position::Top,
                );

                // H3: rename button
                let current_name = c.name.clone().unwrap_or_else(|| c.jid.clone());
                let jid_for_rename = c.jid.clone();
                let rename_btn = tooltip(
                    button(text("✎").size(10))
                        .on_press(Message::StartRename(jid_for_rename, current_name))
                        .padding([2, 4]),
                    "Rename",
                    tooltip::Position::Top,
                );

                // H3: remove button
                let jid_for_remove = c.jid.clone();
                let remove_btn = tooltip(
                    button(text("✕").size(10))
                        .on_press(Message::RemoveContact(jid_for_remove))
                        .padding([2, 4]),
                    "Remove contact",
                    tooltip::Position::Top,
                );

                // H3: if renaming this contact, show inline rename input
                let is_renaming = self.rename_state.as_ref().is_some_and(|(j, _)| j == &c.jid);

                if is_renaming {
                    let (_, new_name) = self.rename_state.as_ref().unwrap();
                    let rename_input = text_input("New name…", new_name)
                        .on_input(Message::RenameInputChanged)
                        .on_submit(Message::SubmitRename)
                        .padding(4);
                    let confirm_btn = button(text("OK").size(10))
                        .on_press(Message::SubmitRename)
                        .padding([2, 5]);
                    let cancel_btn = button(text("✕").size(10))
                        .on_press(Message::CancelRename)
                        .padding([2, 4]);
                    row![rename_input, confirm_btn, cancel_btn]
                        .spacing(4)
                        .align_y(Alignment::Center)
                        .into()
                } else {
                    row![contact_btn, profile_btn, rename_btn, remove_btn]
                        .spacing(2)
                        .align_y(Alignment::Center)
                        .into()
                }
            })
            .collect();

        let empty_note = if self.contacts.is_empty() {
            Some(text("(no contacts)").size(12))
        } else {
            None
        };

        let mut col = column![header_row]
            .spacing(4)
            .padding(8)
            .width(Length::Fill);
        if let Some(add_row) = add_contact_row {
            col = col.push(add_row);
        }
        if let Some(jr_row) = join_room_row {
            col = col.push(jr_row);
        }
        if let Some(cr_row) = create_room_row {
            col = col.push(cr_row);
        }

        for row in contact_rows {
            col = col.push(row);
        }

        if let Some(note) = empty_note {
            col = col.push(note);
        }

        // H4: profile popover — shown below contact list when a profile is selected
        if let Some(ref jid) = self.selected_profile {
            let contact = self.contacts.iter().find(|c| &c.jid == jid);
            let name = contact
                .and_then(|c| c.name.as_deref())
                .unwrap_or(jid.as_str());
            let close_btn = button(text("✕").size(10))
                .on_press(Message::CloseProfile)
                .padding([2, 4]);
            let profile_col = column![
                row![text("Profile").size(12).width(Length::Fill), close_btn,]
                    .spacing(4)
                    .align_y(Alignment::Center),
                text(name).size(13),
                text(jid.as_str()).size(11),
            ]
            .spacing(4)
            .padding(8);
            let profile_panel =
                container(profile_col)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb(
                            0.13, 0.13, 0.16,
                        ))),
                        border: iced::Border {
                            color: iced::Color::from_rgb(0.3, 0.3, 0.35),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    });
            col = col.push(profile_panel);
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
