// Task P6.3 — XEP-0147 XMPP URI parser
// XEP reference: https://xmpp.org/extensions/xep-0147.html
//
// Format: `xmpp:{jid}?{action}[;{key}={value}]*`
//
// Pure functions — no I/O, no async.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// The action portion of an `xmpp:` URI.
#[derive(Debug, Clone, PartialEq)]
pub enum XmppUriAction {
    /// `?message` — send a message.
    Message,
    /// `?join` — join a MUC room.
    Join,
    /// `?subscribe` — send a presence subscribe request.
    Subscribe,
    /// `?remove` — remove from roster.
    Remove,
    /// Any other (or absent) action string.
    Unknown(String),
}

/// A parsed `xmpp:` URI.
#[derive(Debug, Clone, PartialEq)]
pub struct XmppUri {
    /// The JID that is the target of the URI.
    pub jid: String,
    /// The action from the query component.
    pub action: XmppUriAction,
    /// Additional query parameters (e.g. `"body"` for message, `"password"` for MUC join).
    pub params: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an `xmpp:` URI string.
///
/// Returns `None` if the string does not start with `"xmpp:"`.
///
/// # Examples
///
/// ```
/// # use xmpp_start::xmpp::modules::xmpp_uri::parse;
/// let uri = parse("xmpp:user@server?message;body=Hello").unwrap();
/// assert_eq!(uri.jid, "user@server");
/// ```
pub fn parse(uri: &str) -> Option<XmppUri> {
    let rest = uri.strip_prefix("xmpp:")?;

    // Split on `?` to separate JID from the query component.
    let (jid, query) = match rest.split_once('?') {
        Some((j, q)) => (j, Some(q)),
        None => (rest, None),
    };

    let jid = jid.to_string();

    let (action, params) = match query {
        None => (XmppUriAction::Unknown(String::new()), HashMap::new()),
        Some(q) => {
            // The query is semicolon-delimited: `action[;key=value]*`
            let mut parts = q.split(';');
            let action_str = parts.next().unwrap_or("");

            let action = match action_str {
                "message" => XmppUriAction::Message,
                "join" => XmppUriAction::Join,
                "subscribe" => XmppUriAction::Subscribe,
                "remove" => XmppUriAction::Remove,
                other => XmppUriAction::Unknown(other.to_string()),
            };

            let mut params = HashMap::new();
            for part in parts {
                if let Some((k, v)) = part.split_once('=') {
                    params.insert(k.to_string(), v.to_string());
                }
            }

            (action, params)
        }
    };

    Some(XmppUri {
        jid,
        action,
        params,
    })
}

/// Build an `xmpp:` URI string from its components.
///
/// If `params` is non-empty, they are appended as `key=value` pairs separated
/// by `;` after the action.
pub fn build(jid: &str, action: &XmppUriAction, params: &[(&str, &str)]) -> String {
    let action_str = match action {
        XmppUriAction::Message => "message",
        XmppUriAction::Join => "join",
        XmppUriAction::Subscribe => "subscribe",
        XmppUriAction::Remove => "remove",
        XmppUriAction::Unknown(s) => s.as_str(),
    };

    let mut uri = format!("xmpp:{}?{}", jid, action_str);

    for (k, v) in params {
        uri.push(';');
        uri.push_str(k);
        uri.push('=');
        uri.push_str(v);
    }

    uri
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1 -----------------------------------------------------------------------
    #[test]
    fn parse_bare_jid() {
        let uri = parse("xmpp:user@example.org").expect("should parse");
        assert_eq!(uri.jid, "user@example.org");
        assert_eq!(uri.action, XmppUriAction::Unknown(String::new()));
        assert!(uri.params.is_empty());
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn parse_message_action() {
        let uri = parse("xmpp:user@example.org?message").expect("should parse");
        assert_eq!(uri.jid, "user@example.org");
        assert_eq!(uri.action, XmppUriAction::Message);
        assert!(uri.params.is_empty());
    }

    // 3 -----------------------------------------------------------------------
    #[test]
    fn parse_join_action() {
        let uri = parse("xmpp:room@muc.example.org?join").expect("should parse");
        assert_eq!(uri.jid, "room@muc.example.org");
        assert_eq!(uri.action, XmppUriAction::Join);
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn parse_params() {
        let uri = parse("xmpp:user@example.org?message;body=Hello%20World;thread=t1")
            .expect("should parse");
        assert_eq!(uri.action, XmppUriAction::Message);
        assert_eq!(uri.params.get("body"), Some(&"Hello%20World".to_string()));
        assert_eq!(uri.params.get("thread"), Some(&"t1".to_string()));
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn returns_none_for_non_xmpp_uri() {
        assert!(parse("https://example.org").is_none());
        assert!(parse("mailto:user@example.org").is_none());
        assert!(parse("").is_none());
    }

    // 6 -----------------------------------------------------------------------
    #[test]
    fn build_round_trips() {
        let original = "xmpp:user@example.org?message;body=Hi;thread=abc";
        let parsed = parse(original).expect("should parse");

        // Collect params in a stable order to build deterministically.
        let mut param_pairs: Vec<(&str, &str)> = parsed
            .params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        param_pairs.sort_by_key(|(k, _)| *k);

        let rebuilt = build(&parsed.jid, &parsed.action, &param_pairs);
        let re_parsed = parse(&rebuilt).expect("rebuilt URI should parse");

        assert_eq!(re_parsed.jid, parsed.jid);
        assert_eq!(re_parsed.action, parsed.action);
        assert_eq!(re_parsed.params, parsed.params);
    }
}
