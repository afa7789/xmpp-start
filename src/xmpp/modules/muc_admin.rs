// S3: MUC Admin module — XEP-0045 (Affiliations and Role Changes)
// Reference: https://xmpp.org/extensions/xep-0045.html
//
// Handles:
//   - Ban/unban users (outcast affiliation)
//   - Grant/revoke membership (member affiliation)
//   - Grant/revoke admin role
//   - Grant/revoke ownership
//   - Role changes (moderator, participant, visitor)

use tokio_xmpp::minidom::Element;

use super::{NS_CLIENT, NS_MUC_ADMIN};

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
pub struct MucAdminManager;

impl MucAdminManager {
    pub fn new() -> Self {
        Self
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
}

impl Default for MucAdminManager {
    fn default() -> Self {
        Self::new()
    }
}
