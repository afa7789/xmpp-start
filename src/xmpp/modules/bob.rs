#![allow(dead_code)]
// Task Q2 — XEP-0231 Bits of Binary
// XEP reference: https://xmpp.org/extensions/xep-0231.html
//
// Pure stanza builder/parser — no I/O, no async.
// Supports embedding small binary data (images, sounds) directly in XMPP
// stanzas using a content-addressed scheme (SHA-1 CID).

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{find_child_recursive, NS_CLIENT};

const NS_BOB: &str = "urn:xmpp:bob";

// ---------------------------------------------------------------------------
// Domain type
// ---------------------------------------------------------------------------

/// A Bits of Binary data attachment (XEP-0231).
#[derive(Debug, Clone, PartialEq)]
pub struct BobData {
    /// Content identifier, e.g. "sha1+8f35fef110ffc5df08d579a50083ff9308fb6242@bob.xmpp.org"
    pub cid: String,
    /// MIME type, e.g. "image/png"
    pub content_type: String,
    /// Raw binary payload.
    pub data: Vec<u8>,
    /// Optional max-age in seconds (0 = do not cache).
    pub max_age: Option<u32>,
}

// ---------------------------------------------------------------------------
// Stanza builders / parsers
// ---------------------------------------------------------------------------

/// Build a `<data>` element carrying the binary payload.
///
/// ```xml
/// <data xmlns="urn:xmpp:bob"
///       cid="sha1+...@bob.xmpp.org"
///       type="image/png"
///       max-age="86400">
///   BASE64_DATA
/// </data>
/// ```
pub fn build_bob_data(bob: &BobData) -> Element {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let encoded = STANDARD.encode(&bob.data);

    let mut builder = Element::builder("data", NS_BOB)
        .attr("cid", &bob.cid)
        .attr("type", &bob.content_type);

    if let Some(age) = bob.max_age {
        builder = builder.attr("max-age", age.to_string().as_str());
    }

    builder.append(encoded.as_str()).build()
}

/// Parse a `<data xmlns="urn:xmpp:bob">` element (or any wrapper containing
/// one) into a `BobData`. Returns `None` if the element is not a valid BoB
/// data element.
pub fn parse_bob_data(element: &Element) -> Option<BobData> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let data_el = find_bob_data(element)?;

    let cid = data_el.attr("cid")?.to_string();
    let content_type = data_el.attr("type")?.to_string();
    let max_age = data_el
        .attr("max-age")
        .and_then(|v| v.parse::<u32>().ok());

    let raw = data_el.text();
    let trimmed = raw.split_whitespace().collect::<String>();
    let data = STANDARD.decode(&trimmed).ok()?;

    Some(BobData {
        cid,
        content_type,
        data,
        max_age,
    })
}

/// Build an IQ get request for a BoB CID.
///
/// ```xml
/// <iq type="get" id="{uuid}" to="{from}">
///   <data xmlns="urn:xmpp:bob" cid="sha1+...@bob.xmpp.org"/>
/// </iq>
/// ```
pub fn build_bob_request(cid: &str, from: &str) -> Element {
    let id = Uuid::new_v4().to_string();

    let data_el = Element::builder("data", NS_BOB)
        .attr("cid", cid)
        .build();

    Element::builder("iq", NS_CLIENT)
        .attr("type", "get")
        .attr("id", &id)
        .attr("to", from)
        .append(data_el)
        .build()
}

/// Recursively search `el` for a `<data xmlns="urn:xmpp:bob">` element.
fn find_bob_data(el: &Element) -> Option<&Element> {
    find_child_recursive(el, "data", NS_BOB)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bob() -> BobData {
        BobData {
            cid: "sha1+aabbcc@bob.xmpp.org".to_string(),
            content_type: "image/png".to_string(),
            data: vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
            max_age: Some(86400),
        }
    }

    #[test]
    fn build_bob_data_has_correct_attrs() {
        let bob = sample_bob();
        let el = build_bob_data(&bob);

        assert_eq!(el.name(), "data");
        assert_eq!(el.ns(), NS_BOB);
        assert_eq!(el.attr("cid"), Some("sha1+aabbcc@bob.xmpp.org"));
        assert_eq!(el.attr("type"), Some("image/png"));
        assert_eq!(el.attr("max-age"), Some("86400"));
    }

    #[test]
    fn parse_bob_data_roundtrip() {
        let original = sample_bob();
        let el = build_bob_data(&original);
        let parsed = parse_bob_data(&el).expect("parse failed");

        assert_eq!(parsed.cid, original.cid);
        assert_eq!(parsed.content_type, original.content_type);
        assert_eq!(parsed.data, original.data);
        assert_eq!(parsed.max_age, original.max_age);
    }

    #[test]
    fn parse_bob_data_no_max_age() {
        let bob = BobData {
            cid: "sha1+aabbcc@bob.xmpp.org".to_string(),
            content_type: "image/jpeg".to_string(),
            data: vec![0xff, 0xd8, 0xff],
            max_age: None,
        };
        let el = build_bob_data(&bob);
        let parsed = parse_bob_data(&el).expect("parse failed");

        assert_eq!(parsed.max_age, None);
        assert_eq!(parsed.data, bob.data);
    }

    #[test]
    fn build_bob_request_has_correct_structure() {
        let el = build_bob_request("sha1+aabbcc@bob.xmpp.org", "user@example.org");

        assert_eq!(el.name(), "iq");
        assert_eq!(el.attr("type"), Some("get"));
        assert_eq!(el.attr("to"), Some("user@example.org"));
        assert!(el.attr("id").is_some());

        let data_el = el
            .children()
            .find(|c| c.name() == "data")
            .expect("no data child");
        assert_eq!(data_el.ns(), NS_BOB);
        assert_eq!(
            data_el.attr("cid"),
            Some("sha1+aabbcc@bob.xmpp.org")
        );
    }

    #[test]
    fn build_bob_request_unique_ids() {
        let el1 = build_bob_request("cid1@bob.xmpp.org", "a@example.org");
        let el2 = build_bob_request("cid1@bob.xmpp.org", "a@example.org");
        assert_ne!(el1.attr("id"), el2.attr("id"));
    }

    #[test]
    fn parse_bob_data_from_wrapper_element() {
        let bob = sample_bob();
        let data_el = build_bob_data(&bob);

        // Wrap in an <iq> result — parse should still find the <data>.
        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .append(data_el)
            .build();

        let parsed = parse_bob_data(&iq).expect("parse failed");
        assert_eq!(parsed.cid, bob.cid);
    }
}
