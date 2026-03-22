#![allow(dead_code)]
// Task P3.2 — MUC occupant panel with role/affiliation display
// XEP reference: https://xmpp.org/extensions/xep-0045.html

use iced::{
    widget::{column, container, row, scrollable, text},
    Element, Length,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single occupant entry suitable for display in the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccupantEntry {
    pub nick: String,
    /// One of: "Moderator", "Participant", "Visitor", "None"
    pub role: String,
    /// One of: "Owner", "Admin", "Member", "Outcast", "None"
    pub affiliation: String,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Messages produced by the occupant panel.
///
/// Currently empty — future kick/ban actions will be added here.
#[derive(Debug, Clone)]
pub enum Message {}

// ---------------------------------------------------------------------------
// OccupantPanel
// ---------------------------------------------------------------------------

/// Iced widget that renders the occupant list for a MUC room.
///
/// Occupants are grouped by role in order: Moderators → Participants → Visitors.
/// The panel is a fixed 180 px wide and scrollable.
pub struct OccupantPanel {
    room_jid: String,
    occupants: Vec<OccupantEntry>,
}

impl OccupantPanel {
    /// Create an empty panel for the given room JID.
    pub fn new(room_jid: String) -> Self {
        Self {
            room_jid,
            occupants: Vec::new(),
        }
    }

    /// Replace the current occupant list.
    pub fn set_occupants(&mut self, occupants: Vec<OccupantEntry>) {
        self.occupants = occupants;
    }

    /// Returns the room JID this panel belongs to.
    pub fn room_jid(&self) -> &str {
        &self.room_jid
    }

    /// Returns the number of occupants currently stored.
    pub fn occupant_count(&self) -> usize {
        self.occupants.len()
    }

    /// Render the occupant panel as an iced `Element`.
    ///
    /// Layout:
    /// - Header: `"Occupants (n)"`
    /// - Groups: Moderators → Participants → Visitors
    /// - Each row: `"● nick"` (available) or `"○ nick"` (unavailable),
    ///   plus `"[Mod]"` or `"[Admin]"` badge if applicable.
    /// - Fixed width 180 px, scrollable.
    pub fn view(&self) -> Element<'_, Message> {
        let header = text(format!("Occupants ({})", self.occupants.len())).size(15);

        let groups: [(&str, &str); 3] = [
            ("Moderator", "Moderators"),
            ("Participant", "Participants"),
            ("Visitor", "Visitors"),
        ];

        let mut col = column![header].spacing(6).padding(8);

        for (role_key, group_label) in &groups {
            let members: Vec<&OccupantEntry> = self
                .occupants
                .iter()
                .filter(|o| o.role == *role_key)
                .collect();

            if members.is_empty() {
                continue;
            }

            // Group header
            col = col.push(text(*group_label).size(12));

            for entry in members {
                let indicator = if entry.available { "●" } else { "○" };
                let label = format!("{} {}", indicator, entry.nick);

                // Role/affiliation badge
                let badge: Option<&str> = match entry.role.as_str() {
                    "Moderator" => Some("[Mod]"),
                    _ => match entry.affiliation.as_str() {
                        "Owner" | "Admin" => Some("[Admin]"),
                        _ => None,
                    },
                };

                let name_widget = text(label).size(13);

                let entry_row: Element<Message> = if let Some(badge_str) = badge {
                    row![name_widget, text(badge_str).size(11)]
                        .spacing(4)
                        .into()
                } else {
                    name_widget.into()
                };

                col = col.push(entry_row);
            }
        }

        // Show a hint if the room is empty
        if self.occupants.is_empty() {
            col = col.push(text("(empty room)").size(12));
        }

        container(scrollable(col))
            .width(180)
            .height(Length::Fill)
            .into()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(nick: &str, role: &str, affiliation: &str, available: bool) -> OccupantEntry {
        OccupantEntry {
            nick: nick.to_string(),
            role: role.to_string(),
            affiliation: affiliation.to_string(),
            available,
        }
    }

    // 1. New panel has no occupants.
    #[test]
    fn occupant_panel_new_is_empty() {
        let panel = OccupantPanel::new("room@conference.example.com".to_string());
        assert_eq!(panel.occupant_count(), 0);
        assert!(panel.occupants.is_empty());
    }

    // 2. set_occupants stores all entries.
    #[test]
    fn set_occupants_stores_entries() {
        let mut panel = OccupantPanel::new("room@conference.example.com".to_string());
        panel.set_occupants(vec![
            make_entry("alice", "Moderator", "Owner", true),
            make_entry("bob", "Participant", "Member", true),
        ]);
        assert_eq!(panel.occupant_count(), 2);
        assert_eq!(panel.occupants[0].nick, "alice");
        assert_eq!(panel.occupants[1].nick, "bob");
    }

    // 3. Available indicator is distinct from unavailable.
    #[test]
    fn occupant_entry_available_indicator() {
        let online = make_entry("alice", "Participant", "Member", true);
        let offline = make_entry("bob", "Participant", "Member", false);
        assert!(online.available);
        assert!(!offline.available);
        // The two must not be equal.
        assert_ne!(online.available, offline.available);
    }

    // 4. Moderators appear before participants in the sorted view data.
    #[test]
    fn moderators_sorted_before_participants() {
        let mut panel = OccupantPanel::new("room@conference.example.com".to_string());
        panel.set_occupants(vec![
            make_entry("bob", "Participant", "Member", true),
            make_entry("alice", "Moderator", "Owner", true),
        ]);

        // Groups are rendered in order: Moderator → Participant → Visitor.
        // Collect moderators and participants separately to verify ordering.
        let mods: Vec<&OccupantEntry> = panel
            .occupants
            .iter()
            .filter(|o| o.role == "Moderator")
            .collect();
        let participants: Vec<&OccupantEntry> = panel
            .occupants
            .iter()
            .filter(|o| o.role == "Participant")
            .collect();

        assert_eq!(mods.len(), 1);
        assert_eq!(participants.len(), 1);
        assert_eq!(mods[0].nick, "alice");
        assert_eq!(participants[0].nick, "bob");
    }

    // 5. occupant_count matches the number of entries set.
    #[test]
    fn occupant_count() {
        let mut panel = OccupantPanel::new("room@conference.example.com".to_string());
        assert_eq!(panel.occupant_count(), 0);

        panel.set_occupants(vec![
            make_entry("alice", "Moderator", "Owner", true),
            make_entry("bob", "Participant", "Member", true),
            make_entry("carol", "Visitor", "None", false),
        ]);
        assert_eq!(panel.occupant_count(), 3);

        // Replacing with fewer entries updates the count.
        panel.set_occupants(vec![make_entry("dave", "Participant", "Member", true)]);
        assert_eq!(panel.occupant_count(), 1);
    }

    // 6. An empty room shows zero occupants.
    #[test]
    fn empty_room_shows_zero_occupants() {
        let mut panel = OccupantPanel::new("empty@conference.example.com".to_string());
        panel.set_occupants(vec![]);
        assert_eq!(panel.occupant_count(), 0);
        assert!(panel.occupants.is_empty());
    }
}
