#![allow(dead_code)]
// Task P3.1 — XEP-0045 Multi-User Chat core module
// XEP reference: https://xmpp.org/extensions/xep-0045.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - build join/leave presence stanzas
//   - build outbound groupchat message stanzas
//   - maintain per-room occupant lists from incoming presence
//   - parse incoming groupchat messages

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

const NS_MUC: &str = "http://jabber.org/protocol/muc";
const NS_MUC_ADMIN: &str = "http://jabber.org/protocol/muc#admin";
const NS_CLIENT: &str = "jabber:client";

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
    /// Map of nick → Occupant.
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
///
/// Holds the set of rooms we have joined and exposes methods to build outbound
/// stanzas and process inbound presence/message stanzas.
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

    /// Build a join `<presence>` stanza and record the room.
    ///
    /// Returns the stanza that must be sent to the server:
    /// ```xml
    /// <presence to='room@conf/nick'>
    ///   <x xmlns='http://jabber.org/protocol/muc'/>
    /// </presence>
    /// ```
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

    /// Build an unavailable `<presence>` stanza and remove the room.
    ///
    /// Returns `None` if we are not tracking the given room JID.
    pub fn leave_room(&mut self, room_jid: &str) -> Option<Element> {
        let room = self.rooms.remove(room_jid)?;

        let el = Element::builder("presence", NS_CLIENT)
            .attr("to", format!("{}/{}", room_jid, room.nickname))
            .attr("type", "unavailable")
            .build();

        Some(el)
    }

    /// Process an incoming `<presence>` stanza and update the occupant list.
    ///
    /// The `from` attribute must be a full room JID (`room@conf/nick`).
    /// Unavailable presence removes the occupant; available presence upserts.
    pub fn on_presence(&mut self, el: &Element) {
        let from = match el.attr("from") {
            Some(f) => f,
            None => return,
        };

        // Split "room@conf/nick" → ("room@conf", "nick")
        let (room_jid, nick) = match from.split_once('/') {
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

        // Parse <x xmlns='muc#user'> for role/affiliation/jid
        let mut role = Role::None;
        let mut affiliation = Affiliation::None;
        let mut real_jid: Option<String> = None;

        const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";

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

    /// Parse an incoming `<message type='groupchat'>` stanza.
    ///
    /// Returns `None` if the stanza is not a groupchat message or has no body.
    pub fn on_groupchat_message(&self, el: &Element) -> Option<MucMessage> {
        if el.name() != "message" {
            return None;
        }
        if el.attr("type") != Some("groupchat") {
            return None;
        }

        let from = el.attr("from")?;
        let (room_jid, from_nick) = from.split_once('/')?;

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

    /// Build a `<message type='groupchat'>` stanza for the given room.
    ///
    /// Assigns a fresh UUID as the stanza `id`.
    pub fn send_message(&self, room_jid: &str, body: &str) -> Element {
        let id = Uuid::new_v4().to_string();
        Element::builder("message", NS_CLIENT)
            .attr("to", room_jid)
            .attr("type", "groupchat")
            .attr("id", id)
            .append(Element::builder("body", NS_CLIENT).append(body).build())
            .build()
    }

    /// Returns all tracked rooms.
    pub fn rooms(&self) -> &HashMap<String, MucRoom> {
        &self.rooms
    }

    /// Returns a reference to a specific room by bare JID.
    pub fn get_room(&self, jid: &str) -> Option<&MucRoom> {
        self.rooms.get(jid)
    }
}

impl Default for MucManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ModerationManager
// ---------------------------------------------------------------------------

/// XEP-0045 moderation stanza builder.
///
/// Stateless — all methods are pure functions that produce IQ stanzas ready
/// to be sent to the server.  The caller is responsible for writing the
/// returned `Element` to the XMPP stream.
pub struct ModerationManager;

impl ModerationManager {
    /// Build an IQ stanza that sets `role` on an `<item nick='…'>`.
    fn role_iq(room_jid: &str, nick: &str, role: &str, reason: Option<&str>) -> Element {
        let mut item = Element::builder("item", NS_MUC_ADMIN)
            .attr("nick", nick)
            .attr("role", role);

        if let Some(r) = reason {
            item = item.append(Element::builder("reason", NS_MUC_ADMIN).append(r).build());
        }

        let query = Element::builder("query", NS_MUC_ADMIN)
            .append(item.build())
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("to", room_jid)
            .attr("type", "set")
            .attr("id", Uuid::new_v4().to_string())
            .append(query)
            .build()
    }

    /// Build an IQ stanza that sets `affiliation` on an `<item jid='…'>`.
    fn affiliation_iq(
        room_jid: &str,
        jid: &str,
        affiliation: &str,
        reason: Option<&str>,
    ) -> Element {
        let mut item = Element::builder("item", NS_MUC_ADMIN)
            .attr("jid", jid)
            .attr("affiliation", affiliation);

        if let Some(r) = reason {
            item = item.append(Element::builder("reason", NS_MUC_ADMIN).append(r).build());
        }

        let query = Element::builder("query", NS_MUC_ADMIN)
            .append(item.build())
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("to", room_jid)
            .attr("type", "set")
            .attr("id", Uuid::new_v4().to_string())
            .append(query)
            .build()
    }

    /// Kick a user by nick (sets role to `none`).
    pub fn kick(room_jid: &str, nick: &str, reason: Option<&str>) -> Element {
        Self::role_iq(room_jid, nick, "none", reason)
    }

    /// Ban a user by real JID (sets affiliation to `outcast`).
    pub fn ban(room_jid: &str, jid: &str, reason: Option<&str>) -> Element {
        Self::affiliation_iq(room_jid, jid, "outcast", reason)
    }

    /// Mute a user by nick (sets role to `visitor`).
    pub fn mute(room_jid: &str, nick: &str, reason: Option<&str>) -> Element {
        Self::role_iq(room_jid, nick, "visitor", reason)
    }

    /// Unmute a user by nick (sets role to `participant`).
    pub fn unmute(room_jid: &str, nick: &str, reason: Option<&str>) -> Element {
        Self::role_iq(room_jid, nick, "participant", reason)
    }

    /// Grant moderator role to a nick.
    pub fn grant_moderator(room_jid: &str, nick: &str) -> Element {
        Self::role_iq(room_jid, nick, "moderator", None)
    }

    /// Revoke moderator role from a nick (back to participant).
    pub fn revoke_moderator(room_jid: &str, nick: &str) -> Element {
        Self::role_iq(room_jid, nick, "participant", None)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM_JID: &str = "general@conference.example.com";
    const NICK: &str = "alice";
    const NS_MUC_USER: &str = "http://jabber.org/protocol/muc#user";

    fn make_available_presence(
        room_jid: &str,
        nick: &str,
        role: &str,
        affiliation: &str,
    ) -> Element {
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

    // 1. New manager is empty
    #[test]
    fn muc_manager_new_is_empty() {
        let mgr = MucManager::new();
        assert!(mgr.rooms().is_empty());
    }

    // 2. join_room builds correct presence stanza
    #[test]
    fn join_room_builds_presence_stanza() {
        let mut mgr = MucManager::new();
        let el = mgr.join_room(ROOM_JID, NICK);

        assert_eq!(el.name(), "presence");
        let expected_to = format!("{}/{}", ROOM_JID, NICK);
        assert_eq!(el.attr("to"), Some(expected_to.as_str()));

        // Must contain <x xmlns='muc'/>
        let x = el.children().find(|c| c.name() == "x");
        assert!(x.is_some(), "presence must contain <x/>");
        assert_eq!(x.unwrap().ns(), NS_MUC);
    }

    // 3. join_room registers the room in the manager
    #[test]
    fn join_room_adds_room_to_manager() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        assert!(mgr.get_room(ROOM_JID).is_some());
        assert_eq!(mgr.get_room(ROOM_JID).unwrap().nickname, NICK);
    }

    // 4. leave_room removes the room from the manager
    #[test]
    fn leave_room_removes_room() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        assert!(mgr.get_room(ROOM_JID).is_some());

        mgr.leave_room(ROOM_JID);
        assert!(mgr.get_room(ROOM_JID).is_none());
    }

    // 5. leave_room returns unavailable presence with correct attributes
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

    // 6. send_message builds a groupchat stanza with body
    #[test]
    fn send_message_builds_groupchat_stanza() {
        let mgr = MucManager::new();
        let el = mgr.send_message(ROOM_JID, "hello world");

        assert_eq!(el.name(), "message");
        assert_eq!(el.attr("type"), Some("groupchat"));
        assert_eq!(el.attr("to"), Some(ROOM_JID));
        assert!(el.attr("id").is_some(), "message must have an id");

        let body = el.children().find(|c| c.name() == "body");
        assert!(body.is_some(), "message must contain <body/>");
        assert_eq!(body.unwrap().text(), "hello world");
    }

    // 7. on_presence adds occupant to the room
    #[test]
    fn on_presence_adds_occupant() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);

        let presence = make_available_presence(ROOM_JID, "bob", "participant", "member");
        mgr.on_presence(&presence);

        let room = mgr.get_room(ROOM_JID).unwrap();
        assert!(room.occupants.contains_key("bob"));
        let occ = &room.occupants["bob"];
        assert!(occ.available);
        assert_eq!(occ.role, Role::Participant);
        assert_eq!(occ.affiliation, Affiliation::Member);
    }

    // 8. on_presence with type='unavailable' removes occupant
    #[test]
    fn on_presence_unavailable_removes_occupant() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);

        // Add bob first
        mgr.on_presence(&make_available_presence(
            ROOM_JID,
            "bob",
            "participant",
            "member",
        ));
        assert!(mgr
            .get_room(ROOM_JID)
            .unwrap()
            .occupants
            .contains_key("bob"));

        // Now bob leaves
        mgr.on_presence(&make_unavailable_presence(ROOM_JID, "bob"));
        assert!(!mgr
            .get_room(ROOM_JID)
            .unwrap()
            .occupants
            .contains_key("bob"));
    }

    // 9. get_room returns the correct room
    #[test]
    fn get_room_returns_room() {
        let mut mgr = MucManager::new();
        mgr.join_room(ROOM_JID, NICK);
        mgr.join_room("other@conference.example.com", "carol");

        let room = mgr.get_room(ROOM_JID).unwrap();
        assert_eq!(room.jid, ROOM_JID);
        assert_eq!(room.nickname, NICK);

        assert!(mgr.get_room("other@conference.example.com").is_some());
        assert!(mgr.get_room("nonexistent@conference.example.com").is_none());
    }

    // 10. on_groupchat_message parses body, nick, and room_jid correctly
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

    // ---------------------------------------------------------------------------
    // ModerationManager tests
    // ---------------------------------------------------------------------------

    const NS_MUC_ADMIN: &str = "http://jabber.org/protocol/muc#admin";

    /// Returns the first `<item>` inside `<query xmlns='muc#admin'>`.
    fn get_admin_item(iq: &Element) -> &Element {
        let query = iq
            .children()
            .find(|c| c.name() == "query" && c.ns() == NS_MUC_ADMIN)
            .expect("iq must contain <query xmlns='muc#admin'>");
        query
            .children()
            .find(|c| c.name() == "item")
            .expect("query must contain <item>")
    }

    // 11. kick_builds_correct_iq
    #[test]
    fn kick_builds_correct_iq() {
        let iq = ModerationManager::kick(ROOM_JID, "mallory", None);

        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("to"), Some(ROOM_JID));
        assert_eq!(iq.attr("type"), Some("set"));
        assert!(iq.attr("id").is_some(), "iq must have an id");

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("nick"), Some("mallory"));
        assert_eq!(item.attr("role"), Some("none"));
    }

    // 12. kick_with_reason_includes_reason_element
    #[test]
    fn kick_with_reason_includes_reason_element() {
        let iq = ModerationManager::kick(ROOM_JID, "mallory", Some("disruptive behaviour"));

        let item = get_admin_item(&iq);
        let reason_el = item
            .children()
            .find(|c| c.name() == "reason")
            .expect("item must contain <reason>");
        assert_eq!(reason_el.text(), "disruptive behaviour");
    }

    // 13. ban_builds_correct_iq
    #[test]
    fn ban_builds_correct_iq() {
        let iq = ModerationManager::ban(ROOM_JID, "troll@example.com", None);

        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("to"), Some(ROOM_JID));
        assert_eq!(iq.attr("type"), Some("set"));

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("jid"), Some("troll@example.com"));
        assert_eq!(item.attr("affiliation"), Some("outcast"));
    }

    // 14. ban_with_reason_includes_reason_element
    #[test]
    fn ban_with_reason_includes_reason_element() {
        let iq = ModerationManager::ban(ROOM_JID, "troll@example.com", Some("spamming"));

        let item = get_admin_item(&iq);
        let reason_el = item
            .children()
            .find(|c| c.name() == "reason")
            .expect("item must contain <reason>");
        assert_eq!(reason_el.text(), "spamming");
    }

    // 15. mute_sets_role_visitor
    #[test]
    fn mute_sets_role_visitor() {
        let iq = ModerationManager::mute(ROOM_JID, "chatterbox", None);

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("nick"), Some("chatterbox"));
        assert_eq!(item.attr("role"), Some("visitor"));
    }

    // 16. unmute_sets_role_participant
    #[test]
    fn unmute_sets_role_participant() {
        let iq = ModerationManager::unmute(ROOM_JID, "chatterbox", None);

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("nick"), Some("chatterbox"));
        assert_eq!(item.attr("role"), Some("participant"));
    }

    // 17. grant_moderator_sets_role_moderator
    #[test]
    fn grant_moderator_sets_role_moderator() {
        let iq = ModerationManager::grant_moderator(ROOM_JID, "trusted");

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("nick"), Some("trusted"));
        assert_eq!(item.attr("role"), Some("moderator"));
    }

    // 18. revoke_moderator_sets_role_participant
    #[test]
    fn revoke_moderator_sets_role_participant() {
        let iq = ModerationManager::revoke_moderator(ROOM_JID, "trusted");

        let item = get_admin_item(&iq);
        assert_eq!(item.attr("nick"), Some("trusted"));
        assert_eq!(item.attr("role"), Some("participant"));
    }
}
