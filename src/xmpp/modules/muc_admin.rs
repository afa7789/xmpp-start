#![allow(dead_code)]
// S3: MUC Admin module — XEP-0045 §10 (Affiliations) & §11 (Moderation)
// Reference: https://xmpp.org/extensions/xep-0045.html
//
// Handles:
//   - Ban/unban users (outcast affiliation)
//   - Grant/revoke membership (member affiliation)
//   - Grant/revoke admin role
//   - Grant/revoke ownership
//
// Usage:
//   let (query_id, iq) = MucAdminManager::build_affiliation_query(&room_jid, AffiliationAction::Ban(jid));
//   let (query_id, iq) = MucAdminManager::build_role_query(&room_jid, RoleAction::RevokeVoice(nick));

use tokio_xmpp::minidom::Element;

use super::NS_CLIENT;

const NS_MUC_ADMIN: &str = "http://jabber.org/protocol/muc#admin";

#[derive(Debug, Clone)]
pub enum AffiliationAction {
    GrantOwner(String),
    GrantAdmin(String),
    GrantMember(String),
    RevokeMembership(String),
    Ban(String),
    Unban(String),
}

#[derive(Debug, Clone)]
pub enum RoleAction {
    GrantModerator(String),
    GrantVoice(String),
    RevokeVoice(String),
}

#[derive(Debug, Clone)]
pub struct MucAdminManager {
    pending_queries: std::collections::HashMap<String, MucAdminQuery>,
}

#[derive(Debug, Clone)]
pub enum MucAdminQuery {
    AffiliationQuery {
        room_jid: String,
        action: AffiliationAction,
    },
    RoleQuery {
        room_jid: String,
        nick: String,
        role: String,
    },
}

impl MucAdminManager {
    pub fn new() -> Self {
        Self {
            pending_queries: std::collections::HashMap::new(),
        }
    }

    pub fn build_affiliation_query(
        &mut self,
        room_jid: &str,
        action: AffiliationAction,
    ) -> (String, Element) {
        let query_id = format!("muc-affil-{}", uuid::Uuid::new_v4());
        let (affiliation, jid) = match &action {
            AffiliationAction::GrantOwner(j) => ("owner", Some(j.clone())),
            AffiliationAction::GrantAdmin(j) => ("admin", Some(j.clone())),
            AffiliationAction::GrantMember(j) => ("member", Some(j.clone())),
            AffiliationAction::RevokeMembership(j) => ("none", Some(j.clone())),
            AffiliationAction::Ban(j) => ("outcast", Some(j.clone())),
            AffiliationAction::Unban(j) => ("none", Some(j.clone())),
        };

        self.pending_queries.insert(
            query_id.clone(),
            MucAdminQuery::AffiliationQuery {
                room_jid: room_jid.to_string(),
                action: action.clone(),
            },
        );

        let item = if let Some(ref j) = jid {
            Element::builder("item", NS_MUC_ADMIN)
                .attr("affiliation", affiliation)
                .attr("jid", j)
                .build()
        } else {
            Element::builder("item", NS_MUC_ADMIN)
                .attr("affiliation", affiliation)
                .build()
        };

        let query = Element::builder("query", NS_MUC_ADMIN).append(item).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("id", &query_id)
            .attr("to", room_jid)
            .attr("type", "set")
            .append(query)
            .build();

        (query_id, iq)
    }

    pub fn build_role_query(
        &mut self,
        room_jid: &str,
        nick: &str,
        role: &str,
    ) -> (String, Element) {
        let query_id = format!("muc-role-{}", uuid::Uuid::new_v4());

        self.pending_queries.insert(
            query_id.clone(),
            MucAdminQuery::RoleQuery {
                room_jid: room_jid.to_string(),
                nick: nick.to_string(),
                role: role.to_string(),
            },
        );

        let item = Element::builder("item", NS_MUC_ADMIN)
            .attr("nick", nick)
            .attr("role", role)
            .build();

        let query = Element::builder("query", NS_MUC_ADMIN).append(item).build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("id", &query_id)
            .attr("to", room_jid)
            .attr("type", "set")
            .append(query)
            .build();

        (query_id, iq)
    }

    pub fn build_affiliation_list_query(&mut self, room_jid: &str) -> (String, Element) {
        let query_id = format!("muc-affil-list-{}", uuid::Uuid::new_v4());

        let query = Element::builder("query", NS_MUC_ADMIN)
            .attr("node", "all")
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("id", &query_id)
            .attr("to", room_jid)
            .attr("type", "get")
            .append(query)
            .build();

        (query_id, iq)
    }

    pub fn parse_affiliation_list(&self, el: &Element) -> Vec<(String, String)> {
        let mut results = Vec::new();
        for item in el
            .children()
            .filter(|c| c.name() == "item" && c.ns() == NS_MUC_ADMIN)
        {
            if let (Some(jid), Some(affil)) = (item.attr("jid"), item.attr("affiliation")) {
                results.push((jid.to_string(), affil.to_string()));
            }
        }
        results
    }

    pub fn on_result(&mut self, el: &Element) -> Option<MucAdminResult> {
        let query_id = el.attr("id")?;
        self.pending_queries.remove(query_id)?;
        Some(MucAdminResult::Success)
    }
}

#[derive(Debug, Clone)]
pub enum MucAdminResult {
    Success,
    Error(String),
}

impl Default for MucAdminManager {
    fn default() -> Self {
        Self::new()
    }
}
