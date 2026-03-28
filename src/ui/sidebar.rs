// Task P2.2 — Sidebar / contact list
// Source reference: apps/fluux/src/components/Sidebar.tsx
//                   apps/fluux/src/components/sidebar-components/ContactList.tsx

use std::collections::HashMap;

use iced::{
    widget::text::Shaping,
    widget::{
        button, column, container, horizontal_rule, image, row, scrollable, text, text_input,
        tooltip,
    },
    Alignment, Element, Length, Task,
};

use crate::ui::account_state::account_color;
use crate::ui::avatar::{jid_color, jid_initial};
use crate::ui::palette;

use crate::xmpp::{modules::presence_machine::PresenceStatus, AccountId, RosterContact};

#[derive(Debug, Clone)]
pub struct SidebarScreen {
    contacts: Vec<RosterContact>,
    selected_jid: Option<String>,
    presence: HashMap<String, bool>,
    unread_counts: HashMap<String, u32>, // B5: unread message counts per JID
    last_messages: HashMap<String, String>, // last-message preview per JID
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
    // UX-2: account menu popover state
    show_account_menu: bool,
    // C2: own presence status for the indicator dot
    pub(crate) own_presence: PresenceStatus,
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
    // MULTI: open the account switcher panel
    OpenAccountSwitcher,
    // UX-2: account menu popover
    ToggleAccountMenu,
    SetPresence(PresenceStatus),
    OpenSettings,
}

#[allow(dead_code)]
pub enum Action {
    None,
    Task(Task<Message>),
    SelectContact(String),
    AddContact(String),
    RemoveContact(String),
    JoinRoom {
        jid: String,
        nick: String,
    },
    CreateRoom {
        local: String,
        service: String,
        nick: String,
    },
    RenameContact {
        jid: String,
        name: String,
    },
    SetPresence(PresenceStatus),
    OpenSettings,
    OpenAccountSwitcher,
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
            last_messages: HashMap::new(),
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
            show_account_menu: false,
            own_presence: PresenceStatus::Available,
        }
    }

    /// Ensure a JID appears in the sidebar contacts list.
    /// If it's not already present, add a synthetic roster entry.
    pub fn ensure_contact(&mut self, jid: &str, name: Option<&str>) {
        if !self.contacts.iter().any(|c| c.jid == jid) {
            self.contacts.push(crate::xmpp::RosterContact {
                jid: jid.to_string(),
                name: name.map(str::to_string),
                subscription: "none".to_string(),
            });
        }
    }

    /// G1: remove a contact/conversation from the sidebar.
    #[allow(dead_code)]
    pub fn remove_contact(&mut self, jid: &str) {
        self.contacts.retain(|c| c.jid != jid);
        self.unread_counts.remove(jid);
        self.last_messages.remove(jid);
        self.presence.remove(jid);
    }

    /// B5: increment unread count for a JID.
    pub fn increment_unread(&mut self, jid: &str) {
        *self.unread_counts.entry(jid.to_owned()).or_insert(0) += 1;
    }

    /// B5: clear unread count for a JID.
    pub fn clear_unread(&mut self, jid: &str) {
        self.unread_counts.remove(jid);
    }

    /// Set the last-message preview for a JID, truncated to 60 chars.
    pub fn set_last_message(&mut self, jid: &str, body: &str) {
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return;
        }
        let preview = if trimmed.len() > 60 {
            // Truncate at a char boundary to avoid panics on multi-byte text.
            let end = trimmed
                .char_indices()
                .nth(60)
                .map_or(trimmed.len(), |(i, _)| i);
            format!("{}...", &trimmed[..end])
        } else {
            trimmed.to_owned()
        };
        self.last_messages.insert(jid.to_owned(), preview);
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

    pub fn update(&mut self, msg: Message) -> Action {
        match msg {
            Message::SelectContact(jid) => {
                self.selected_jid = Some(jid.clone());
                self.selected_profile = None;
                return Action::SelectContact(jid);
            }
            Message::ToggleAddContact => {
                self.show_add_contact = !self.show_add_contact;
                self.add_contact_input.clear();
            }
            Message::AddContactInputChanged(v) => {
                self.add_contact_input = v;
            }
            Message::SubmitAddContact => {
                let jid = std::mem::take(&mut self.add_contact_input);
                self.show_add_contact = false;
                if !jid.trim().is_empty() {
                    return Action::AddContact(jid);
                }
            }
            Message::RemoveContact(jid) => {
                return Action::RemoveContact(jid);
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
                if let Some((jid, name)) = self.rename_state.take() {
                    if !name.trim().is_empty() {
                        return Action::RenameContact { jid, name };
                    }
                }
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
                let jid = std::mem::take(&mut self.join_room_jid);
                let nick = std::mem::take(&mut self.join_room_nick);
                self.show_join_room = false;
                if !jid.trim().is_empty() && !nick.trim().is_empty() {
                    return Action::JoinRoom { jid, nick };
                }
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
                let local = std::mem::take(&mut self.create_room_local);
                let service = std::mem::take(&mut self.create_room_service);
                let nick = std::mem::take(&mut self.create_room_nick);
                self.show_create_room = false;
                if !local.trim().is_empty() && !service.trim().is_empty() && !nick.trim().is_empty()
                {
                    return Action::CreateRoom {
                        local,
                        service,
                        nick,
                    };
                }
            }
            Message::OpenAccountSwitcher => {
                self.show_account_menu = false;
                return Action::OpenAccountSwitcher;
            }
            // UX-2: account menu popover
            Message::ToggleAccountMenu => {
                self.show_account_menu = !self.show_account_menu;
            }
            Message::SetPresence(status) => {
                self.show_account_menu = false;
                return Action::SetPresence(status);
            }
            Message::OpenSettings => {
                self.show_account_menu = false;
                return Action::OpenSettings;
            }
        }
        Action::None
    }

    /// G6: render sidebar with optional draft indicators.
    /// `drafts` is a list of JIDs that currently have a non-empty draft.
    /// `default_conference_service` is pre-filled in the create-room form.
    /// `active_account` is the currently active account for the indicator bar
    /// (pass `None` when operating in single-account mode or before login).
    /// `unread_total` is the aggregate unread count shown on the indicator badge.
    /// `muc_rooms` is the set of JIDs that are MUC rooms (shown with a # prefix).
    pub fn view_with_drafts(
        &self,
        drafts: &[String],
        default_conference_service: &str,
        active_account: Option<(&AccountId, usize)>,
        vctx: &super::ViewContext<'_>,
        muc_rooms: &std::collections::HashSet<String>,
    ) -> Element<'_, Message> {
        // MULTI: account indicator bar — shown at the very top of the sidebar
        // when an account is active. Displays a colored dot, truncated JID,
        // and (optionally) an unread badge; clicking opens the account switcher.
        let account_indicator: Option<Element<Message>> = active_account.map(|(id, unread)| {
            let _account_color = account_color(id);
            // C2: dot color reflects presence status
            let color = match self.own_presence {
                PresenceStatus::Available => iced::Color::from_rgb(0.2, 0.8, 0.2),    // green
                PresenceStatus::Away => iced::Color::from_rgb(1.0, 0.75, 0.0),        // amber
                PresenceStatus::ExtendedAway => iced::Color::from_rgb(1.0, 0.6, 0.0), // orange
                PresenceStatus::DoNotDisturb => iced::Color::from_rgb(0.9, 0.2, 0.2), // red
                PresenceStatus::Offline => iced::Color::from_rgb(0.5, 0.5, 0.5),      // gray
            };
            let dot = container(text("").size(1)).width(10).height(10).style(
                move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );

            // Truncate JID to at most 22 chars so it fits the 200-px sidebar.
            let jid = id.as_str();
            let label = if jid.len() > 22 {
                format!("{}…", &jid[..21])
            } else {
                jid.to_owned()
            };

            let label_elem: Element<Message> = if unread > 0 {
                row![
                    text(label).size(12).width(Length::Fill),
                    container(text(unread.to_string()).size(10))
                        .width(18)
                        .height(18)
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center),
                ]
                .align_y(Alignment::Center)
                .into()
            } else {
                text(label).size(12).width(Length::Fill).into()
            };

            let indicator_row = row![dot, label_elem]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Length::Fill);

            button(indicator_row)
                .on_press(Message::ToggleAccountMenu)
                .width(Length::Fill)
                .padding([4, 8])
                .into()
        });

        let add_btn = tooltip(
            button("+")
                .on_press(Message::ToggleAddContact)
                .padding([2, 6]),
            "Add Contact",
            tooltip::Position::Bottom,
        );
        let join_btn = tooltip(
            button("#")
                .on_press(Message::ToggleJoinRoom)
                .padding([2, 6]),
            "Join Room",
            tooltip::Position::Bottom,
        );
        let create_btn = tooltip(
            button("New")
                .on_press(Message::ToggleCreateRoom)
                .padding([2, 6]),
            "Create Room",
            tooltip::Position::Bottom,
        );
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
                let is_muc = muc_rooms.contains(&c.jid);
                let muc_prefix = if is_muc { "# " } else { "" };
                // G6: append [draft] if this JID has a non-empty draft
                let has_draft = drafts.iter().any(|d| d == &c.jid);
                let name_label = if has_draft {
                    format!("{}{} {} [draft]", indicator, muc_prefix, display_name)
                } else {
                    format!("{}{} {}", indicator, muc_prefix, display_name)
                };

                // H5/H1: avatar image if available, otherwise colored initial (32x32)
                let avatar: Element<Message> = if let Some(png) = vctx.avatars.get(c.jid.as_str()) {
                    let handle = iced::widget::image::Handle::from_bytes(png.clone());
                    image(handle).width(32).height(32).into()
                } else {
                    let color = jid_color(&c.jid);
                    let initial = jid_initial(&c.jid).to_string();
                    container(text(initial).size(14))
                        .width(32)
                        .height(32)
                        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                            background: Some(iced::Background::Color(color)),
                            ..Default::default()
                        })
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .into()
                };

                // B5: unread badge
                let unread = self.unread_counts.get(c.jid.as_str()).copied().unwrap_or(0);
                let name_row: Element<Message> = if unread > 0 {
                    row![
                        text(name_label)
                            .size(13)
                            .width(Length::Fill)
                            .shaping(Shaping::Advanced),
                        container(text(unread.to_string()).size(11))
                            .width(20)
                            .height(20)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    ]
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    text(name_label).size(13).shaping(Shaping::Advanced).into()
                };

                // Last-message preview (muted grey, smaller text)
                let info_col: Element<Message> =
                    if let Some(preview) = self.last_messages.get(c.jid.as_str()) {
                        column![
                            name_row,
                            text(preview.as_str())
                                .size(11)
                                .color(palette::MUTED_TEXT)
                                .shaping(Shaping::Advanced),
                        ]
                        .spacing(1)
                        .width(Length::Fill)
                        .into()
                    } else {
                        name_row
                    };

                let label_row = row![avatar, info_col]
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
                    button(text("✏").size(10).shaping(Shaping::Advanced))
                        .on_press(Message::StartRename(jid_for_rename, current_name))
                        .padding([2, 4]),
                    "Rename",
                    tooltip::Position::Top,
                );

                // H3: remove button
                let jid_for_remove = c.jid.clone();
                let remove_btn = tooltip(
                    button(text("✕").size(10).shaping(Shaping::Advanced))
                        .on_press(Message::RemoveContact(jid_for_remove))
                        .padding([2, 4]),
                    "Remove contact",
                    tooltip::Position::Top,
                );

                // H3: if renaming this contact, show inline rename input
                if let Some((_, new_name)) = self.rename_state.as_ref().filter(|(j, _)| j == &c.jid)
                {
                    let rename_input = text_input("New name…", new_name)
                        .on_input(Message::RenameInputChanged)
                        .on_submit(Message::SubmitRename)
                        .padding(4);
                    let confirm_btn = button(text("OK").size(10))
                        .on_press(Message::SubmitRename)
                        .padding([2, 5]);
                    let cancel_btn = button(text("✕").size(10).shaping(Shaping::Advanced))
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

        let mut col = column![].spacing(4).padding(8).width(Length::Fill);
        if let Some(indicator) = account_indicator {
            col = col.push(indicator);
        }
        // UX-2: account menu popover — presence options + settings
        if self.show_account_menu {
            let available_btn = button(
                row![text("●").size(12).shaping(Shaping::Advanced), text("Available").size(12)].spacing(6),
            )
            .on_press(Message::SetPresence(PresenceStatus::Available))
            .width(Length::Fill)
            .padding([4, 8]);
            let away_btn = button(
                row![text("◐").size(12).shaping(Shaping::Advanced), text("Away").size(12)].spacing(6),
            )
            .on_press(Message::SetPresence(PresenceStatus::Away))
            .width(Length::Fill)
            .padding([4, 8]);
            let dnd_btn = button(
                row![text("⛔").size(12).shaping(Shaping::Advanced), text("DND").size(12)].spacing(6),
            )
            .on_press(Message::SetPresence(PresenceStatus::DoNotDisturb))
            .width(Length::Fill)
            .padding([4, 8]);
            let settings_btn = button(
                row![text("⚙").size(12).shaping(Shaping::Advanced), text("Settings").size(12)].spacing(6),
            )
            .on_press(Message::OpenSettings)
            .width(Length::Fill)
            .padding([4, 8]);
            let switch_btn = button(text("Switch Account").size(12))
                .on_press(Message::OpenAccountSwitcher)
                .width(Length::Fill)
                .padding([4, 8]);
            let menu_col = column![
                available_btn,
                away_btn,
                dnd_btn,
                horizontal_rule(1),
                settings_btn,
                switch_btn,
            ]
            .spacing(2)
            .padding(4);
            let menu_panel =
                container(menu_col)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(palette::SURFACE_DARK)),
                        border: iced::Border {
                            color: palette::BORDER_SUBTLE,
                            width: 1.0,
                            radius: 2.0.into(),
                        },
                        ..Default::default()
                    });
            col = col.push(menu_panel);
        }
        col = col.push(header_row);
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
            let close_btn = button(text("✕").size(10).shaping(Shaping::Advanced))
                .on_press(Message::CloseProfile)
                .padding([2, 4]);

            // H5: large avatar for profile popover (64x64)
            let profile_avatar: Element<'_, Message> =
                if let Some(png) = vctx.avatars.get(jid.as_str()) {
                    let handle = iced::widget::image::Handle::from_bytes(png.clone());
                    iced::widget::image(handle).width(64).height(64).into()
                } else {
                    let avatar_color = jid_color(jid.as_str());
                    let avatar_initial = jid_initial(jid.as_str()).to_string();
                    container(text(avatar_initial).size(28))
                        .width(64)
                        .height(64)
                        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                            background: Some(iced::Background::Color(avatar_color)),
                            ..Default::default()
                        })
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .into()
                };

            let profile_col = column![
                row![text("Profile").size(12).width(Length::Fill), close_btn,]
                    .spacing(4)
                    .align_y(Alignment::Center),
                profile_avatar,
                text(name).size(13).shaping(Shaping::Advanced),
                text(jid.as_str()).size(11),
            ]
            .spacing(4)
            .padding(8);
            let profile_panel =
                container(profile_col)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(palette::SURFACE_DARK)),
                        border: iced::Border {
                            color: palette::BORDER_SUBTLE,
                            width: 1.0,
                            radius: 2.0.into(),
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
