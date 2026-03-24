// Task P6.3 — XEP-0077 In-Band Registration: account management IQs
// XEP reference: https://xmpp.org/extensions/xep-0077.html
//
// Builds change-password and delete-account IQs.
// Tracks pending IQ IDs so the engine can correlate results.
// Pure — no I/O, no async.

use std::collections::HashSet;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::{NS_CLIENT, NS_REGISTER};

// ---------------------------------------------------------------------------
// AccountManager
// ---------------------------------------------------------------------------

/// XEP-0077 account management IQ builder.
///
/// Tracks in-flight IQ IDs so callers can detect when a result belongs to
/// a change-password or delete-account request.
///
/// All methods are synchronous: no I/O, no async.
pub struct AccountManager {
    /// IQ IDs for in-flight change-password requests.
    pending_change_password: HashSet<String>,
    /// IQ IDs for in-flight delete-account requests.
    pending_delete_account: HashSet<String>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            pending_change_password: HashSet::new(),
            pending_delete_account: HashSet::new(),
        }
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
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_change_password_iq(&mut self, username: &str, new_password: &str) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        self.pending_change_password.insert(id.clone());

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

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(query)
            .build();

        (id, iq)
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
    ///
    /// Returns `(iq_id, element)`.
    pub fn build_delete_account_iq(&mut self) -> (String, Element) {
        let id = Uuid::new_v4().to_string();
        self.pending_delete_account.insert(id.clone());

        let remove_el = Element::builder("remove", NS_REGISTER).build();
        let query = Element::builder("query", NS_REGISTER)
            .append(remove_el)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "set")
            .attr("id", &id)
            .append(query)
            .build();

        (id, iq)
    }

    /// Call when an IQ `result` or `error` stanza arrives.
    ///
    /// Returns `Some(AccountIqResult)` if the IQ id was one we sent;
    /// returns `None` if the stanza is unrelated to this manager.
    pub fn on_iq_result(&mut self, el: &Element) -> Option<AccountIqResult> {
        let iq_type = el.attr("type")?;
        let iq_id = el.attr("id")?;

        if self.pending_change_password.remove(iq_id) {
            return Some(AccountIqResult {
                kind: AccountIqKind::ChangePassword,
                success: iq_type == "result",
            });
        }
        if self.pending_delete_account.remove(iq_id) {
            return Some(AccountIqResult {
                kind: AccountIqKind::DeleteAccount,
                success: iq_type == "result",
            });
        }

        None
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Which account operation produced a result.
#[derive(Debug, Clone, PartialEq)]
pub enum AccountIqKind {
    ChangePassword,
    DeleteAccount,
}

/// Outcome of a change-password or delete-account IQ.
#[derive(Debug, Clone)]
pub struct AccountIqResult {
    pub kind: AccountIqKind,
    pub success: bool,
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
        let mut mgr = AccountManager::new();
        let (id, iq) = mgr.build_change_password_iq("alice", "s3cr3t!");

        assert!(!id.is_empty());
        assert_eq!(iq.attr("type"), Some("set"));
        assert_eq!(iq.attr("id"), Some(id.as_str()));

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
        let mut mgr = AccountManager::new();
        let (id, iq) = mgr.build_delete_account_iq();

        assert!(!id.is_empty());
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

    // 3 -----------------------------------------------------------------------
    #[test]
    fn on_iq_result_matches_change_password() {
        let mut mgr = AccountManager::new();
        let (id, _iq) = mgr.build_change_password_iq("alice", "newpass");

        let result_el = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .build();

        let result = mgr.on_iq_result(&result_el).expect("expected a result");
        assert_eq!(result.kind, AccountIqKind::ChangePassword);
        assert!(result.success);

        // Consumed — second call returns None.
        assert!(mgr.on_iq_result(&result_el).is_none());
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn on_iq_result_matches_delete_account() {
        let mut mgr = AccountManager::new();
        let (id, _iq) = mgr.build_delete_account_iq();

        let result_el = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", &id)
            .build();

        let result = mgr.on_iq_result(&result_el).expect("expected a result");
        assert_eq!(result.kind, AccountIqKind::DeleteAccount);
        assert!(result.success);
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn on_iq_result_detects_error() {
        let mut mgr = AccountManager::new();
        let (id, _iq) = mgr.build_change_password_iq("alice", "newpass");

        let error_el = Element::builder("iq", NS_CLIENT)
            .attr("type", "error")
            .attr("id", &id)
            .build();

        let result = mgr.on_iq_result(&error_el).expect("expected a result");
        assert_eq!(result.kind, AccountIqKind::ChangePassword);
        assert!(!result.success);
    }

    // 6 -----------------------------------------------------------------------
    #[test]
    fn on_iq_result_ignores_unknown_id() {
        let mut mgr = AccountManager::new();
        let el = Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", "unknown-id")
            .build();
        assert!(mgr.on_iq_result(&el).is_none());
    }
}
