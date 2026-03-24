// K2: vCard editing — XEP-0054 (vCard-temp) + XEP-0292 (vCard4 over XMPP)
// XEP references:
//   https://xmpp.org/extensions/xep-0054.html
//   https://xmpp.org/extensions/xep-0292.html
//
// This module handles fetching and publishing the user's own vCard
// (nickname, full name, organisation, email, phone).
// All methods are pure: no I/O, no async.

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_VCARD};

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Fields editable by the user in the vCard editor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VCardFields {
    pub nickname: String,
    pub full_name: String,
    pub organisation: String,
    pub email: String,
    pub phone: String,
}

// ---------------------------------------------------------------------------
// VCardEditManager
// ---------------------------------------------------------------------------

/// Manages own-vCard fetch and publish for the logged-in user.
///
/// All methods are synchronous. The caller sends/receives XMPP stanzas
/// and calls the relevant handler.
pub struct VCardEditManager {
    /// iq_id → () for in-flight "get own vCard" requests.
    pending_get: HashMap<String, ()>,
    /// iq_id → () for in-flight "set own vCard" requests.
    pending_set: HashMap<String, ()>,
}

impl Default for VCardEditManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VCardEditManager {
    pub fn new() -> Self {
        Self {
            pending_get: HashMap::new(),
            pending_set: HashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Fetch own vCard (IQ get, no `to` attribute → own JID)
    // -----------------------------------------------------------------------

    /// Build an IQ `get` to fetch the logged-in user's own vCard.
    ///
    /// ```xml
    /// <iq type="get" id="{id}">
    ///   <vCard xmlns="vcard-temp"/>
    /// </iq>
    /// ```
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_get(&mut self) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        self.pending_get.insert(id.clone(), ());

        let vcard = Element::builder("vCard", NS_VCARD).build();
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", &id)
            .append(vcard)
            .build();

        (id, iq)
    }

    /// Parse an IQ `result` response to a previously-issued `get`.
    ///
    /// Returns `Some(VCardFields)` if the response was ours.
    /// Returns `None` if the IQ id is unknown or the element is not a valid
    /// vCard result.
    pub fn on_get_result(&mut self, el: &Element) -> Option<VCardFields> {
        let iq_id = el.attr("id")?;
        self.pending_get.remove(iq_id)?;

        let vcard = el
            .children()
            .find(|c| c.name() == "vCard" && c.ns() == NS_VCARD)?;

        Some(parse_vcard_fields(vcard))
    }

    // -----------------------------------------------------------------------
    // Publish own vCard (IQ set)
    // -----------------------------------------------------------------------

    /// Build an IQ `set` to publish the given vCard fields.
    ///
    /// ```xml
    /// <iq type="set" id="{id}">
    ///   <vCard xmlns="vcard-temp">
    ///     <NICKNAME>…</NICKNAME>
    ///     <FN>…</FN>
    ///     <ORG><ORGNAME>…</ORGNAME></ORG>
    ///     <EMAIL><INTERNET/><USERID>…</USERID></EMAIL>
    ///     <TEL><VOICE/><NUMBER>…</NUMBER></TEL>
    ///   </vCard>
    /// </iq>
    /// ```
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_set(&mut self, fields: &VCardFields) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        self.pending_set.insert(id.clone(), ());

        let vcard = build_vcard_element(fields);
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(vcard)
            .build();

        (id, iq)
    }

    /// Returns `true` if this result IQ matches a pending `set` request.
    ///
    /// Call this after a successful `build_set` to detect success/failure.
    pub fn on_set_result(&mut self, el: &Element) -> bool {
        let iq_id = match el.attr("id") {
            Some(id) => id,
            None => return false,
        };
        self.pending_set.remove(iq_id).is_some()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract text content of a direct child element by name.
fn child_text(parent: &Element, name: &str) -> String {
    parent
        .children()
        .find(|c| c.name() == name)
        .map(tokio_xmpp::minidom::Element::text)
        .unwrap_or_default()
}

/// Parse a `<vCard xmlns="vcard-temp">` element into `VCardFields`.
fn parse_vcard_fields(vcard: &Element) -> VCardFields {
    let nickname = child_text(vcard, "NICKNAME");
    let full_name = child_text(vcard, "FN");

    // <ORG><ORGNAME>…</ORGNAME></ORG>
    let organisation = vcard
        .children()
        .find(|c| c.name() == "ORG")
        .map(|org| child_text(org, "ORGNAME"))
        .unwrap_or_default();

    // <EMAIL><USERID>…</USERID></EMAIL>
    let email = vcard
        .children()
        .find(|c| c.name() == "EMAIL")
        .map(|em| child_text(em, "USERID"))
        .unwrap_or_default();

    // <TEL><NUMBER>…</NUMBER></TEL>
    let phone = vcard
        .children()
        .find(|c| c.name() == "TEL")
        .map(|tel| child_text(tel, "NUMBER"))
        .unwrap_or_default();

    VCardFields {
        nickname,
        full_name,
        organisation,
        email,
        phone,
    }
}

/// Build a `<vCard xmlns="vcard-temp">` element from `VCardFields`.
fn build_vcard_element(fields: &VCardFields) -> Element {
    let mut vcard = Element::builder("vCard", NS_VCARD);

    if !fields.nickname.is_empty() {
        let el = Element::builder("NICKNAME", NS_VCARD)
            .append(fields.nickname.as_str())
            .build();
        vcard = vcard.append(el);
    }
    if !fields.full_name.is_empty() {
        let el = Element::builder("FN", NS_VCARD)
            .append(fields.full_name.as_str())
            .build();
        vcard = vcard.append(el);
    }
    if !fields.organisation.is_empty() {
        let orgname = Element::builder("ORGNAME", NS_VCARD)
            .append(fields.organisation.as_str())
            .build();
        let org = Element::builder("ORG", NS_VCARD).append(orgname).build();
        vcard = vcard.append(org);
    }
    if !fields.email.is_empty() {
        let internet = Element::builder("INTERNET", NS_VCARD).build();
        let userid = Element::builder("USERID", NS_VCARD)
            .append(fields.email.as_str())
            .build();
        let email_el = Element::builder("EMAIL", NS_VCARD)
            .append(internet)
            .append(userid)
            .build();
        vcard = vcard.append(email_el);
    }
    if !fields.phone.is_empty() {
        let voice = Element::builder("VOICE", NS_VCARD).build();
        let number = Element::builder("NUMBER", NS_VCARD)
            .append(fields.phone.as_str())
            .build();
        let tel = Element::builder("TEL", NS_VCARD)
            .append(voice)
            .append(number)
            .build();
        vcard = vcard.append(tel);
    }

    vcard.build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. build_get registers pending and produces a type="get" IQ
    #[test]
    fn build_get_registers_pending() {
        let mut mgr = VCardEditManager::new();
        let (id, iq) = mgr.build_get();

        assert!(!id.is_empty());
        assert!(mgr.pending_get.contains_key(&id));
        assert_eq!(iq.attr("type"), Some("get"));

        let vcard = iq
            .children()
            .find(|c| c.name() == "vCard")
            .expect("<vCard> child missing");
        assert_eq!(vcard.ns(), NS_VCARD);
    }

    // 2. build_set produces a type="set" IQ with all non-empty fields encoded
    #[test]
    fn build_set_encodes_all_fields() {
        let mut mgr = VCardEditManager::new();
        let fields = VCardFields {
            nickname: "jdoe".into(),
            full_name: "John Doe".into(),
            organisation: "ACME Corp".into(),
            email: "john@example.com".into(),
            phone: "+1-555-0100".into(),
        };
        let (_id, iq) = mgr.build_set(&fields);

        assert_eq!(iq.attr("type"), Some("set"));
        let vcard = iq
            .children()
            .find(|c| c.name() == "vCard")
            .expect("<vCard> missing");

        assert_eq!(
            vcard
                .children()
                .find(|c| c.name() == "NICKNAME")
                .map(tokio_xmpp::minidom::Element::text),
            Some("jdoe".into())
        );
        assert_eq!(
            vcard
                .children()
                .find(|c| c.name() == "FN")
                .map(tokio_xmpp::minidom::Element::text),
            Some("John Doe".into())
        );
        // ORG / ORGNAME
        let org = vcard
            .children()
            .find(|c| c.name() == "ORG")
            .expect("<ORG> missing");
        assert_eq!(child_text(org, "ORGNAME"), "ACME Corp");
        // EMAIL / USERID
        let email_el = vcard
            .children()
            .find(|c| c.name() == "EMAIL")
            .expect("<EMAIL> missing");
        assert_eq!(child_text(email_el, "USERID"), "john@example.com");
        // TEL / NUMBER
        let tel = vcard
            .children()
            .find(|c| c.name() == "TEL")
            .expect("<TEL> missing");
        assert_eq!(child_text(tel, "NUMBER"), "+1-555-0100");
    }

    // 3. on_get_result parses a well-formed vCard result
    #[test]
    fn on_get_result_parses_fields() {
        let mut mgr = VCardEditManager::new();
        let (id, _) = mgr.build_get();

        // Build a synthetic vCard result IQ.
        let nickname = Element::builder("NICKNAME", NS_VCARD)
            .append("jdoe")
            .build();
        let fn_el = Element::builder("FN", NS_VCARD)
            .append("John Doe")
            .build();
        let orgname = Element::builder("ORGNAME", NS_VCARD)
            .append("ACME")
            .build();
        let org = Element::builder("ORG", NS_VCARD).append(orgname).build();
        let userid = Element::builder("USERID", NS_VCARD)
            .append("john@example.com")
            .build();
        let email_el = Element::builder("EMAIL", NS_VCARD).append(userid).build();
        let number = Element::builder("NUMBER", NS_VCARD)
            .append("+1-555-0100")
            .build();
        let tel = Element::builder("TEL", NS_VCARD).append(number).build();

        let vcard = Element::builder("vCard", NS_VCARD)
            .append(nickname)
            .append(fn_el)
            .append(org)
            .append(email_el)
            .append(tel)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(vcard)
            .build();

        let result = mgr.on_get_result(&iq).expect("expected VCardFields");
        assert_eq!(result.nickname, "jdoe");
        assert_eq!(result.full_name, "John Doe");
        assert_eq!(result.organisation, "ACME");
        assert_eq!(result.email, "john@example.com");
        assert_eq!(result.phone, "+1-555-0100");
    }

    // 4. on_get_result clears pending and returns None for unknown IQ id
    #[test]
    fn on_get_result_clears_pending_and_ignores_unknown() {
        let mut mgr = VCardEditManager::new();
        let (id, _) = mgr.build_get();

        // Unknown id.
        let bogus_vcard = Element::builder("vCard", NS_VCARD).build();
        let bogus = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", "not-a-real-id")
            .append(bogus_vcard)
            .build();
        assert!(mgr.on_get_result(&bogus).is_none());

        // Now handle the real one.
        let real_vcard = Element::builder("vCard", NS_VCARD).build();
        let real = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .append(real_vcard)
            .build();
        let result = mgr.on_get_result(&real);
        assert!(result.is_some());
        assert!(!mgr.pending_get.contains_key(&id));
    }

    // 5. on_set_result returns true for the matching IQ id
    #[test]
    fn on_set_result_matches_pending() {
        let mut mgr = VCardEditManager::new();
        let fields = VCardFields::default();
        let (id, _) = mgr.build_set(&fields);

        // Unknown id returns false.
        let bogus = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", "wrong-id")
            .build();
        assert!(!mgr.on_set_result(&bogus));

        // Correct id returns true and clears pending.
        let real = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .build();
        assert!(mgr.on_set_result(&real));
        assert!(!mgr.pending_set.contains_key(&id));
    }

    // 6. Empty fields are omitted from the built <vCard>
    #[test]
    fn build_set_omits_empty_fields() {
        let mut mgr = VCardEditManager::new();
        let fields = VCardFields {
            nickname: "nick".into(),
            ..Default::default()
        };
        let (_id, iq) = mgr.build_set(&fields);
        let vcard = iq.children().find(|c| c.name() == "vCard").unwrap();

        assert!(vcard.children().find(|c| c.name() == "FN").is_none());
        assert!(vcard.children().find(|c| c.name() == "ORG").is_none());
        assert!(vcard.children().find(|c| c.name() == "EMAIL").is_none());
        assert!(vcard.children().find(|c| c.name() == "TEL").is_none());
    }

    // 7. Roundtrip: build_set then parse the stanza back with parse_vcard_fields
    #[test]
    fn roundtrip_build_then_parse() {
        let mut mgr = VCardEditManager::new();
        let original = VCardFields {
            nickname: "alice".into(),
            full_name: "Alice Smith".into(),
            organisation: "Corp".into(),
            email: "alice@corp.com".into(),
            phone: "123".into(),
        };
        let (_id, iq) = mgr.build_set(&original);
        let vcard_el = iq.children().find(|c| c.name() == "vCard").unwrap();
        let parsed = parse_vcard_fields(vcard_el);
        assert_eq!(parsed, original);
    }
}
