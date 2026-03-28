// Task P3.4 — XEP-0048 Room Bookmarks
// XEP reference: https://xmpp.org/extensions/xep-0048.html
//
// This is a pure data module — no I/O, no async.
// Supports private XML storage (XEP-0048 §3.2).

use tokio_xmpp::minidom::Element;

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

    /// Builds a private-XML-get IQ to fetch bookmarks (XEP-0048 §3.2).
    ///
    /// ```xml
    /// <iq type="get" id="{id}" xmlns="jabber:client">
    ///   <query xmlns="jabber:iq:private">
    ///     <storage xmlns="storage:bookmarks"/>
    ///   </query>
    /// </iq>
    /// ```
    #[allow(dead_code)]
    pub fn build_fetch_iq(&self, id: &str) -> Element {
        let storage = Element::builder("storage", NS_BOOKMARKS).build();
        let query = Element::builder("query", NS_PRIVATE)
            .append(storage)
            .build();
        Element::builder("iq", "jabber:client")
            .attr("type", "get")
            .attr("id", id)
            .append(query)
            .build()
    }

    /// Builds a private-XML-set IQ that persists the current bookmark list
    /// (XEP-0048 §3.2).  Uses `self.bookmarks` as the authoritative state.
    ///
    /// ```xml
    /// <iq type="set" id="{id}" xmlns="jabber:client">
    ///   <query xmlns="jabber:iq:private">
    ///     <storage xmlns="storage:bookmarks">
    ///       <conference jid="…" name="…" autojoin="true">
    ///         <nick>…</nick>
    ///       </conference>
    ///     </storage>
    ///   </query>
    /// </iq>
    /// ```
    // TODO: wire into persist-after-add bookmark flow
    #[allow(dead_code)]
    pub fn build_save_iq(&self, id: &str) -> Element {
        let mut storage_builder = Element::builder("storage", NS_BOOKMARKS);

        for bm in &self.bookmarks {
            let autojoin_val = if bm.autojoin { "true" } else { "false" };
            let mut conf_builder = Element::builder("conference", NS_BOOKMARKS)
                .attr("jid", &bm.jid)
                .attr("autojoin", autojoin_val);

            if let Some(ref name) = bm.name {
                conf_builder = conf_builder.attr("name", name.as_str());
            }

            let mut conf = conf_builder.build();

            if let Some(ref nick) = bm.nick {
                let mut nick_el = Element::builder("nick", NS_BOOKMARKS).build();
                nick_el.append_text_node(nick.as_str());
                conf.append_child(nick_el);
            }

            if let Some(ref pw) = bm.password {
                let mut pw_el = Element::builder("password", NS_BOOKMARKS).build();
                pw_el.append_text_node(pw.as_str());
                conf.append_child(pw_el);
            }

            storage_builder = storage_builder.append(conf);
        }

        let query = Element::builder("query", NS_PRIVATE)
            .append(storage_builder.build())
            .build();

        Element::builder("iq", "jabber:client")
            .attr("type", "set")
            .attr("id", id)
            .append(query)
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

    // 1. parse_bookmarks_from_iq parses <conference> elements correctly
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

    // 2. parse_bookmarks_from_iq: accepts full <iq> wrapper with <query><storage>.
    #[test]
    fn parse_bookmarks_from_iq_accepts_full_iq_wrapper() {
        let conf = Element::builder("conference", NS_BOOKMARKS)
            .attr("jid", "wrapped@conference.example.com")
            .attr("autojoin", "true")
            .build();

        let storage = Element::builder("storage", NS_BOOKMARKS)
            .append(conf)
            .build();

        let query = Element::builder("query", NS_PRIVATE)
            .append(storage)
            .build();

        let iq = Element::builder("iq", "jabber:client")
            .attr("type", "result")
            .attr("id", "bm-get-1")
            .append(query)
            .build();

        let bookmarks = BookmarkManager::parse_bookmarks_from_iq(&iq);
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].jid, "wrapped@conference.example.com");
        assert!(bookmarks[0].autojoin);
    }

    // 3. build_fetch_iq: produces a type="get" IQ with the correct structure.
    #[test]
    fn build_fetch_iq_has_correct_structure() {
        let mgr = BookmarkManager::new();
        let iq = mgr.build_fetch_iq("bm-fetch-test");

        assert_eq!(iq.name(), "iq");
        assert_eq!(iq.attr("type"), Some("get"));
        assert_eq!(iq.attr("id"), Some("bm-fetch-test"));

        let query = iq
            .get_child("query", NS_PRIVATE)
            .expect("<query> must exist");
        let storage = query
            .get_child("storage", NS_BOOKMARKS)
            .expect("<storage> must exist");
        assert_eq!(storage.ns(), NS_BOOKMARKS);
    }

    // 4. build_save_iq: serialises bookmarks back into a type="set" IQ.
    #[test]
    fn build_save_iq_serialises_bookmarks() {
        let mut mgr = BookmarkManager::new();
        mgr.set_bookmarks(vec![
            Bookmark {
                jid: "dev@conference.example.com".to_string(),
                name: Some("Dev Room".to_string()),
                autojoin: true,
                nick: Some("Alice".to_string()),
                password: None,
            },
            Bookmark {
                jid: "lurk@conference.example.com".to_string(),
                name: None,
                autojoin: false,
                nick: None,
                password: None,
            },
        ]);

        let iq = mgr.build_save_iq("bm-save-1");

        assert_eq!(iq.attr("type"), Some("set"));
        assert_eq!(iq.attr("id"), Some("bm-save-1"));

        let storage = iq
            .get_child("query", NS_PRIVATE)
            .and_then(|q| q.get_child("storage", NS_BOOKMARKS))
            .expect("<storage> must exist");

        let confs: Vec<&Element> = storage
            .children()
            .filter(|c| c.name() == "conference")
            .collect();

        assert_eq!(confs.len(), 2);
        assert_eq!(confs[0].attr("jid"), Some("dev@conference.example.com"));
        assert_eq!(confs[0].attr("name"), Some("Dev Room"));
        assert_eq!(confs[0].attr("autojoin"), Some("true"));
        assert_eq!(
            confs[0]
                .get_child("nick", NS_BOOKMARKS)
                .map(tokio_xmpp::minidom::Element::text)
                .as_deref(),
            Some("Alice")
        );

        assert_eq!(confs[1].attr("jid"), Some("lurk@conference.example.com"));
        assert_eq!(confs[1].attr("autojoin"), Some("false"));
        assert!(confs[1].get_child("nick", NS_BOOKMARKS).is_none());
    }

    // 5. build_save_iq + parse round-trip: what we save we can parse back.
    #[test]
    fn save_iq_roundtrips_through_parse() {
        let original = vec![
            Bookmark {
                jid: "alpha@conference.example.com".to_string(),
                name: Some("Alpha".to_string()),
                autojoin: true,
                nick: Some("Bot".to_string()),
                password: Some("secret".to_string()),
            },
            Bookmark {
                jid: "beta@conference.example.com".to_string(),
                name: None,
                autojoin: false,
                nick: None,
                password: None,
            },
        ];

        let mut mgr = BookmarkManager::new();
        mgr.set_bookmarks(original.clone());
        let iq = mgr.build_save_iq("rt-1");

        let parsed = BookmarkManager::parse_bookmarks_from_iq(&iq);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].jid, original[0].jid);
        assert_eq!(parsed[0].name, original[0].name);
        assert_eq!(parsed[0].autojoin, original[0].autojoin);
        assert_eq!(parsed[0].nick, original[0].nick);
        assert_eq!(parsed[0].password, original[0].password);
        assert_eq!(parsed[1].jid, original[1].jid);
        assert!(!parsed[1].autojoin);
    }
}
