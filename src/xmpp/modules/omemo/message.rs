#![allow(dead_code)]
// OMEMO encrypted message stanza builder and parser (XEP-0384)
//
// Builds and parses the <encrypted xmlns="eu.siacs.conversations.axolotl">
// stanza inside a <message> element. This is a pure, synchronous, I/O-free
// module — no async, no crypto. Encoding/decoding only.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use tokio_xmpp::minidom::Element;

use super::NS_OMEMO;
use crate::xmpp::modules::NS_CLIENT;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A single per-device encrypted key slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageKey {
    /// Recipient device ID.
    pub rid: u32,
    /// `true` when this is a PreKey (X3DH key-exchange) message.
    pub prekey: bool,
    /// The encrypted key bytes (Olm ciphertext).
    pub data: Vec<u8>,
}

/// The `<header>` of an OMEMO `<encrypted>` stanza.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageHeader {
    /// Sender device ID.
    pub sid: u32,
    /// One key slot per recipient device.
    pub keys: Vec<MessageKey>,
    /// AES-256-GCM initialisation vector (12 bytes).
    pub iv: Vec<u8>,
}

/// A fully-parsed OMEMO encrypted message.
///
/// `payload` is `None` for key-transport messages (no body ciphertext).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedMessage {
    pub header: MessageHeader,
    /// AES-256-GCM ciphertext (body + tag). `None` for key-transport.
    pub payload: Option<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Stanza builders
// ---------------------------------------------------------------------------

/// Build a `<message>` stanza carrying an OMEMO `<encrypted>` payload.
///
/// ```xml
/// <message to="{to}" type="chat">
///   <encrypted xmlns="eu.siacs.conversations.axolotl">
///     <header sid="{sid}">
///       <key rid="{rid}" prekey="true">{base64}</key>
///       ...
///       <iv>{base64}</iv>
///     </header>
///     <payload>{base64}</payload>
///   </encrypted>
///   <store xmlns="urn:xmpp:hints"/>
/// </message>
/// ```
pub fn build_encrypted_message(
    to: &str,
    _from_device: u32,
    encrypted: &EncryptedMessage,
) -> Element {
    let encrypted_el = build_encrypted_element(&encrypted.header, encrypted.payload.as_deref());

    let mut message = Element::builder("message", NS_CLIENT)
        .attr("to", to)
        .attr("type", "chat")
        .append(encrypted_el)
        .build();

    // XEP-0334 <store/> hint so the server archives the stanza.
    let store_el = Element::builder("store", "urn:xmpp:hints").build();
    message.append_child(store_el);

    message
}

/// Build a key-transport `<message>` (no payload — header only).
///
/// Used to establish an Olm session with a device before sending real content.
pub fn build_key_transport(to: &str, _from_device: u32, header: &MessageHeader) -> Element {
    let transport = EncryptedMessage {
        header: header.clone(),
        payload: None,
    };
    build_encrypted_message(to, 0, &transport)
}

// ---------------------------------------------------------------------------
// Stanza parsers
// ---------------------------------------------------------------------------

/// Parse an incoming `<message>` (or bare `<encrypted>`) into an
/// [`EncryptedMessage`]. Returns `None` if the stanza is not a valid OMEMO
/// encrypted message.
pub fn parse_encrypted_message(element: &Element) -> Option<EncryptedMessage> {
    // Accept both a bare <encrypted> and a <message> wrapping one.
    let encrypted_el = if element.name() == "encrypted" && element.ns() == NS_OMEMO {
        element
    } else {
        element.get_child("encrypted", NS_OMEMO)?
    };

    let header_el = encrypted_el.get_child("header", NS_OMEMO)?;

    let sid: u32 = header_el.attr("sid")?.parse().ok()?;

    let mut keys = Vec::new();
    for key_el in header_el.children() {
        if key_el.name() != "key" || key_el.ns() != NS_OMEMO {
            continue;
        }
        let rid: u32 = key_el.attr("rid")?.parse().ok()?;
        let prekey = key_el.attr("prekey").is_some_and(|v| v == "true");
        let data = BASE64.decode(key_el.text()).ok()?;
        keys.push(MessageKey { rid, prekey, data });
    }

    let iv_el = header_el.get_child("iv", NS_OMEMO)?;
    let iv = BASE64.decode(iv_el.text()).ok()?;

    let header = MessageHeader { sid, keys, iv };

    let payload = encrypted_el
        .get_child("payload", NS_OMEMO)
        .and_then(|el| BASE64.decode(el.text()).ok());

    Some(EncryptedMessage { header, payload })
}

/// Returns `true` when the message carries no payload (key-transport only).
pub fn is_key_transport(msg: &EncryptedMessage) -> bool {
    msg.payload.is_none()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build the inner `<encrypted xmlns="...">` element shared by both builders.
fn build_encrypted_element(header: &MessageHeader, payload: Option<&[u8]>) -> Element {
    let iv_el = Element::builder("iv", NS_OMEMO)
        .append(BASE64.encode(&header.iv))
        .build();

    let mut header_el = Element::builder("header", NS_OMEMO)
        .attr("sid", header.sid.to_string())
        .build();

    for key in &header.keys {
        let mut key_builder = Element::builder("key", NS_OMEMO).attr("rid", key.rid.to_string());
        if key.prekey {
            key_builder = key_builder.attr("prekey", "true");
        }
        let key_el = key_builder.append(BASE64.encode(&key.data)).build();
        header_el.append_child(key_el);
    }
    header_el.append_child(iv_el);

    let mut encrypted_builder = Element::builder("encrypted", NS_OMEMO).append(header_el);

    if let Some(payload_bytes) = payload {
        let payload_el = Element::builder("payload", NS_OMEMO)
            .append(BASE64.encode(payload_bytes))
            .build();
        encrypted_builder = encrypted_builder.append(payload_el);
    }

    encrypted_builder.build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(sid: u32) -> MessageHeader {
        MessageHeader {
            sid,
            keys: vec![
                MessageKey {
                    rid: 100,
                    prekey: true,
                    data: vec![0xde, 0xad, 0xbe, 0xef],
                },
                MessageKey {
                    rid: 200,
                    prekey: false,
                    data: vec![0xca, 0xfe, 0xba, 0xbe],
                },
            ],
            iv: vec![0x01; 12],
        }
    }

    // -----------------------------------------------------------------------
    // Roundtrip: build → parse → equals original
    // -----------------------------------------------------------------------

    #[test]
    fn roundtrip_with_payload() {
        let original = EncryptedMessage {
            header: make_header(12345),
            payload: Some(vec![0xab; 32]),
        };

        let message_el = build_encrypted_message("bob@example.com", 12345, &original);
        let parsed = parse_encrypted_message(&message_el)
            .expect("parse_encrypted_message must succeed");

        assert_eq!(parsed, original);
    }

    #[test]
    fn roundtrip_key_transport_no_payload() {
        let original = EncryptedMessage {
            header: make_header(99),
            payload: None,
        };

        let message_el = build_encrypted_message("carol@example.com", 99, &original);
        let parsed = parse_encrypted_message(&message_el)
            .expect("parse_encrypted_message must succeed for key-transport");

        assert_eq!(parsed, original);
        assert!(is_key_transport(&parsed));
    }

    // -----------------------------------------------------------------------
    // is_key_transport
    // -----------------------------------------------------------------------

    #[test]
    fn is_key_transport_with_payload_returns_false() {
        let msg = EncryptedMessage {
            header: make_header(1),
            payload: Some(vec![1, 2, 3]),
        };
        assert!(!is_key_transport(&msg));
    }

    #[test]
    fn is_key_transport_without_payload_returns_true() {
        let msg = EncryptedMessage {
            header: make_header(1),
            payload: None,
        };
        assert!(is_key_transport(&msg));
    }

    // -----------------------------------------------------------------------
    // Multiple recipients
    // -----------------------------------------------------------------------

    #[test]
    fn roundtrip_multiple_recipients() {
        let original = EncryptedMessage {
            header: MessageHeader {
                sid: 42,
                keys: vec![
                    MessageKey { rid: 1, prekey: true, data: vec![0x01, 0x02] },
                    MessageKey { rid: 2, prekey: false, data: vec![0x03, 0x04] },
                    MessageKey { rid: 3, prekey: true, data: vec![0x05, 0x06] },
                ],
                iv: vec![0xff; 12],
            },
            payload: Some(vec![0x10, 0x20, 0x30]),
        };

        let el = build_encrypted_message("group@conference.example.com", 42, &original);
        let parsed = parse_encrypted_message(&el).expect("must parse");
        assert_eq!(parsed, original);
        assert_eq!(parsed.header.keys.len(), 3);
    }

    // -----------------------------------------------------------------------
    // build_key_transport helper
    // -----------------------------------------------------------------------

    #[test]
    fn build_key_transport_produces_no_payload() {
        let header = make_header(77);
        let el = build_key_transport("dave@example.com", 77, &header);
        let parsed = parse_encrypted_message(&el).expect("must parse");
        assert!(is_key_transport(&parsed));
        assert_eq!(parsed.header, header);
    }

    // -----------------------------------------------------------------------
    // Structural checks
    // -----------------------------------------------------------------------

    #[test]
    fn message_element_has_correct_attributes() {
        let msg = EncryptedMessage {
            header: make_header(1),
            payload: None,
        };
        let el = build_encrypted_message("target@example.com", 1, &msg);
        assert_eq!(el.name(), "message");
        assert_eq!(el.attr("to"), Some("target@example.com"));
        assert_eq!(el.attr("type"), Some("chat"));
    }

    #[test]
    fn encrypted_element_uses_correct_namespace() {
        let msg = EncryptedMessage {
            header: make_header(1),
            payload: None,
        };
        let el = build_encrypted_message("x@x.com", 1, &msg);
        let encrypted = el.get_child("encrypted", NS_OMEMO);
        assert!(encrypted.is_some(), "must have <encrypted> child in NS_OMEMO");
    }

    #[test]
    fn prekey_flag_round_trips() {
        let original = EncryptedMessage {
            header: MessageHeader {
                sid: 1,
                keys: vec![
                    MessageKey { rid: 10, prekey: true, data: vec![0xaa] },
                    MessageKey { rid: 20, prekey: false, data: vec![0xbb] },
                ],
                iv: vec![0x00; 12],
            },
            payload: None,
        };

        let el = build_encrypted_message("x@x.com", 1, &original);
        let parsed = parse_encrypted_message(&el).unwrap();

        let prekey_true = parsed.header.keys.iter().find(|k| k.rid == 10).unwrap();
        let prekey_false = parsed.header.keys.iter().find(|k| k.rid == 20).unwrap();
        assert!(prekey_true.prekey);
        assert!(!prekey_false.prekey);
    }

    // -----------------------------------------------------------------------
    // Parse bare <encrypted> element (not wrapped in <message>)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_bare_encrypted_element() {
        let original = EncryptedMessage {
            header: make_header(55),
            payload: Some(vec![0x99; 8]),
        };
        let message_el = build_encrypted_message("x@x.com", 55, &original);
        // Extract the <encrypted> child and parse it directly.
        let encrypted_el = message_el
            .get_child("encrypted", NS_OMEMO)
            .expect("must have encrypted child");
        let parsed = parse_encrypted_message(encrypted_el).expect("must parse bare encrypted");
        assert_eq!(parsed, original);
    }
}
