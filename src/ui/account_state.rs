// MULTI: Per-account UI state isolation
//
// `AccountState` holds all UI state that must be kept separate per account.
// `AccountStateManager` is the container used by the App to route events and
// render the active account.

use std::collections::HashMap;

use iced::Color;

use crate::ui::avatar::jid_color;
use crate::ui::chat::ChatScreen;
use crate::xmpp::{AccountId, RosterContact};

// ---------------------------------------------------------------------------
// Per-account presence status (lightweight, UI-side only)
// ---------------------------------------------------------------------------

/// Simple online/offline flag as seen by the UI for a contact.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PresenceStatus {
    Available,
    Unavailable,
}

// ---------------------------------------------------------------------------
// AccountState
// ---------------------------------------------------------------------------

/// All per-account UI state.
#[allow(dead_code)]
pub struct AccountState {
    /// Conversation list, active conversation, composer drafts, etc.
    pub chat: ChatScreen,
    /// Roster contacts for this account.
    pub roster: Vec<RosterContact>,
    /// Contact presence keyed by bare JID.
    pub presence: HashMap<String, PresenceStatus>,
    /// Decoded avatar image handles keyed by bare JID.
    pub avatar_cache: HashMap<String, iced::widget::image::Handle>,
    /// Total unread messages across all conversations — used for sidebar badge.
    pub unread_total: usize,
}

impl AccountState {
    #[allow(dead_code)]
    fn new(jid: impl Into<String>) -> Self {
        Self {
            chat: ChatScreen::new(jid.into()),
            roster: Vec::new(),
            presence: HashMap::new(),
            avatar_cache: HashMap::new(),
            unread_total: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// AccountStateManager
// ---------------------------------------------------------------------------

/// Manages per-account UI state and tracks the active account.
#[allow(dead_code)]
pub struct AccountStateManager {
    accounts: HashMap<AccountId, AccountState>,
    active: Option<AccountId>,
}

#[allow(dead_code)]
impl AccountStateManager {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            active: None,
        }
    }

    /// Return the active account's state, if any.
    pub fn get_active(&self) -> Option<&AccountState> {
        self.active.as_ref().and_then(|id| self.accounts.get(id))
    }

    /// Return a mutable reference to the active account's state, if any.
    pub fn get_active_mut(&mut self) -> Option<&mut AccountState> {
        self.active
            .as_ref()
            .and_then(|id| self.accounts.get_mut(id))
    }

    /// Switch the active account to `id`. Returns the new active state, or
    /// `None` if `id` is not registered.
    pub fn switch_to(&mut self, id: &AccountId) -> Option<&AccountState> {
        if self.accounts.contains_key(id) {
            self.active = Some(id.clone());
            self.accounts.get(id)
        } else {
            None
        }
    }

    /// Register a new account (creates empty state). Returns a mutable
    /// reference to the newly created state. If the account is already
    /// registered the existing state is returned unchanged.
    /// The first account added becomes active automatically.
    pub fn add_account(&mut self, id: AccountId) -> &mut AccountState {
        let is_first = self.accounts.is_empty();
        let jid = id.0.clone();
        self.accounts.entry(id.clone()).or_insert_with(|| AccountState::new(jid));
        if is_first {
            self.active = Some(id.clone());
        }
        self.accounts.get_mut(&id).unwrap()
    }

    /// Remove an account and its state. If the removed account was active,
    /// the active account is cleared (caller should call `switch_to` with a
    /// remaining account).
    pub fn remove_account(&mut self, id: &AccountId) {
        self.accounts.remove(id);
        if self.active.as_ref() == Some(id) {
            self.active = None;
        }
    }

    /// Return the currently active `AccountId`, if any.
    pub fn active_id(&self) -> Option<&AccountId> {
        self.active.as_ref()
    }

    /// Iterate over all registered account IDs.
    pub fn account_ids(&self) -> impl Iterator<Item = &AccountId> {
        self.accounts.keys()
    }

    /// Returns `true` when more than one account is registered.
    pub fn is_multi_account(&self) -> bool {
        self.accounts.len() > 1
    }
}

impl Default for AccountStateManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Account color coding (XEP-0392)
// ---------------------------------------------------------------------------

/// Derive a consistent accent color for an account from its JID.
///
/// Delegates to the same XEP-0392 algorithm used for contact avatars so the
/// visual language is consistent across the UI.
pub fn account_color(id: &AccountId) -> Color {
    jid_color(id.as_str())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn id(jid: &str) -> AccountId {
        AccountId::new(jid)
    }

    // --- AccountStateManager ---

    #[test]
    fn add_first_account_becomes_active() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        assert_eq!(mgr.active_id().unwrap().as_str(), "alice@example.com");
    }

    #[test]
    fn add_second_account_does_not_change_active() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        mgr.add_account(id("bob@example.com"));
        assert_eq!(mgr.active_id().unwrap().as_str(), "alice@example.com");
    }

    #[test]
    fn switch_to_existing_account() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        mgr.add_account(id("bob@example.com"));
        let result = mgr.switch_to(&id("bob@example.com"));
        assert!(result.is_some());
        assert_eq!(mgr.active_id().unwrap().as_str(), "bob@example.com");
    }

    #[test]
    fn switch_to_unknown_account_returns_none() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        let result = mgr.switch_to(&id("unknown@example.com"));
        assert!(result.is_none());
        // Active should be unchanged
        assert_eq!(mgr.active_id().unwrap().as_str(), "alice@example.com");
    }

    #[test]
    fn remove_active_account_clears_active() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        mgr.remove_account(&id("alice@example.com"));
        assert!(mgr.active_id().is_none());
        assert!(mgr.get_active().is_none());
    }

    #[test]
    fn remove_inactive_account_leaves_active_unchanged() {
        let mut mgr = AccountStateManager::new();
        mgr.add_account(id("alice@example.com"));
        mgr.add_account(id("bob@example.com"));
        mgr.remove_account(&id("bob@example.com"));
        assert_eq!(mgr.active_id().unwrap().as_str(), "alice@example.com");
    }

    #[test]
    fn is_multi_account() {
        let mut mgr = AccountStateManager::new();
        assert!(!mgr.is_multi_account());
        mgr.add_account(id("alice@example.com"));
        assert!(!mgr.is_multi_account());
        mgr.add_account(id("bob@example.com"));
        assert!(mgr.is_multi_account());
    }

    // --- account_color ---

    #[test]
    fn account_color_is_deterministic() {
        let c1 = account_color(&id("alice@example.com"));
        let c2 = account_color(&id("alice@example.com"));
        assert_eq!(c1.r, c2.r);
        assert_eq!(c1.g, c2.g);
        assert_eq!(c1.b, c2.b);
    }

    #[test]
    fn account_color_differs_per_account() {
        let c1 = account_color(&id("alice@example.com"));
        let c2 = account_color(&id("bob@example.com"));
        // Colors are in valid range regardless
        assert!(c1.r >= 0.0 && c1.r <= 1.0);
        assert!(c2.r >= 0.0 && c2.r <= 1.0);
        // These two JIDs produce different hues
        let equal = (c1.r - c2.r).abs() < 1e-6
            && (c1.g - c2.g).abs() < 1e-6
            && (c1.b - c2.b).abs() < 1e-6;
        assert!(!equal, "different JIDs should produce different colors");
    }

    #[test]
    fn account_color_valid_range() {
        for jid in &["a@b.com", "carol@xmpp.org", "z@z.z"] {
            let c = account_color(&id(jid));
            assert!(c.r >= 0.0 && c.r <= 1.0);
            assert!(c.g >= 0.0 && c.g <= 1.0);
            assert!(c.b >= 0.0 && c.b <= 1.0);
        }
    }
}
