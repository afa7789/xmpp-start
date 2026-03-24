#![allow(dead_code)]
// S3: MUC Voice Request module — XEP-0045 §5.2 (Requesting Voice)
// Reference: https://xmpp.org/extensions/xep-0045.html
//
// Handles:
//   - User requests voice (when room is locked/voice-requested)
//   - Admin/owner receives and processes voice requests
//   - Grant/revoke voice to users

use tokio_xmpp::minidom::Element;

const NS_MUC: &str = "http://jabber.org/protocol/muc";

#[derive(Debug, Clone)]
pub struct MucVoiceManager {
    pending_requests: std::collections::HashMap<String, VoiceRequest>,
}

#[derive(Debug, Clone)]
pub struct VoiceRequest {
    pub room_jid: String,
    pub nick: String,
    pub role: String,
}

impl MucVoiceManager {
    pub fn new() -> Self {
        Self {
            pending_requests: std::collections::HashMap::new(),
        }
    }

    pub fn build_voice_request(&self, room_jid: &str, _nick: &str) -> Element {
        Element::builder("message", "jabber:client")
            .attr("to", room_jid)
            .append(
                Element::builder("x", NS_MUC)
                    .append(Element::builder("decline", NS_MUC).build())
                    .build(),
            )
            .build()
    }

    pub fn build_approve_voice(&self, room_jid: &str, nick: &str) -> Element {
        Element::builder("message", "jabber:client")
            .attr("to", room_jid)
            .append(
                Element::builder("x", NS_MUC)
                    .append(
                        Element::builder("approve", NS_MUC)
                            .attr("nick", nick)
                            .build(),
                    )
                    .build(),
            )
            .build()
    }

    pub fn build_decline_voice(&self, room_jid: &str, nick: &str) -> Element {
        Element::builder("message", "jabber:client")
            .attr("to", room_jid)
            .append(
                Element::builder("x", NS_MUC)
                    .append(
                        Element::builder("decline", NS_MUC)
                            .attr("nick", nick)
                            .build(),
                    )
                    .build(),
            )
            .build()
    }

    pub fn parse_voice_request(&self, el: &Element) -> Option<VoiceRequest> {
        let x = el
            .children()
            .find(|c| c.name() == "x" && c.ns() == NS_MUC)?;
        let request = x.children().find(|c| c.name() == "request")?;

        let room_jid = el.attr("from")?.to_string();
        let nick = request.attr("nick")?.to_string();

        Some(VoiceRequest {
            room_jid,
            nick,
            role: "participant".to_string(),
        })
    }

    pub fn is_voice_request(&self, el: &Element) -> bool {
        el.children().any(|c| {
            c.name() == "x" && c.ns() == NS_MUC && c.children().any(|cc| cc.name() == "request")
        })
    }
}

impl Default for MucVoiceManager {
    fn default() -> Self {
        Self::new()
    }
}
