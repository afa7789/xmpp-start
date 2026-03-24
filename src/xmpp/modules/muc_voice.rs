// S3: MUC Voice Request module — XEP-0045 §5.2 (Requesting Voice)
// Reference: https://xmpp.org/extensions/xep-0045.html
//
// Handles:
//   - User requests voice (when room is moderated)
//   - Admin approves or declines voice requests

use tokio_xmpp::minidom::Element;

use super::{NS_CLIENT, NS_MUC};

#[derive(Debug, Clone)]
pub struct VoiceRequest {
    pub room_jid: String,
    pub nick: String,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct MucVoiceManager;

impl MucVoiceManager {
    pub fn new() -> Self {
        Self
    }

    pub fn build_voice_request(&self, room_jid: &str, _nick: &str) -> Element {
        Element::builder("message", NS_CLIENT)
            .attr("to", room_jid)
            .append(
                Element::builder("x", NS_MUC)
                    .append(Element::builder("decline", NS_MUC).build())
                    .build(),
            )
            .build()
    }

    pub fn build_approve_voice(&self, room_jid: &str, nick: &str) -> Element {
        Element::builder("message", NS_CLIENT)
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
        Element::builder("message", NS_CLIENT)
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
}

impl Default for MucVoiceManager {
    fn default() -> Self {
        Self::new()
    }
}
