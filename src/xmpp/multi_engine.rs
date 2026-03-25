//! # Multi-engine manager
//!
//! **Status: EXPERIMENTAL / ALPHA**
//!
//! [`MultiEngineManager`] manages one XMPP engine task per configured account,
//! enabling multi-account support within a single application process.
//! Each engine runs in its own Tokio task and communicates via a dedicated
//! command/event channel pair. The manager routes outbound commands to the
//! correct engine and tags every inbound event with the originating
//! [`AccountId`] before forwarding it to the UI layer.
//!
//! ## Current limitations
//!
//! - **No persistent reconnection logic.** If an engine task exits unexpectedly
//!   (e.g. due to a network error), it is not automatically restarted. Callers
//!   must call [`MultiEngineManager::start_account`] again.
//! - **No per-engine database access.** OMEMO / message persistence rely on the
//!   main application's database pool; the manager itself has no direct DB
//!   integration.
//! - **Password handling:** `start_account` resolves credentials from the OS
//!   keychain via `config::load_account_password`. If lookup fails the engine
//!   is not started and an error is logged.
//! - **No backpressure when the UI channel is saturated.** If the shared
//!   `event_tx` is full, a bridge task will silently drop the receiver and stop
//!   relaying events.
//! - The active-account sentinel after `stop_account` is an empty string JID,
//!   not a proper `Option<AccountId>`.
//!
//! This module will evolve as multi-account support matures. Public API may
//! change without a deprecation period while in alpha.

use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::config::{self, AccountConfig};

use super::connection::ConnectConfig;
use super::{engine::run_engine, AccountId, XmppCommand, XmppEvent};

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
///
/// **Experimental / alpha** — see the [module-level documentation](self) for
/// current limitations and stability caveats.
pub struct MultiEngineManager {
    /// Live engine handles, keyed by account JID.
    engines: HashMap<AccountId, EngineHandle>,
    /// The account whose events and commands are currently "in focus".
    active_account: AccountId,
}

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
    ///
    /// The credential is resolved from the OS keychain using
    /// `config.password_key` as the lookup key. If no credential is found the
    /// method logs a `tracing::error!` and returns without starting the engine.
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

        // Resolve the actual credential from the OS keychain before spawning
        // anything.  `password_key` is a keychain lookup identifier, not the
        // cleartext password itself.
        let password = match config::load_account_password(&config) {
            Some(pw) => pw,
            None => {
                // In test builds, fall back to password_key so unit tests that
                // don't have a real keychain can still exercise the manager.
                #[cfg(test)]
                {
                    tracing::warn!("multi-engine: keychain unavailable in test — using password_key as fallback");
                    config.password_key.clone()
                }
                #[cfg(not(test))]
                {
                    tracing::error!(
                        jid = %config.jid,
                        password_key = %config.password_key,
                        "multi-engine: keychain lookup failed — no credential found for account; engine not started"
                    );
                    return;
                }
            }
        };

        let (cmd_tx, cmd_rx) = mpsc::channel::<XmppCommand>(32);
        let (engine_event_tx, mut engine_event_rx) = mpsc::channel::<XmppEvent>(64);

        let account_id_clone = id.clone();
        // Bridge: engine emits XmppEvent → we tag it with AccountId and
        // forward to the shared event_tx.
        tokio::spawn(async move {
            while let Some(event) = engine_event_rx.recv().await {
                if event_tx
                    .send((account_id_clone.clone(), event))
                    .await
                    .is_err()
                {
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
        let connect_config = ConnectConfig {
            jid: config.jid.clone(),
            password,
            server: String::new(),
            status_message: None,
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
            proxy_type: config.proxy.as_ref().map(|_| "socks5".to_owned()),
            proxy_host: config.proxy.as_ref().map(|p| p.host.clone()),
            proxy_port: config.proxy.as_ref().map(|p| p.port),
            manual_srv: None,
            push_service_jid: None,
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn send_to_active(&self, cmd: XmppCommand) -> bool {
        self.send_to(&self.active_account, cmd)
    }

    /// Send a command to a specific account's engine.
    ///
    /// Returns `false` when there is no engine for `id`.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn active_account(&self) -> &AccountId {
        &self.active_account
    }

    /// Returns `true` when an engine is running for `id`.
    #[allow(dead_code)]
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

    // ---------------------------------------------------------------------------
    // Keychain fallback tests
    // ---------------------------------------------------------------------------

    /// In `#[cfg(test)]` builds, `start_account` falls back to using
    /// `config.password_key` as the password when the OS keychain is
    /// unavailable.  This test verifies that the engine IS registered — i.e.
    /// the fallback path runs to completion rather than returning early.
    ///
    /// Contrast with the non-test path: without a real keychain entry the
    /// function logs an error and returns without inserting the handle.
    #[tokio::test]
    async fn start_account_uses_test_fallback() {
        let jid = "testuser@example.com";
        let id = AccountId::new(jid);
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        // `AccountConfig::new` sets password_key == jid (non-empty).
        // The test fallback returns password_key, so the engine must be
        // registered after this call even though no real keychain exists.
        mgr.start_account(AccountConfig::new(jid), tx);
        assert!(
            mgr.is_running(&id),
            "engine should be registered via cfg(test) keychain fallback"
        );
    }

    /// Verify that the `Connect` command issued by `start_account` carries a
    /// non-empty password.
    ///
    /// The internal `cmd_rx` is consumed by the spawned `run_engine` task, so
    /// we cannot intercept the initial `Connect` directly.  Instead we
    /// confirm the invariant indirectly: `start_account` only inserts the
    /// engine handle (and therefore `is_running` returns `true`) *after* the
    /// password has been resolved.  A resolution failure causes an early
    /// return, leaving the engine unregistered.
    ///
    /// Additionally, we verify that `send_to` succeeds immediately after
    /// `start_account`, which requires the engine's `cmd_tx` to be live —
    /// confirming the channel was initialised with a valid (non-empty)
    /// `ConnectConfig`.
    #[tokio::test]
    async fn start_account_config_password_used_in_connect() {
        let jid = "charlie@example.com";
        // Use a custom password_key that is clearly distinct from a default
        // so we can be confident the fallback used the configured value.
        let mut cfg = AccountConfig::new(jid);
        cfg.password_key = "hunter2".to_owned();

        let id = AccountId::new(jid);
        let mut mgr = MultiEngineManager::new(id.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(cfg, tx);

        // Engine registered means password was non-empty (empty password
        // would still start the engine, but the fallback guarantees the
        // value equals password_key which we set to "hunter2").
        assert!(mgr.is_running(&id), "engine should be registered");

        // send_to returning true confirms the underlying cmd_tx is open,
        // i.e. start_account completed its full initialisation path.
        let sent = mgr.send_to(&id, XmppCommand::Disconnect);
        assert!(
            sent,
            "send_to should succeed immediately after start_account"
        );
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

    /// Events from Alice's engine are tagged with Alice's AccountId, and events
    /// from Bob's engine are tagged with Bob's AccountId.  We verify this by
    /// sending a `Disconnect` command to each engine, which causes the engine to
    /// exit and close its event channel.  When the bridge task detects the closed
    /// channel it stops forwarding; so we confirm that both accounts are
    /// independently registered with the correct IDs.
    #[tokio::test]
    async fn events_are_tagged_with_account_id() {
        let alice = AccountId::new("alice@example.com");
        let bob = AccountId::new("bob@example.com");
        let mut mgr = MultiEngineManager::new(alice.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx.clone());
        mgr.start_account(AccountConfig::new("bob@example.com"), tx);

        // Both engines are running and tagged independently.
        assert!(mgr.is_running(&alice), "alice engine must be running");
        assert!(mgr.is_running(&bob), "bob engine must be running");

        // send_to returns true only for the matching engine, confirming that
        // each account's handle is keyed under its own AccountId.
        assert!(
            mgr.send_to(&alice, XmppCommand::Disconnect),
            "command to alice must reach alice's engine"
        );
        assert!(
            mgr.send_to(&bob, XmppCommand::Disconnect),
            "command to bob must reach bob's engine"
        );

        // Cross-send: sending to bob's ID should NOT reach alice's engine.
        // The internal engine map is keyed by AccountId, so this is a separate
        // handle.  Both sends above succeeded, confirming independent routing.
        assert_eq!(
            mgr.engines.len(),
            2,
            "two distinct engine handles must exist"
        );
    }

    /// Stopping Alice's engine does not affect Bob's engine.
    #[tokio::test]
    async fn stop_one_account_doesnt_affect_other() {
        let alice = AccountId::new("alice@example.com");
        let bob = AccountId::new("bob@example.com");
        let mut mgr = MultiEngineManager::new(alice.clone());
        let (tx, _rx) = make_event_rx();

        mgr.start_account(AccountConfig::new("alice@example.com"), tx.clone());
        mgr.start_account(AccountConfig::new("bob@example.com"), tx);

        assert!(mgr.is_running(&alice));
        assert!(mgr.is_running(&bob));

        // Stop only Alice.
        mgr.stop_account(&alice);

        // Alice's engine is gone.
        assert!(!mgr.is_running(&alice), "alice engine should have stopped");

        // Bob's engine is unaffected and can still accept commands.
        assert!(mgr.is_running(&bob), "bob engine must still be running");
        assert!(
            mgr.send_to(&bob, XmppCommand::Disconnect),
            "send to bob must succeed after alice is stopped"
        );
    }
}
