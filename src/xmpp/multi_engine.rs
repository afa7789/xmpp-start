// MULTI: Multi-engine manager
//
// Manages one XMPP engine task per configured account.
// Each engine runs in its own Tokio task and communicates via channel pairs.
// The manager routes commands to the correct engine and tags inbound events
// with the originating AccountId before forwarding them to the UI.

use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::config::AccountConfig;

use super::{engine::run_engine, AccountId, XmppCommand, XmppEvent};
use super::connection::ConnectConfig;

// ---------------------------------------------------------------------------
// Per-engine handle
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct EngineHandle {
    /// Command sender into this engine's loop.
    cmd_tx: mpsc::Sender<XmppCommand>,
}

// ---------------------------------------------------------------------------
// MultiEngineManager
// ---------------------------------------------------------------------------

/// Owns one XMPP engine per account and routes commands / events accordingly.
#[allow(dead_code)]
pub struct MultiEngineManager {
    /// Live engine handles, keyed by account JID.
    engines: HashMap<AccountId, EngineHandle>,
    /// The account whose events and commands are currently "in focus".
    active_account: AccountId,
}

#[allow(dead_code)]
impl MultiEngineManager {
    /// Create a manager with no engines.  `initial_active` is the account that
    /// will be considered active until `switch_active` is called.
    pub fn new(initial_active: AccountId) -> Self {
        Self {
            engines: HashMap::new(),
            active_account: initial_active,
        }
    }

    /// Spawn an engine task for `config` and connect it immediately.
    ///
    /// `event_tx` receives `(AccountId, XmppEvent)` pairs; the manager tags
    /// every event from this engine with the account's `AccountId` before
    /// forwarding so the UI can route events to the right account state.
    ///
    /// Calling this twice for the same account is a no-op (the existing engine
    /// is left running).
    pub fn start_account(
        &mut self,
        config: AccountConfig,
        event_tx: mpsc::Sender<(AccountId, XmppEvent)>,
    ) {
        let id = AccountId::new(config.jid.clone());

        if self.engines.contains_key(&id) {
            tracing::debug!("multi: engine for {} already running", id);
            return;
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<XmppCommand>(32);
        let (engine_event_tx, mut engine_event_rx) = mpsc::channel::<XmppEvent>(64);

        let account_id_clone = id.clone();
        // Bridge: engine emits XmppEvent → we tag it with AccountId and
        // forward to the shared event_tx.
        tokio::spawn(async move {
            while let Some(event) = engine_event_rx.recv().await {
                if event_tx.send((account_id_clone.clone(), event)).await.is_err() {
                    // UI dropped the receiver — stop relaying.
                    break;
                }
            }
        });

        // Spawn the engine loop.  Pass `None` for db — per-engine DB
        // integration is handled by the parent (multi-engine has no direct DB
        // access; OMEMO persistence uses the main app's pool passed at start).
        tokio::spawn(run_engine(engine_event_tx, cmd_rx, None));

        // Immediately issue a Connect command so the engine dials the server.
        // Password lookup from OS keychain is the caller's responsibility; here
        // we use password_key as a placeholder (real integration would read it).
        let connect_config = ConnectConfig {
            jid: config.jid.clone(),
            password: config.password_key.clone(),
            server: String::new(),
            status_message: None,
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
        };
        // Best-effort: if the channel is full the connect is deferred.
        let _ = cmd_tx.try_send(XmppCommand::Connect(connect_config));

        self.engines.insert(id.clone(), EngineHandle { cmd_tx });
        tracing::info!("multi: started engine for {}", id);
    }

    /// Disconnect and remove the engine for `id`.
    ///
    /// Sends a `Disconnect` command to the engine, then removes the handle.
    /// The engine task will exit naturally once the channel is dropped.
    pub fn stop_account(&mut self, id: &AccountId) {
        if let Some(handle) = self.engines.remove(id) {
            let _ = handle.cmd_tx.try_send(XmppCommand::Disconnect);
            tracing::info!("multi: stopped engine for {}", id);
        }

        // If the active account was stopped, clear to an empty sentinel.
        if &self.active_account == id {
            self.active_account = AccountId::new("");
        }
    }

    /// Send a command to the currently active account's engine.
    ///
    /// Returns `false` when there is no engine for the active account.
    pub fn send_to_active(&self, cmd: XmppCommand) -> bool {
        self.send_to(&self.active_account, cmd)
    }

    /// Send a command to a specific account's engine.
    ///
    /// Returns `false` when there is no engine for `id`.
    pub fn send_to(&self, id: &AccountId, cmd: XmppCommand) -> bool {
        if let Some(handle) = self.engines.get(id) {
            handle.cmd_tx.try_send(cmd).is_ok()
        } else {
            tracing::warn!("multi: no engine for account {}", id);
            false
        }
    }

    /// Change which account is currently in focus.
    pub fn switch_active(&mut self, id: AccountId) {
        tracing::info!("multi: switching active account to {}", id);
        self.active_account = id;
    }

    /// Returns the currently active account ID.
    pub fn active_account(&self) -> &AccountId {
        &self.active_account
    }

    /// Returns `true` when an engine is running for `id`.
    pub fn is_running(&self, id: &AccountId) -> bool {
        self.engines.contains_key(id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    type EventChannel = (
        mpsc::Sender<(AccountId, XmppEvent)>,
        mpsc::Receiver<(AccountId, XmppEvent)>,
    );

    fn make_event_rx() -> EventChannel {
        mpsc::channel(16)
    }

    #[tokio::test]
    async fn start_account_registers_engine() {
        let id = AccountId::new("alice@example.com");
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx);
        assert!(mgr.is_running(&id));
    }

    #[tokio::test]
    async fn start_account_idempotent() {
        let id = AccountId::new("alice@example.com");
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx.clone());
        mgr.start_account(AccountConfig::new("alice@example.com"), tx);
        // Still only one handle stored.
        assert_eq!(mgr.engines.len(), 1);
    }

    #[tokio::test]
    async fn stop_account_removes_engine() {
        let id = AccountId::new("alice@example.com");
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx);
        assert!(mgr.is_running(&id));

        mgr.stop_account(&id);
        assert!(!mgr.is_running(&id));
    }

    #[tokio::test]
    async fn stop_active_clears_active() {
        let id = AccountId::new("alice@example.com");
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx);
        mgr.stop_account(&id);
        // active_account is reset to the empty sentinel.
        assert_eq!(mgr.active_account().as_str(), "");
    }

    #[tokio::test]
    async fn switch_active_changes_focus() {
        let alice = AccountId::new("alice@example.com");
        let bob = AccountId::new("bob@example.com");
        let mut mgr = MultiEngineManager::new(alice.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("bob@example.com"), tx);
        mgr.switch_active(bob.clone());
        assert_eq!(mgr.active_account(), &bob);
    }

    #[tokio::test]
    async fn send_to_unknown_returns_false() {
        let id = AccountId::new("alice@example.com");
        let mgr = MultiEngineManager::new(id.clone());

        let result = mgr.send_to(&id, XmppCommand::Disconnect);
        assert!(!result);
    }

    #[tokio::test]
    async fn send_to_active_unknown_returns_false() {
        let id = AccountId::new("nobody@example.com");
        let mgr = MultiEngineManager::new(id);

        let result = mgr.send_to_active(XmppCommand::Disconnect);
        assert!(!result);
    }

    #[tokio::test]
    async fn multiple_accounts_independent() {
        let alice = AccountId::new("alice@example.com");
        let bob = AccountId::new("bob@example.com");
        let mut mgr = MultiEngineManager::new(alice.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx.clone());
        mgr.start_account(AccountConfig::new("bob@example.com"), tx);

        assert!(mgr.is_running(&alice));
        assert!(mgr.is_running(&bob));
        assert_eq!(mgr.engines.len(), 2);

        mgr.stop_account(&alice);
        assert!(!mgr.is_running(&alice));
        assert!(mgr.is_running(&bob));
    }
}
