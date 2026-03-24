// S3: MUC Room Configuration module — XEP-0045 §6 (Room Configuration)
// Reference: https://xmpp.org/extensions/xep-0045.html
//
// Handles:
//   - Request room configuration form (XEP-0004)
//   - Submit room configuration
//   - Parse configuration options

use tokio_xmpp::minidom::Element;

use super::{NS_CLIENT, NS_MUC_OWNER};

#[derive(Debug, Clone)]
pub struct MucConfigManager {
    pending_queries: std::collections::HashMap<String, MucConfigQuery>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MucConfigQuery {
    RequestConfig { room_jid: String },
    SubmitConfig { room_jid: String },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MucRoomConfig {
    pub room_name: Option<String>,
    pub room_description: Option<String>,
    pub max_history_fetch: Option<i32>,
    pub allow_private_messages: Option<bool>,
    pub allow_public_messages: Option<bool>,
    pub public: Option<bool>,
    pub persistent_room: Option<bool>,
    pub password_protected: Option<bool>,
    pub password: Option<String>,
    pub whois: Option<String>,
    pub max_users: Option<u32>,
}

impl Default for MucRoomConfig {
    fn default() -> Self {
        Self {
            room_name: None,
            room_description: None,
            max_history_fetch: Some(50),
            allow_private_messages: Some(true),
            allow_public_messages: Some(true),
            public: Some(true),
            persistent_room: Some(true),
            password_protected: Some(false),
            password: None,
            whois: Some("anyone".to_string()),
            max_users: Some(200),
        }
    }
}

impl MucConfigManager {
    pub fn new() -> Self {
        Self {
            pending_queries: std::collections::HashMap::new(),
        }
    }

    pub fn build_config_request(&mut self, room_jid: &str) -> (String, Element) {
        let query_id = format!("muc-config-req-{}", uuid::Uuid::new_v4());

        self.pending_queries.insert(
            query_id.clone(),
            MucConfigQuery::RequestConfig {
                room_jid: room_jid.to_string(),
            },
        );

        let query = Element::builder("query", NS_MUC_OWNER).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("id", &query_id)
            .attr("to", room_jid)
            .attr("type", "get")
            .append(query)
            .build();

        (query_id, iq)
    }

    pub fn build_config_submit(
        &mut self,
        room_jid: &str,
        config: &MucRoomConfig,
    ) -> (String, Element) {
        let query_id = format!("muc-config-submit-{}", uuid::Uuid::new_v4());

        self.pending_queries.insert(
            query_id.clone(),
            MucConfigQuery::SubmitConfig {
                room_jid: room_jid.to_string(),
            },
        );

        let mut form = Element::builder("x", "jabber:x:data")
            .attr("type", "submit")
            .build();

        for field in self.config_to_form_fields(config) {
            form.append_child(field);
        }

        let query = Element::builder("query", NS_MUC_OWNER).append(form).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("id", &query_id)
            .attr("to", room_jid)
            .attr("type", "set")
            .append(query)
            .build();

        (query_id, iq)
    }

    fn config_to_form_fields(&self, config: &MucRoomConfig) -> Vec<Element> {
        let mut fields = Vec::new();

        fields.push(self.text_field(
            "FORM_TYPE",
            Some("http://jabber.org/protocol/muc#roomconfig"),
            true,
        ));

        if let Some(ref name) = config.room_name {
            fields.push(self.text_field("muc#roomconfig_roomname", Some(name), false));
        }
        if let Some(ref desc) = config.room_description {
            fields.push(self.text_field("muc#roomconfig_roomdesc", Some(desc), false));
        }
        if let Some(max) = config.max_users {
            fields.push(self.text_field("muc#roomconfig_maxusers", Some(&max.to_string()), false));
        }
        if let Some(pub_val) = config.public {
            fields.push(self.bool_field("muc#roomconfig_publicroom", pub_val));
        }
        if let Some(persistent) = config.persistent_room {
            fields.push(self.bool_field("muc#roomconfig_persistentroom", persistent));
        }
        if let Some(whois) = &config.whois {
            fields.push(self.text_field("muc#roomconfig_whois", Some(whois), false));
        }
        if let Some(max_hist) = config.max_history_fetch {
            fields.push(self.text_field(
                "muc#roomconfig_maxstanzas",
                Some(&max_hist.to_string()),
                false,
            ));
        }

        fields
    }

    fn text_field(&self, var: &str, value: Option<&str>, hidden: bool) -> Element {
        let field_type = if hidden { "hidden" } else { "text-single" };
        let mut field = Element::builder("field", "jabber:x:data")
            .attr("var", var)
            .attr("type", field_type)
            .build();

        if let Some(v) = value {
            let value_el = Element::builder("value", "jabber:x:data").append(v).build();
            field.append_child(value_el);
        }

        field
    }

    fn bool_field(&self, var: &str, value: bool) -> Element {
        let mut field = Element::builder("field", "jabber:x:data")
            .attr("var", var)
            .attr("type", "boolean")
            .build();

        let value_el = Element::builder("value", "jabber:x:data")
            .append(if value { "1" } else { "0" })
            .build();
        field.append_child(value_el);

        field
    }

    pub fn parse_config_form(&self, el: &Element) -> Option<MucRoomConfig> {
        let form = el
            .children()
            .find(|c| c.name() == "x" && c.ns() == "jabber:x:data")?;
        let mut config = MucRoomConfig::default();

        for field in form.children().filter(|c| c.name() == "field") {
            let var = field.attr("var")?;
            let value = field
                .children()
                .find(|c| c.name() == "value")
                .map(tokio_xmpp::minidom::Element::text);

            match var {
                "muc#roomconfig_roomname" => config.room_name = value,
                "muc#roomconfig_roomdesc" => config.room_description = value,
                "muc#roomconfig_maxusers" => {
                    config.max_users = value.and_then(|v| v.parse().ok());
                }
                "muc#roomconfig_publicroom" => {
                    config.public = value.map(|v| v == "1");
                }
                "muc#roomconfig_persistentroom" => {
                    config.persistent_room = value.map(|v| v == "1");
                }
                "muc#roomconfig_whois" => config.whois = value,
                _ => {}
            }
        }

        Some(config)
    }
}

impl Default for MucConfigManager {
    fn default() -> Self {
        Self::new()
    }
}
