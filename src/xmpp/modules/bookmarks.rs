#![allow(dead_code)]
// Task P3.4 — XEP-0048 Room Bookmarks
// XEP reference: https://xmpp.org/extensions/xep-0048.html
//
// This is a pure data module — no I/O, no async.
// Supports private XML storage (XEP-0048 §3.2).

use tokio_xmpp::minidom::Element;
use tokio_xmpp::minidom::Node;

const NS_BOOKMARKS: &str = "storage:bookmarks";
const NS_PRIVATE: &str = "jabber:iq:private";

/// A single XMPP MUC room bookmark (XEP-0048).
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    /// MUC room JID (e.g. `room@conference.example.org`).
    pub jid: String,
    /// Human-readable room name.
    pub name: Option<String>,
    /// Join this room automatically on login.
    pub autojoin: bool,
    /// Preferred nickname inside the room.
    pub nick: Option<String>,
    /// Room password, if required.
    pub password: Option<String>,
}

/// Manages the local set of room bookmarks and builds/parses the XEP-0048
/// private XML storage stanzas.
pub struct BookmarkManager {
    bookmarks: Vec<Bookmark>,
}

impl BookmarkManager {
    /// Creates a new, empty manager.
    pub fn new() -> Self {
        Self {
            bookmarks: Vec::new(),
        }
    }

    /// Replaces the entire bookmark list with `bookmarks`.
    pub fn set_bookmarks(&mut self, bookmarks: Vec<Bookmark>) {
        self.bookmarks = bookmarks;
    }

    /// Inserts `bookmark`, or replaces the existing entry with the same JID
    /// (case-sensitive comparison).
    pub fn add(&mut self, bookmark: Bookmark) {
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.jid == bookmark.jid) {
            *existing = bookmark;
        } else {
            self.bookmarks.push(bookmark);
        }
    }

    /// Removes the bookmark whose JID equals `jid`.  No-op if not found.
    pub fn remove(&mut self, jid: &str) {
        self.bookmarks.retain(|b| b.jid != jid);
    }

    /// Returns a reference to the bookmark with the given `jid`, if any.
    pub fn get(&self, jid: &str) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|b| b.jid == jid)
    }

    /// Returns all stored bookmarks.
    pub fn all(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Returns all bookmarks that have `autojoin` set to `true`.
    pub fn autojoin_rooms(&self) -> Vec<&Bookmark> {
        self.bookmarks.iter().filter(|b| b.autojoin).collect()
    }

    /// Builds an `<iq type='set'>` stanza that stores the current bookmarks
    /// via private XML storage (XEP-0048 §3.2).
    ///
    /// ```xml
    /// <iq type='set' id='bookmarks-1'>
    ///   <query xmlns='jabber:iq:private'>
    ///     <storage xmlns='storage:bookmarks'>
    ///       <conference jid='room@server' name='Room Name' autojoin='true'>
    ///         <nick>MyNick</nick>
    ///       </conference>
    ///     </storage>
    ///   </query>
    /// </iq>
    /// ```
    pub fn build_publish_iq(&self) -> Element {
        let mut storage = Element::builder("storage", NS_BOOKMARKS).build();

        for bm in &self.bookmarks {
            let mut conf = Element::builder("conference", NS_BOOKMARKS)
                .attr("jid", bm.jid.clone())
                .attr("autojoin", if bm.autojoin { "true" } else { "false" });

            if let Some(name) = &bm.name {
                conf = conf.attr("name", name.clone());
            }

            let mut conf_el = conf.build();

            if let Some(nick) = &bm.nick {
                let mut nick_el = Element::builder("nick", NS_BOOKMARKS).build();
                nick_el.append_text_node(nick.clone());
                conf_el.append_child(nick_el);
            }

            if let Some(password) = &bm.password {
                let mut pw_el = Element::builder("password", NS_BOOKMARKS).build();
                pw_el.append_text_node(password.clone());
                conf_el.append_child(pw_el);
            }

            storage.append_child(conf_el);
        }

        let query = Element::builder("query", NS_PRIVATE)
            .append(Node::Element(storage))
            .build();

        Element::builder("iq", "jabber:client")
            .attr("type", "set")
            .attr("id", "bookmarks-1")
            .append(Node::Element(query))
            .build()
    }

    /// Parses bookmarks from a `<storage xmlns='storage:bookmarks'>` element.
    ///
    /// Tolerates missing optional attributes/children gracefully.
    pub fn parse_bookmarks_from_iq(el: &Element) -> Vec<Bookmark> {
        // Accept either a bare <storage> element or a full <iq> wrapping
        // <query><storage>…</storage></query>.
        let storage = if el.name() == "storage" && el.ns() == NS_BOOKMARKS {
            el
        } else if let Some(query) = el.get_child("query", NS_PRIVATE) {
            match query.get_child("storage", NS_BOOKMARKS) {
                Some(s) => s,
                None => return Vec::new(),
            }
        } else {
            return Vec::new();
        };

        storage
            .children()
            .filter(|child| child.name() == "conference")
            .map(|conf| {
                let jid = conf.attr("jid").unwrap_or("").to_string();
                let name = conf.attr("name").map(std::string::ToString::to_string);
                let autojoin = matches!(conf.attr("autojoin"), Some("true") | Some("1"));
                let nick = conf
                    .get_child("nick", NS_BOOKMARKS)
                    .map(tokio_xmpp::minidom::Element::text);
                let password = conf
                    .get_child("password", NS_BOOKMARKS)
                    .map(tokio_xmpp::minidom::Element::text);

                Bookmark {
                    jid,
                    name,
                    autojoin,
                    nick,
                    password,
                }
            })
            .collect()
    }
}

impl Default for BookmarkManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bookmark(jid: &str) -> Bookmark {
        Bookmark {
            jid: jid.to_string(),
            name: Some(format!("Room {jid}")),
            autojoin: false,
            nick: None,
            password: None,
        }
    }

    // 1. New manager starts empty
    #[test]
    fn bookmark_manager_new_is_empty() {
        let mgr = BookmarkManager::new();
        assert!(mgr.all().is_empty());
    }

    // 2. Adding a bookmark inserts it
    #[test]
    fn add_bookmark_inserts() {
        let mut mgr = BookmarkManager::new();
        mgr.add(make_bookmark("room@server"));
        assert_eq!(mgr.all().len(), 1);
        assert_eq!(mgr.all()[0].jid, "room@server");
    }

    // 3. Adding a bookmark with the same JID replaces the existing one (upsert)
    #[test]
    fn add_bookmark_upserts_by_jid() {
        let mut mgr = BookmarkManager::new();
        mgr.add(make_bookmark("room@server"));

        let updated = Bookmark {
            jid: "room@server".to_string(),
            name: Some("Updated".to_string()),
            autojoin: true,
            nick: Some("Nick".to_string()),
            password: None,
        };
        mgr.add(updated);

        assert_eq!(mgr.all().len(), 1, "upsert must not duplicate");
        assert_eq!(mgr.all()[0].name, Some("Updated".to_string()));
        assert!(mgr.all()[0].autojoin);
    }

    // 4. Removing a bookmark
    #[test]
    fn remove_bookmark() {
        let mut mgr = BookmarkManager::new();
        mgr.add(make_bookmark("room@server"));
        mgr.add(make_bookmark("other@server"));
        mgr.remove("room@server");

        assert_eq!(mgr.all().len(), 1);
        assert_eq!(mgr.all()[0].jid, "other@server");
    }

    // 5. autojoin_rooms filters correctly
    #[test]
    fn autojoin_rooms_filter() {
        let mut mgr = BookmarkManager::new();
        mgr.add(Bookmark {
            jid: "a@server".to_string(),
            name: None,
            autojoin: true,
            nick: None,
            password: None,
        });
        mgr.add(Bookmark {
            jid: "b@server".to_string(),
            name: None,
            autojoin: false,
            nick: None,
            password: None,
        });
        mgr.add(Bookmark {
            jid: "c@server".to_string(),
            name: None,
            autojoin: true,
            nick: None,
            password: None,
        });

        let aj = mgr.autojoin_rooms();
        assert_eq!(aj.len(), 2);
        assert!(aj.iter().all(|b| b.autojoin));
    }

    // 6. get returns the correct bookmark by JID
    #[test]
    fn get_bookmark_by_jid() {
        let mut mgr = BookmarkManager::new();
        mgr.add(make_bookmark("find-me@server"));
        mgr.add(make_bookmark("other@server"));

        let found = mgr.get("find-me@server");
        assert!(found.is_some());
        assert_eq!(found.unwrap().jid, "find-me@server");

        assert!(mgr.get("missing@server").is_none());
    }

    // 7. build_publish_iq produces an element containing the storage element
    #[test]
    fn build_publish_iq_contains_storage_element() {
        let mut mgr = BookmarkManager::new();
        mgr.add(Bookmark {
            jid: "room@conference.example".to_string(),
            name: Some("Example Room".to_string()),
            autojoin: true,
            nick: Some("TestNick".to_string()),
            password: None,
        });

        let iq = mgr.build_publish_iq();

        // Top-level element must be <iq type='set'>
        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("type"), Some("set"));

        // Must contain <query xmlns='jabber:iq:private'>
        let query = iq.get_child("query", NS_PRIVATE);
        assert!(query.is_some(), "<query> element missing");

        // Must contain <storage xmlns='storage:bookmarks'>
        let storage = query.unwrap().get_child("storage", NS_BOOKMARKS);
        assert!(storage.is_some(), "<storage> element missing");

        // Storage must contain the conference child
        let storage = storage.unwrap();
        let conf = storage.get_child("conference", NS_BOOKMARKS);
        assert!(conf.is_some(), "<conference> element missing");

        let conf = conf.unwrap();
        assert_eq!(conf.attr("jid"), Some("room@conference.example"));
        assert_eq!(conf.attr("autojoin"), Some("true"));
        assert_eq!(conf.attr("name"), Some("Example Room"));

        // Nick child must be present
        let nick = conf.get_child("nick", NS_BOOKMARKS);
        assert!(nick.is_some(), "<nick> element missing");
        assert_eq!(nick.unwrap().text(), "TestNick");
    }

    // 8. parse_bookmarks_from_iq parses <conference> elements correctly
    #[test]
    fn parse_bookmarks_from_iq_parses_conference() {
        // Build a <storage> element directly to feed the parser.
        let mut storage = Element::builder("storage", NS_BOOKMARKS).build();

        let mut conf1 = Element::builder("conference", NS_BOOKMARKS)
            .attr("jid", "room1@server")
            .attr("name", "Room One")
            .attr("autojoin", "true")
            .build();
        let mut nick_el = Element::builder("nick", NS_BOOKMARKS).build();
        nick_el.append_text_node("Alice");
        conf1.append_child(nick_el);
        storage.append_child(conf1);

        let conf2 = Element::builder("conference", NS_BOOKMARKS)
            .attr("jid", "room2@server")
            .attr("autojoin", "1")
            .build();
        storage.append_child(conf2);

        let conf3 = Element::builder("conference", NS_BOOKMARKS)
            .attr("jid", "room3@server")
            .build();
        storage.append_child(conf3);

        let bookmarks = BookmarkManager::parse_bookmarks_from_iq(&storage);
        assert_eq!(bookmarks.len(), 3);

        let b1 = &bookmarks[0];
        assert_eq!(b1.jid, "room1@server");
        assert_eq!(b1.name, Some("Room One".to_string()));
        assert!(b1.autojoin);
        assert_eq!(b1.nick, Some("Alice".to_string()));

        let b2 = &bookmarks[1];
        assert_eq!(b2.jid, "room2@server");
        assert!(b2.autojoin, "autojoin='1' should parse as true");

        let b3 = &bookmarks[2];
        assert_eq!(b3.jid, "room3@server");
        assert!(!b3.autojoin);
        assert!(b3.nick.is_none());
    }
}
