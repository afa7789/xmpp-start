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

#[allow(dead_code)]
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. build_affiliation_query: GrantMember produces an IQ with affiliation="member".
    #[test]
    fn affiliation_grant_member_sets_correct_affiliation() {
        let mut mgr = MucAdminManager::new();
        let (id, iq) = mgr.build_affiliation_query(
            "room@conference.example.com",
            AffiliationAction::GrantMember("alice@example.com".to_string()),
        );

        assert!(!id.is_empty());
        assert_eq!(iq.attr("type"), Some("set"));
        assert_eq!(iq.attr("to"), Some("room@conference.example.com"));

        let query = iq
            .get_child("query", NS_MUC_ADMIN)
            .expect("<query> must exist");
        let item = query
            .get_child("item", NS_MUC_ADMIN)
            .expect("<item> must exist");

        assert_eq!(item.attr("affiliation"), Some("member"));
        assert_eq!(item.attr("jid"), Some("alice@example.com"));
    }

    // 2. build_affiliation_query: Ban produces affiliation="outcast".
    #[test]
    fn affiliation_ban_sets_outcast() {
        let mut mgr = MucAdminManager::new();
        let (_id, iq) = mgr.build_affiliation_query(
            "room@conference.example.com",
            AffiliationAction::Ban("troll@example.com".to_string()),
        );

        let item = iq
            .get_child("query", NS_MUC_ADMIN)
            .and_then(|q| q.get_child("item", NS_MUC_ADMIN))
            .expect("<item> must exist");

        assert_eq!(item.attr("affiliation"), Some("outcast"));
        assert_eq!(item.attr("jid"), Some("troll@example.com"));
    }

    // 3. build_affiliation_query: RevokeMembership produces affiliation="none".
    #[test]
    fn affiliation_revoke_sets_none() {
        let mut mgr = MucAdminManager::new();
        let (_id, iq) = mgr.build_affiliation_query(
            "room@conference.example.com",
            AffiliationAction::RevokeMembership("bob@example.com".to_string()),
        );

        let item = iq
            .get_child("query", NS_MUC_ADMIN)
            .and_then(|q| q.get_child("item", NS_MUC_ADMIN))
            .expect("<item> must exist");

        assert_eq!(item.attr("affiliation"), Some("none"));
        assert_eq!(item.attr("jid"), Some("bob@example.com"));
    }

    // 4. build_role_query: moderator role uses nick attribute, not jid.
    #[test]
    fn role_query_moderator_uses_nick() {
        let mut mgr = MucAdminManager::new();
        let (id, iq) = mgr.build_role_query("room@conference.example.com", "Alice", "moderator");

        assert!(!id.is_empty());
        assert_eq!(iq.attr("type"), Some("set"));

        let item = iq
            .get_child("query", NS_MUC_ADMIN)
            .and_then(|q| q.get_child("item", NS_MUC_ADMIN))
            .expect("<item> must exist");

        assert_eq!(item.attr("nick"), Some("Alice"));
        assert_eq!(item.attr("role"), Some("moderator"));
        // Role changes use nick, never jid.
        assert!(item.attr("jid").is_none());
    }

    // 5. build_role_query: kick is expressed as role="none".
    #[test]
    fn role_query_kick_sets_role_none() {
        let mut mgr = MucAdminManager::new();
        let (_id, iq) = mgr.build_role_query("room@conference.example.com", "Troublemaker", "none");

        let item = iq
            .get_child("query", NS_MUC_ADMIN)
            .and_then(|q| q.get_child("item", NS_MUC_ADMIN))
            .expect("<item> must exist");

        assert_eq!(item.attr("nick"), Some("Troublemaker"));
        assert_eq!(item.attr("role"), Some("none"));
    }

    // 6. build_affiliation_query: each call produces a unique IQ id.
    #[test]
    fn affiliation_query_ids_are_unique() {
        let mut mgr = MucAdminManager::new();
        let (id1, _) = mgr.build_affiliation_query(
            "room@conference.example.com",
            AffiliationAction::GrantMember("a@example.com".to_string()),
        );
        let (id2, _) = mgr.build_affiliation_query(
            "room@conference.example.com",
            AffiliationAction::GrantMember("b@example.com".to_string()),
        );
        assert_ne!(id1, id2);
    }
}
