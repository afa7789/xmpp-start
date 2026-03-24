#![allow(dead_code)]
// Task P6.3 — XEP-0077 In-Band Registration: account management IQs
// XEP reference: https://xmpp.org/extensions/xep-0077.html
//
// Builds change-password and delete-account IQs.
// Pure — no I/O, no async.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_REGISTER};

// ---------------------------------------------------------------------------
// AccountManager
// ---------------------------------------------------------------------------

/// XEP-0077 account management IQ builder.
///
/// All methods are pure: no I/O, no async.
pub struct AccountManager;

impl AccountManager {
    pub fn new() -> Self {
        Self
    }

    /// Build an IQ to change the account password.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}">
    ///   <query xmlns="jabber:iq:register">
    ///     <username>{username}</username>
    ///     <password>{new_password}</password>
    ///   </query>
    /// </iq>
    /// ```
    pub fn build_change_password_iq(&self, username: &str, new_password: &str) -> Element {
        let id = Uuid::new_v4().to_string();

        let username_el = Element::builder("username", NS_REGISTER)
            .append(username)
            .build();
        let password_el = Element::builder("password", NS_REGISTER)
            .append(new_password)
            .build();
        let query = Element::builder("query", NS_REGISTER)
            .append(username_el)
            .append(password_el)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(query)
            .build()
    }

    /// Build an IQ to delete (unregister) the account.
    ///
    /// ```xml
    /// <iq type="set" id="{uuid}">
    ///   <query xmlns="jabber:iq:register">
    ///     <remove/>
    ///   </query>
    /// </iq>
    /// ```
    pub fn build_delete_account_iq(&self) -> Element {
        let id = Uuid::new_v4().to_string();

        let remove_el = Element::builder("remove", NS_REGISTER).build();
        let query = Element::builder("query", NS_REGISTER)
            .append(remove_el)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(query)
            .build()
    }
}

impl Default for AccountManager {
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

    // 1 -----------------------------------------------------------------------
    #[test]
    fn change_password_has_correct_ns() {
        let mgr = AccountManager::new();
        let iq = mgr.build_change_password_iq("alice", "s3cr3t!");

        assert_eq!(iq.attr("type"), Some("set"));
        assert!(!iq.attr("id").unwrap_or("").is_empty());

        let query = iq
            .children()
            .find(|c| c.name() == "query")
            .expect("expected <query> child");
        assert_eq!(query.ns(), NS_REGISTER);

        let username_el = query
            .children()
            .find(|c| c.name() == "username")
            .expect("expected <username> child");
        assert_eq!(username_el.text(), "alice");

        let password_el = query
            .children()
            .find(|c| c.name() == "password")
            .expect("expected <password> child");
        assert_eq!(password_el.text(), "s3cr3t!");
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn delete_account_has_remove_element() {
        let mgr = AccountManager::new();
        let iq = mgr.build_delete_account_iq();

        assert_eq!(iq.attr("type"), Some("set"));

        let query = iq
            .children()
            .find(|c| c.name() == "query")
            .expect("expected <query> child");
        assert_eq!(query.ns(), NS_REGISTER);

        let remove_el = query
            .children()
            .find(|c| c.name() == "remove")
            .expect("expected <remove> child");
        assert_eq!(remove_el.ns(), NS_REGISTER);
    }
}
