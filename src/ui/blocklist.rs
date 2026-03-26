// M3: Blocklist search + add JID UI (XEP-0191)
//
// A panel that shows the current block list, lets the user filter it,
// add a new JID to block, and unblock existing entries.
//
// The panel produces Action values that the caller (SettingsScreen)
// converts into XmppCommand::BlockJid / XmppCommand::UnblockJid.

use iced::{
    widget::{button, column, container, row, scrollable, text, text_input},
    Alignment, Element, Length,
};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BlocklistPanel {
    /// Sorted, deduplicated list of currently blocked JIDs.
    pub blocked: Vec<String>,
    /// Current text in the search/filter input.
    pub filter: String,
    /// Current text in the "add new JID" input.
    pub new_jid: String,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    FilterChanged(String),
    NewJidChanged(String),
    AddJid,
    Unblock(String),
}

// ---------------------------------------------------------------------------
// Actions returned to the caller
// ---------------------------------------------------------------------------

/// Side-effects the caller should perform after an update.
pub enum Action {
    None,
    Block(String),
    Unblock(String),
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

impl BlocklistPanel {
    pub fn new(blocked: Vec<String>) -> Self {
        let mut sorted = blocked;
        sorted.sort();
        Self {
            blocked: sorted,
            filter: String::new(),
            new_jid: String::new(),
        }
    }

    /// Update the panel, returning an action for the caller.
    pub fn update(&mut self, msg: Message) -> Action {
        match msg {
            Message::FilterChanged(v) => {
                self.filter = v;
                Action::None
            }
            Message::NewJidChanged(v) => {
                self.new_jid = v;
                Action::None
            }
            Message::AddJid => {
                let jid = self.new_jid.trim().to_string();
                if jid.is_empty() {
                    return Action::None;
                }
                self.new_jid.clear();
                // Optimistically add to local list so UI updates immediately.
                if !self.blocked.contains(&jid) {
                    self.blocked.push(jid.clone());
                    self.blocked.sort();
                }
                Action::Block(jid)
            }
            Message::Unblock(jid) => {
                self.blocked.retain(|b| b != &jid);
                Action::Unblock(jid)
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = text("Blocked Users").size(16);

        // Search / filter input
        let filter_input = text_input("Filter list…", &self.filter)
            .on_input(Message::FilterChanged)
            .padding([6, 10])
            .width(Length::Fill);

        // Filtered list of blocked JIDs
        let filtered: Vec<&String> = self
            .blocked
            .iter()
            .filter(|jid| {
                self.filter.is_empty() || jid.to_lowercase().contains(&self.filter.to_lowercase())
            })
            .collect();

        let list_items: Vec<Element<Message>> = filtered
            .into_iter()
            .map(|jid| {
                let jid_clone = jid.clone();
                row![
                    text(jid.as_str()).size(13).width(Length::Fill),
                    button(text("Unblock").size(12))
                        .on_press(Message::Unblock(jid_clone))
                        .padding([3, 8]),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            })
            .collect();

        let list = list_items
            .into_iter()
            .fold(column![].spacing(4), iced::widget::Column::push);

        let list_scroll = scrollable(list).height(200);

        // Add-new-JID row
        let add_input = text_input("e.g. spammer@example.org", &self.new_jid)
            .on_input(Message::NewJidChanged)
            .on_submit(Message::AddJid)
            .padding([6, 10])
            .width(Length::Fill);

        let add_btn = button(text("Block").size(13))
            .on_press(Message::AddJid)
            .padding([6, 12]);

        let add_row = row![add_input, add_btn]
            .spacing(8)
            .align_y(Alignment::Center);

        let content = column![header, filter_input, list_scroll, add_row].spacing(12);

        container(content).padding(0).into()
    }
}
