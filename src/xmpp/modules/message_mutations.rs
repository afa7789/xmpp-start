// Task P5.3 — XEP-0444 Reactions, XEP-0308 Last Message Correction, XEP-0424 Retraction
//
// References:
//   XEP-0444: https://xmpp.org/extensions/xep-0444.html
//   XEP-0308: https://xmpp.org/extensions/xep-0308.html
//   XEP-0424: https://xmpp.org/extensions/xep-0424.html
//
// Pure state machine — no I/O, no async.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_REACTIONS};

const NS_CORRECTION: &str = "urn:xmpp:message-correct:0";
const NS_RETRACTION: &str = "urn:xmpp:message-retract:1";
const NS_FASTEN: &str = "urn:xmpp:fasten:0";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// XEP-0444: a reaction update from a sender targeting a specific message.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionUpdate {
    /// The message ID being reacted to.
    pub target_id: String,
    /// JID of the sender.
    pub from_jid: String,
    /// Current full set of emoji reactions from this sender (empty = all removed).
    pub emojis: Vec<String>,
}

/// XEP-0308: a last message correction replacing a previous message body.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// The original message ID being corrected (`replace/@id`).
    pub target_id: String,
    pub from_jid: String,
    pub new_body: String,
}

/// XEP-0424: a retraction of a previously sent message.
#[derive(Debug, Clone, PartialEq)]
pub struct Retraction {
    /// The origin-id of the message to retract.
    pub target_id: String,
    pub from_jid: String,
}

// ---------------------------------------------------------------------------
// MutationManager
// ---------------------------------------------------------------------------

/// Builds and parses stanzas for XEP-0444, XEP-0308, and XEP-0424.
///
/// All methods are pure: no I/O, no async, no persistent state beyond
/// construction.
pub struct MutationManager;

impl Default for MutationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MutationManager {
    /// Creates a new manager.
    pub fn new() -> Self {
        Self
    }

    // ---- XEP-0444 Reactions ------------------------------------------------

    /// Build a reactions message to send.
    ///
    /// `emojis` is the full new set for this sender.  An empty slice removes
    /// all reactions.
    ///
    /// ```xml
    /// <message to="{to}" type="chat" xmlns="jabber:client">
    ///   <reactions xmlns="urn:xmpp:reactions:0" id="{target_id}">
    ///     <reaction>👍</reaction>
    ///   </reactions>
    /// </message>
    /// ```
    pub fn build_reaction(
        &self,
        to_jid: &str,
        target_message_id: &str,
        emojis: &[&str],
    ) -> Element {
        let mut reactions_builder =
            Element::builder("reactions", NS_REACTIONS).attr("id", target_message_id);

        for emoji in emojis {
            let reaction_el = Element::builder("reaction", NS_REACTIONS)
                .append(*emoji)
                .build();
            reactions_builder = reactions_builder.append(reaction_el);
        }

        Element::builder("message", NS_CLIENT)
            .attr("to", to_jid)
            .attr("type", "chat")
            .append(reactions_builder.build())
            .build()
    }

    /// Parse an incoming reactions message.
    ///
    /// Returns `Some(ReactionUpdate)` when the message contains a
    /// `<reactions xmlns='urn:xmpp:reactions:0'>` child, `None` otherwise.
    pub fn parse_reaction(&self, from_jid: &str, el: &Element) -> Option<ReactionUpdate> {
        if el.name() != "message" {
            return None;
        }

        let reactions_el = el
            .children()
            .find(|c| c.name() == "reactions" && c.ns() == NS_REACTIONS)?;

        let target_id = reactions_el.attr("id")?.to_string();

        let emojis: Vec<String> = reactions_el
            .children()
            .filter(|c| c.name() == "reaction")
            .map(tokio_xmpp::minidom::Element::text)
            .collect();

        Some(ReactionUpdate {
            target_id,
            from_jid: from_jid.to_string(),
            emojis,
        })
    }

    // ---- XEP-0308 Correction -----------------------------------------------

    /// Build a correction message replacing a previously sent message.
    ///
    /// ```xml
    /// <message to="{to}" type="chat" id="{new_uuid}" xmlns="jabber:client">
    ///   <body>{new_body}</body>
    ///   <replace xmlns="urn:xmpp:message-correct:0" id="{original_id}"/>
    /// </message>
    /// ```
    pub fn build_correction(&self, to_jid: &str, original_id: &str, new_body: &str) -> Element {
        let new_id = Uuid::new_v4().to_string();

        let body_el = Element::builder("body", NS_CLIENT).append(new_body).build();

        let replace_el = Element::builder("replace", NS_CORRECTION)
            .attr("id", original_id)
            .build();

        Element::builder("message", NS_CLIENT)
            .attr("to", to_jid)
            .attr("type", "chat")
            .attr("id", new_id.as_str())
            .append(body_el)
            .append(replace_el)
            .build()
    }

    /// Parse an incoming correction message.
    ///
    /// Returns `Some(Correction)` when the message contains both a `<body>`
    /// and a `<replace xmlns='urn:xmpp:message-correct:0'>` child, `None`
    /// otherwise.
    pub fn parse_correction(&self, from_jid: &str, el: &Element) -> Option<Correction> {
        if el.name() != "message" {
            return None;
        }

        let replace_el = el
            .children()
            .find(|c| c.name() == "replace" && c.ns() == NS_CORRECTION)?;

        let target_id = replace_el.attr("id")?.to_string();

        let body_el = el.children().find(|c| c.name() == "body")?;
        let new_body = body_el.text();

        Some(Correction {
            target_id,
            from_jid: from_jid.to_string(),
            new_body,
        })
    }

    // ---- XEP-0424 Retraction -----------------------------------------------

    /// Build a retraction message revoking a previously sent message.
    ///
    /// Uses the XEP-0424 v0.4+ `apply-to` wrapper with namespace `:1`.
    ///
    /// ```xml
    /// <message to="{to}" type="chat" id="{new_uuid}" xmlns="jabber:client">
    ///   <apply-to xmlns="urn:xmpp:fasten:0" id="{origin_id}">
    ///     <retract xmlns="urn:xmpp:message-retract:1"/>
    ///   </apply-to>
    /// </message>
    /// ```
    pub fn build_retraction(&self, to_jid: &str, origin_id: &str) -> Element {
        let new_id = Uuid::new_v4().to_string();

        let apply_to_el = Element::builder("apply-to", NS_FASTEN)
            .attr("id", origin_id)
            .append(Element::builder("retract", NS_RETRACTION).build())
            .build();

        Element::builder("message", NS_CLIENT)
            .attr("to", to_jid)
            .attr("type", "chat")
            .attr("id", new_id.as_str())
            .append(apply_to_el)
            .build()
    }

    /// Parse an incoming retraction message.
    ///
    /// Returns `Some(Retraction)` when the message contains an
    /// `<apply-to xmlns='urn:xmpp:fasten:0'>` child that wraps a
    /// `<retract xmlns='urn:xmpp:message-retract:1'>` element. `None` otherwise.
    pub fn parse_retraction(&self, from_jid: &str, el: &Element) -> Option<Retraction> {
        if el.name() != "message" {
            return None;
        }

        let apply_to_el = el
            .children()
            .find(|c| c.name() == "apply-to" && c.ns() == NS_FASTEN)?;

        let target_id = apply_to_el.attr("id")?.to_string();

        // Confirm the retract child is present.
        apply_to_el
            .children()
            .find(|c| c.name() == "retract" && c.ns() == NS_RETRACTION)?;

        Some(Retraction {
            target_id,
            from_jid: from_jid.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. build_reaction: outer message has the reactions namespace child.
    #[test]
    fn build_reaction_has_reactions_namespace() {
        let mgr = MutationManager::new();
        let el = mgr.build_reaction("bob@example.com", "msg-001", &["👍"]);

        let reactions_el = el
            .children()
            .find(|c| c.name() == "reactions")
            .expect("<reactions> child must exist");

        assert_eq!(reactions_el.ns(), NS_REACTIONS);
        assert_eq!(reactions_el.attr("id"), Some("msg-001"));
    }

    // 2. build_reaction: emojis appear as <reaction> children.
    #[test]
    fn build_reaction_includes_emojis() {
        let mgr = MutationManager::new();
        let el = mgr.build_reaction("bob@example.com", "msg-002", &["👍", "❤️", "😂"]);

        let reactions_el = el
            .children()
            .find(|c| c.name() == "reactions")
            .expect("<reactions> must exist");

        let collected: Vec<String> = reactions_el
            .children()
            .filter(|c| c.name() == "reaction")
            .map(tokio_xmpp::minidom::Element::text)
            .collect();

        assert_eq!(collected, vec!["👍", "❤️", "😂"]);
    }

    // 3. build_reaction: empty emojis slice → no <reaction> children.
    #[test]
    fn build_reaction_empty_emojis_no_reaction_children() {
        let mgr = MutationManager::new();
        let el = mgr.build_reaction("bob@example.com", "msg-003", &[]);

        let reactions_el = el
            .children()
            .find(|c| c.name() == "reactions")
            .expect("<reactions> must exist");

        let count = reactions_el
            .children()
            .filter(|c| c.name() == "reaction")
            .count();

        assert_eq!(count, 0);
    }

    // 4. parse_reaction: extracts emojis and target_id correctly.
    #[test]
    fn parse_reaction_extracts_emojis_and_target_id() {
        let mgr = MutationManager::new();
        let built = mgr.build_reaction("alice@example.com", "msg-100", &["👍", "❤️"]);

        let update = mgr
            .parse_reaction("bob@example.com", &built)
            .expect("must parse reaction");

        assert_eq!(update.target_id, "msg-100");
        assert_eq!(update.from_jid, "bob@example.com");
        assert_eq!(update.emojis, vec!["👍", "❤️"]);
    }

    // 5. parse_reaction: returns None for a plain chat message without reactions.
    #[test]
    fn parse_reaction_returns_none_for_non_reaction_message() {
        let mgr = MutationManager::new();

        let plain = Element::builder("message", NS_CLIENT)
            .attr("to", "alice@example.com")
            .attr("type", "chat")
            .append(Element::builder("body", NS_CLIENT).append("hello").build())
            .build();

        let result = mgr.parse_reaction("bob@example.com", &plain);
        assert!(result.is_none());
    }

    // 6. build_correction: message contains a <replace> element with correct namespace.
    #[test]
    fn build_correction_has_replace_element() {
        let mgr = MutationManager::new();
        let el = mgr.build_correction("alice@example.com", "orig-555", "corrected text");

        let replace_el = el
            .children()
            .find(|c| c.name() == "replace")
            .expect("<replace> must exist");

        assert_eq!(replace_el.ns(), NS_CORRECTION);
        assert_eq!(replace_el.attr("id"), Some("orig-555"));
    }

    // 7. parse_correction: extracts target_id and new body.
    #[test]
    fn parse_correction_extracts_target_and_body() {
        let mgr = MutationManager::new();
        let built = mgr.build_correction("alice@example.com", "orig-200", "the fixed body");

        let correction = mgr
            .parse_correction("carol@example.com", &built)
            .expect("must parse correction");

        assert_eq!(correction.target_id, "orig-200");
        assert_eq!(correction.from_jid, "carol@example.com");
        assert_eq!(correction.new_body, "the fixed body");
    }

    // 8. parse_correction: returns None when there is no <replace> child.
    #[test]
    fn parse_correction_returns_none_without_replace() {
        let mgr = MutationManager::new();

        let plain = Element::builder("message", NS_CLIENT)
            .attr("to", "alice@example.com")
            .attr("type", "chat")
            .append(
                Element::builder("body", NS_CLIENT)
                    .append("just a message")
                    .build(),
            )
            .build();

        let result = mgr.parse_correction("bob@example.com", &plain);
        assert!(result.is_none());
    }

    // 9. build_retraction: message contains an apply-to wrapper with the correct origin-id.
    #[test]
    fn build_retraction_has_apply_to_element() {
        let mgr = MutationManager::new();
        let el = mgr.build_retraction("alice@example.com", "origin-888");

        let apply_to_el = el
            .children()
            .find(|c| c.name() == "apply-to" && c.ns() == NS_FASTEN)
            .expect("<apply-to> must exist");

        assert_eq!(apply_to_el.attr("id"), Some("origin-888"));

        let retract_el = apply_to_el
            .children()
            .find(|c| c.name() == "retract" && c.ns() == NS_RETRACTION)
            .expect("<retract> child must exist inside <apply-to>");

        assert_eq!(retract_el.ns(), NS_RETRACTION);
    }

    // 10. parse_retraction: extracts the origin-id from a retraction stanza.
    #[test]
    fn parse_retraction_extracts_origin_id() {
        let mgr = MutationManager::new();
        let built = mgr.build_retraction("alice@example.com", "origin-999");

        let retraction = mgr
            .parse_retraction("dave@example.com", &built)
            .expect("must parse retraction");

        assert_eq!(retraction.target_id, "origin-999");
        assert_eq!(retraction.from_jid, "dave@example.com");
    }

    // 11. parse_retraction: returns None for a message without <retract>.
    #[test]
    fn parse_retraction_returns_none_without_retract() {
        let mgr = MutationManager::new();

        let plain = Element::builder("message", NS_CLIENT)
            .attr("to", "alice@example.com")
            .attr("type", "chat")
            .append(Element::builder("body", NS_CLIENT).append("normal").build())
            .build();

        let result = mgr.parse_retraction("bob@example.com", &plain);
        assert!(result.is_none());
    }

    // 12. build_retraction: the stanza has no <body> (no tombstone text in the v0.4+ format).
    #[test]
    fn build_retraction_has_no_body() {
        let mgr = MutationManager::new();
        let el = mgr.build_retraction("alice@example.com", "origin-777");

        let body_el = el.children().find(|c| c.name() == "body");
        assert!(body_el.is_none(), "retraction stanza must not carry a <body>");
    }
}
