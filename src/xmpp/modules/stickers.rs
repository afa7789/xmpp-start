// Task L1 — XEP-0449 Sticker Packs (placeholder)
// XEP reference: https://xmpp.org/extensions/xep-0449.html
//
// Defines sticker pack data types, a parser, and a message builder.
// Sticker content is carried as Bits of Binary (XEP-0231) references.
// This is a structural placeholder; full PubSub publish/fetch wiring
// is left for the next iteration.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::NS_CLIENT;

const NS_STICKERS: &str = "urn:xmpp:stickers:0";

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A single sticker within a pack.
///
/// The `cid` references a Bits of Binary (XEP-0231) content identifier,
/// allowing the actual image bytes to be fetched separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Sticker {
    pub id: String,
    /// Human-readable description / alt-text.
    pub desc: String,
    /// MIME type of the sticker image (e.g. "image/png", "image/webp").
    pub content_type: String,
    /// BoB content identifier, e.g. "sha1+<hash>@bob.xmpp.org".
    pub cid: String,
}

/// A sticker pack (a named collection of stickers).
#[derive(Debug, Clone, PartialEq)]
pub struct StickerPack {
    pub id: String,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a `<pack xmlns="urn:xmpp:stickers:0">` element into a `StickerPack`.
///
/// Returns `None` if the element is not a valid sticker pack.
///
/// Expected structure:
/// ```xml
/// <pack xmlns="urn:xmpp:stickers:0">
///   <name>My Pack</name>
///   <item id="sticker-1">
///     <desc>A happy face</desc>
///     <file>
///       <media-type>image/png</media-type>
///       <uri>cid:sha1+...@bob.xmpp.org</uri>
///     </file>
///   </item>
/// </pack>
/// ```
pub fn parse_sticker_pack(element: &Element) -> Option<StickerPack> {
    if element.name() != "pack" || element.ns() != NS_STICKERS {
        return None;
    }

    let id = element
        .attr("id")
        .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned);

    let name = element
        .children()
        .find(|c| c.name() == "name" && c.ns() == NS_STICKERS)
        .map_or_else(|| "Unnamed Pack".to_string(), tokio_xmpp::minidom::Element::text);

    let mut stickers = Vec::new();
    for item in element.children().filter(|c| c.name() == "item") {
        if let Some(sticker) = parse_sticker_item(item) {
            stickers.push(sticker);
        }
    }

    Some(StickerPack { id, name, stickers })
}

fn parse_sticker_item(item: &Element) -> Option<Sticker> {
    let id = item.attr("id")?.to_string();

    let desc = item
        .children()
        .find(|c| c.name() == "desc")
        .map(tokio_xmpp::minidom::Element::text)
        .unwrap_or_default();

    // <file> carries <media-type> and <uri cid="..."> (or <uri>cid:...</uri>)
    let file = item.children().find(|c| c.name() == "file")?;

    let content_type = file
        .children()
        .find(|c| c.name() == "media-type")
        .map_or_else(|| "image/png".to_string(), tokio_xmpp::minidom::Element::text);

    // URI is either an attribute on <uri> or the text content prefixed "cid:"
    let cid = file
        .children()
        .find(|c| c.name() == "uri")
        .and_then(|u| {
            // Try attribute first, then text
            u.attr("cid")
                .map(str::to_string)
                .or_else(|| {
                    let txt = u.text();
                    txt.strip_prefix("cid:").map(str::to_owned)
                })
        })?;

    Some(Sticker {
        id,
        desc,
        content_type,
        cid,
    })
}

// ---------------------------------------------------------------------------
// Stanza builder
// ---------------------------------------------------------------------------

/// Build a `<message>` stanza that sends a sticker.
///
/// The sticker is embedded as a `<sticker>` element in the message body,
/// referencing the BoB CID so the recipient can fetch the image bytes.
///
/// ```xml
/// <message type="chat" id="{uuid}" to="{to}">
///   <sticker xmlns="urn:xmpp:stickers:0"
///            pack="{pack_id}"
///            id="{sticker_id}">
///     <desc>{sticker.desc}</desc>
///     <file>
///       <media-type>{sticker.content_type}</media-type>
///       <uri>cid:{sticker.cid}</uri>
///     </file>
///   </sticker>
/// </message>
/// ```
pub fn build_sticker_message(to: &str, pack_id: &str, sticker: &Sticker) -> Element {
    let msg_id = Uuid::new_v4().to_string();

    let media_type_el = Element::builder("media-type", NS_STICKERS)
        .append(sticker.content_type.as_str())
        .build();

    let uri_text = format!("cid:{}", sticker.cid);
    let uri_el = Element::builder("uri", NS_STICKERS)
        .append(uri_text.as_str())
        .build();

    let file_el = Element::builder("file", NS_STICKERS)
        .append(media_type_el)
        .append(uri_el)
        .build();

    let desc_el = Element::builder("desc", NS_STICKERS)
        .append(sticker.desc.as_str())
        .build();

    let sticker_el = Element::builder("sticker", NS_STICKERS)
        .attr("pack", pack_id)
        .attr("id", sticker.id.as_str())
        .append(desc_el)
        .append(file_el)
        .build();

    Element::builder("message", NS_CLIENT)
        .attr("type", "chat")
        .attr("id", &msg_id)
        .attr("to", to)
        .append(sticker_el)
        .build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sticker() -> Sticker {
        Sticker {
            id: "sticker-1".to_string(),
            desc: "A happy face".to_string(),
            content_type: "image/png".to_string(),
            cid: "sha1+aabbccdd@bob.xmpp.org".to_string(),
        }
    }

    fn sample_pack_element() -> Element {
        // Build a minimal valid <pack> element programmatically.
        let name_el = Element::builder("name", NS_STICKERS)
            .append("Test Pack")
            .build();

        let media_type_el = Element::builder("media-type", NS_STICKERS)
            .append("image/png")
            .build();
        let uri_el = Element::builder("uri", NS_STICKERS)
            .append("cid:sha1+aabbccdd@bob.xmpp.org")
            .build();
        let file_el = Element::builder("file", NS_STICKERS)
            .append(media_type_el)
            .append(uri_el)
            .build();
        let desc_el = Element::builder("desc", NS_STICKERS)
            .append("A happy face")
            .build();
        let item_el = Element::builder("item", NS_STICKERS)
            .attr("id", "sticker-1")
            .append(desc_el)
            .append(file_el)
            .build();

        Element::builder("pack", NS_STICKERS)
            .attr("id", "pack-abc")
            .append(name_el)
            .append(item_el)
            .build()
    }

    #[test]
    fn parse_sticker_pack_extracts_name_and_id() {
        let el = sample_pack_element();
        let pack = parse_sticker_pack(&el).expect("parse failed");

        assert_eq!(pack.id, "pack-abc");
        assert_eq!(pack.name, "Test Pack");
    }

    #[test]
    fn parse_sticker_pack_extracts_stickers() {
        let el = sample_pack_element();
        let pack = parse_sticker_pack(&el).expect("parse failed");

        assert_eq!(pack.stickers.len(), 1);
        let s = &pack.stickers[0];
        assert_eq!(s.id, "sticker-1");
        assert_eq!(s.desc, "A happy face");
        assert_eq!(s.content_type, "image/png");
        assert_eq!(s.cid, "sha1+aabbccdd@bob.xmpp.org");
    }

    #[test]
    fn parse_sticker_pack_wrong_element_returns_none() {
        let el = Element::builder("not-a-pack", NS_STICKERS).build();
        assert!(parse_sticker_pack(&el).is_none());
    }

    #[test]
    fn parse_sticker_pack_wrong_ns_returns_none() {
        let el = Element::builder("pack", "wrong:ns").build();
        assert!(parse_sticker_pack(&el).is_none());
    }

    #[test]
    fn build_sticker_message_has_correct_structure() {
        let sticker = sample_sticker();
        let el = build_sticker_message("friend@example.org", "pack-abc", &sticker);

        assert_eq!(el.name(), "message");
        assert_eq!(el.attr("type"), Some("chat"));
        assert_eq!(el.attr("to"), Some("friend@example.org"));
        assert!(el.attr("id").is_some());

        let sticker_el = el
            .children()
            .find(|c| c.name() == "sticker")
            .expect("no sticker child");
        assert_eq!(sticker_el.ns(), NS_STICKERS);
        assert_eq!(sticker_el.attr("pack"), Some("pack-abc"));
        assert_eq!(sticker_el.attr("id"), Some("sticker-1"));
    }

    #[test]
    fn build_sticker_message_contains_file_with_cid() {
        let sticker = sample_sticker();
        let el = build_sticker_message("friend@example.org", "pack-abc", &sticker);
        let sticker_el = el
            .children()
            .find(|c| c.name() == "sticker")
            .unwrap();
        let file_el = sticker_el
            .children()
            .find(|c| c.name() == "file")
            .expect("no file child");
        let uri_el = file_el
            .children()
            .find(|c| c.name() == "uri")
            .expect("no uri child");

        assert_eq!(uri_el.text(), "cid:sha1+aabbccdd@bob.xmpp.org");
    }

    #[test]
    fn build_sticker_message_unique_ids() {
        let sticker = sample_sticker();
        let el1 = build_sticker_message("a@example.org", "pack-1", &sticker);
        let el2 = build_sticker_message("a@example.org", "pack-1", &sticker);
        assert_ne!(el1.attr("id"), el2.attr("id"));
    }
}
