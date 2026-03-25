// Task P3.1 — XEP-0045 Multi-User Chat core module
// XEP reference: https://xmpp.org/extensions/xep-0045.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - build join/leave presence stanzas
//   - maintain per-room occupant lists from incoming presence
//   - parse incoming groupchat messages
//   - build room invitations (XEP-0249)

use std::collections::HashMap;

use tokio_xmpp::jid::Jid;
use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_MUC, NS_MUC_USER, NS_X_CONFERENCE};

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Occupant role within a MUC room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Moderator,
    Participant,
    Visitor,
    None,
}

impl Role {
    fn from_str(s: &str) -> Self {
        match s {
            "moderator" => Role::Moderator,
            "participant" => Role::Participant,
            "visitor" => Role::Visitor,
            _ => Role::None,
        }
    }
}

/// Occupant affiliation with a MUC room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Affiliation {
    Owner,
    Admin,
    Member,
    Outcast,
    None,
}

impl Affiliation {
    fn from_str(s: &str) -> Self {
        match s {
            "owner" => Affiliation::Owner,
            "admin" => Affiliation::Admin,
            "member" => Affiliation::Member,
            "outcast" => Affiliation::Outcast,
            _ => Affiliation::None,
        }
    }
}

/// A single occupant in a MUC room.
#[derive(Debug, Clone)]
pub struct Occupant {
    pub nick: String,
    /// Real JID if disclosed by the server.
    pub jid: Option<String>,
    pub role: Role,
    pub affiliation: Affiliation,
    pub available: bool,
}

/// State for a single MUC room we have joined (or are joining).
#[derive(Debug, Clone)]
pub struct MucRoom {
    /// Bare JID of the room: `room@conference.server`.
    pub jid: String,
    /// Our nickname in the room.
    pub nickname: String,
    /// Current room subject/topic.
    pub subject: String,
    /// Map of nick -> Occupant.
    pub occupants: HashMap<String, Occupant>,
}

/// A parsed groupchat message from a MUC room.
#[derive(Debug, Clone)]
pub struct MucMessage {
    pub room_jid: String,
    pub from_nick: String,
    pub body: String,
    pub id: String,
}

// ---------------------------------------------------------------------------
// MucManager
// ---------------------------------------------------------------------------

/// XEP-0045 multi-user chat state manager.
pub struct MucManager {
    rooms: HashMap<String, MucRoom>,
}

impl MucManager {
    /// Creates an empty manager (no joined rooms).
    pub fn new() -> Self {
        Self {
            rooms: HashMap::new(),
        }
    }

    /// Build a join presence stanza and record the room.
    pub fn join_room(&mut self, room_jid: &str, nickname: &str) -> Element {
        self.rooms.insert(
            room_jid.to_string(),
            MucRoom {
                jid: room_jid.to_string(),
                nickname: nickname.to_string(),
                subject: String::new(),
                occupants: HashMap::new(),
            },
        );

        Element::builder("presence", NS_CLIENT)
            .attr("to", format!("{}/{}", room_jid, nickname))
            .append(Element::builder("x", NS_MUC).build())
            .build()
    }

    /// Build an unavailable presence stanza and remove the room.
    pub fn leave_room(&mut self, room_jid: &str) -> Option<Element> {
        let room = self.rooms.remove(room_jid)?;

        let el = Element::builder("presence", NS_CLIENT)
            .attr("to", format!("{}/{}", room_jid, room.nickname))
            .attr("type", "unavailable")
            .build();

        Some(el)
    }

    /// Process an incoming presence stanza and update the occupant list.
    pub fn on_presence(&mut self, el: &Element) {
        let from = match el.attr("from") {
            Some(f) => f,
            None => return,
        };

        let (room_jid, nick) = match from.split_once("/") {
            Some(pair) => pair,
            None => return,
        };

        let room = match self.rooms.get_mut(room_jid) {
            Some(r) => r,
            None => return,
        };

        let presence_type = el.attr("type").unwrap_or("");

        if presence_type == "unavailable" {
            room.occupants.remove(nick);
            return;
        }

        let mut role = Role::None;
        let mut affiliation = Affiliation::None;
        let mut real_jid: Option<String> = None;

        for child in el.children() {
            if child.name() == "x" && child.ns() == NS_MUC_USER {
                for item in child.children() {
                    if item.name() == "item" {
                        if let Some(r) = item.attr("role") {
                            role = Role::from_str(r);
                        }
                        if let Some(a) = item.attr("affiliation") {
                            affiliation = Affiliation::from_str(a);
                        }
                        if let Some(j) = item.attr("jid") {
                            real_jid = Some(j.to_string());
                        }
                    }
                }
            }
        }

        room.occupants.insert(
            nick.to_string(),
            Occupant {
                nick: nick.to_string(),
                jid: real_jid,
                role,
                affiliation,
                available: true,
            },
        );
    }

    /// Parse an incoming groupchat message stanza.
    pub fn on_groupchat_message(&self, el: &Element) -> Option<MucMessage> {
        if el.name() != "message" {
            return None;
        }
        if el.attr("type") != Some("groupchat") {
            return None;
        }

        let from = el.attr("from")?;
        let (room_jid, from_nick) = from.split_once("/")?;

        let body = el.children().find(|c| c.name() == "body")?.text();

        if body.is_empty() {
            return None;
        }

        let id = el.attr("id").map_or_else(
            || Uuid::new_v4().to_string(),
            std::string::ToString::to_string,
        );

        Some(MucMessage {
            room_jid: room_jid.to_string(),
            from_nick: from_nick.to_string(),
            body,
            id,
        })
    }

    /// K3: Build a direct room invitation (XEP-0249).
    pub fn build_invitation(room: &Jid, user: &Jid, reason: Option<&str>) -> Element {
        let mut x_builder = Element::builder("x", NS_X_CONFERENCE).attr("jid", room.as_str());
        if let Some(r) = reason.filter(|r| !r.is_empty()) {
            x_builder = x_builder.append(Element::builder("reason", NS_CLIENT).append(r).build());
        }

        Element::builder("message", NS_CLIENT)
            .attr("to", user.as_str())
            .append(x_builder.build())
            .build()
    }
}

impl Default for MucManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM_JID: &str = "general@conference.example.com";
    const NICK: &str = "alice";
    const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";

    fn make_available_presence(room_jid: &str, nick: &str, role: &str, affiliation: &str) -> Element {
        let item = Element::builder("item", NS_MUC_USER)
            .attr("role", role)
            .attr("affiliation", affiliation)
            .build();
        let x = Element::builder("x", NS_MUC_USER).append(item).build();
        Element::builder("presence", NS_CLIENT)
            .attr("from", format!("{}/{}", room_jid, nick))
            .append(x)
            .build()
    }

    fn make_unavailable_presence(room_jid: &str, nick: &str) -> Element {
        Element::builder("presence", NS_CLIENT)
            .attr("from", format!("{}/{}", room_jid, nick))
            .attr("type", "unavailable")
            .build()
    }

    fn make_groupchat_message(room_jid: &str, nick: &str, body: &str, id: &str) -> Element {
        let body_el = Element::builder("body", NS_CLIENT).append(body).build();
        Element::builder("message", NS_CLIENT)
            .attr("from", format!("{}/{}", room_jid, nick))
            .attr("type", "groupchat")
            .attr("id", id)
            .append(body_el)
            .build()
    }

    #[test]
    fn muc_manager_new_is_empty() {
        let mgr = MucManager::new();
        assert!(mgr.rooms.is_empty());
    }

    #[test]
    fn join_room_builds_presence_stanza() {
        let mut mgr = MucManager::new();
        let el = mgr.join_room(ROOM_JID, NICK);
        assert_eq!(el.name(), "presence");
        let expected_to = format!("{}/{}", ROOM_JID, NICK);
        assert_eq!(el.attr("to"), Some(expected_to.as_str()));
        let x = el.children().find(|c| c.name() == "x");
        assert!(x.is_some());
        assert_eq!(x.unwrap().ns(), NS_MUC);
    }

    #[test]
    fn join_room_adds_room_to_manager() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        assert!(mgr.rooms.contains_key(ROOM_JID));
        assert_eq!(mgr.rooms[ROOM_JID].nickname, NICK);
    }

    #[test]
    fn leave_room_removes_room() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        assert!(mgr.rooms.contains_key(ROOM_JID));
        mgr.leave_room(ROOM_JID);
        assert!(!mgr.rooms.contains_key(ROOM_JID));
    }

    #[test]
    fn leave_room_returns_unavailable_presence() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        let el = mgr.leave_room(ROOM_JID).expect("should return presence");
        assert_eq!(el.name(), "presence");
        assert_eq!(el.attr("type"), Some("unavailable"));
        let expected_to = format!("{}/{}", ROOM_JID, NICK);
        assert_eq!(el.attr("to"), Some(expected_to.as_str()));
    }

    #[test]
    fn on_presence_adds_occupant() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        let presence = make_available_presence(ROOM_JID, "bob", "participant", "member");
        mgr.on_presence(&presence);
        let room = mgr.rooms.get(ROOM_JID).unwrap();
        assert!(room.occupants.contains_key("bob"));
        let occ = &room.occupants["bob"];
        assert!(occ.available);
        assert_eq!(occ.role, Role::Participant);
        assert_eq!(occ.affiliation, Affiliation::Member);
    }

    #[test]
    fn on_presence_unavailable_removes_occupant() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        mgr.on_presence(&make_available_presence(ROOM_JID, "bob", "participant", "member"));
        assert!(mgr.rooms[ROOM_JID].occupants.contains_key("bob"));
        mgr.on_presence(&make_unavailable_presence(ROOM_JID, "bob"));
        assert!(!mgr.rooms[ROOM_JID].occupants.contains_key("bob"));
    }

    #[test]
    fn rooms_map_contains_joined_rooms() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        mgr.join_room("other@conference.example.com", "carol");
        assert_eq!(mgr.rooms[ROOM_JID].jid, ROOM_JID);
        assert_eq!(mgr.rooms[ROOM_JID].nickname, NICK);
        assert!(mgr.rooms.contains_key("other@conference.example.com"));
        assert!(!mgr.rooms.contains_key("nonexistent@conference.example.com"));
    }

    #[test]
    fn on_groupchat_message_parses_correctly() {
        let mgr = MucManager::new();
        let el = make_groupchat_message(ROOM_JID, "bob", "Hello, world!", "msg-001");
        let msg = mgr.on_groupchat_message(&el).expect("should parse message");
        assert_eq!(msg.room_jid, ROOM_JID);
        assert_eq!(msg.from_nick, "bob");
        assert_eq!(msg.body, "Hello, world!");
        assert_eq!(msg.id, "msg-001");
    }
}
