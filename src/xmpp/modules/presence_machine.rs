#![allow(dead_code)]
// Task P1.4b — Presence state machine (auto-away / xa / DND)
//
// Ports presenceMachine.ts (XState, 6 states) to pure Rust.
// No async, no I/O. The engine polls `effective_status()` and calls
// `build_presence_stanza()` whenever the result changes.
//
// Invariants:
//   - DND: idle/sleep events are silently ignored.
//   - pre_auto_away_status is saved on first entry to AutoAway and reused
//     when the machine enters AutoXa (so activity always restores the
//     same user status regardless of how deep the auto chain went).
//   - on_connected does NOT reset auto_state — preserve across reconnect.

use tokio_xmpp::minidom::Element;

const NS_CLIENT: &str = "jabber:client";

/// The full set of presence values visible to the outside world and to the
/// XMPP server.
#[derive(Debug, Clone, PartialEq)]
pub enum PresenceStatus {
    Available,
    Away,
    ExtendedAway, // xa
    DoNotDisturb, // dnd
    Offline,
}

/// Internal auto-idle layer.  Layered on top of `user_status`.
#[derive(Debug, Clone, PartialEq)]
enum AutoState {
    Active,
    AutoAway,
    AutoXa,
}

/// Pure presence state machine.
///
/// The engine is responsible for:
///   1. Calling `on_idle_detected` / `on_sleep_detected` / `on_activity_detected`
///      based on OS idle timers.
///   2. Calling `on_connected` / `on_disconnected` on session changes.
///   3. Polling `effective_status()` to detect transitions and broadcast them.
#[derive(Debug, Clone)]
pub struct PresenceMachine {
    /// Explicit status set by the user (never auto-modified).
    user_status: PresenceStatus,
    /// Auto-idle state layer on top of user_status.
    auto_state: AutoState,
    /// The status we had before entering auto-away, to restore on activity.
    pre_auto_away_status: PresenceStatus,
    /// True when the session is connected.
    online: bool,
}

impl PresenceMachine {
    /// Creates a new machine in the disconnected/active state.
    pub fn new() -> Self {
        Self {
            user_status: PresenceStatus::Available,
            auto_state: AutoState::Active,
            pre_auto_away_status: PresenceStatus::Available,
            online: false,
        }
    }

    /// User explicitly sets their status.
    ///
    /// Resets auto_state to Active and clears the saved pre-auto status.
    pub fn set_user_status(&mut self, status: PresenceStatus) {
        self.user_status = status.clone();
        self.pre_auto_away_status = status;
        self.auto_state = AutoState::Active;
    }

    /// OS reported idle — transition to AutoAway unless DND or already deeper.
    pub fn on_idle_detected(&mut self) {
        if self.user_status == PresenceStatus::DoNotDisturb {
            return;
        }
        if self.auto_state == AutoState::Active {
            self.pre_auto_away_status = self.user_status.clone();
            self.auto_state = AutoState::AutoAway;
        }
    }

    /// OS reported extended idle / sleep — transition to AutoXa unless DND.
    pub fn on_sleep_detected(&mut self) {
        if self.user_status == PresenceStatus::DoNotDisturb {
            return;
        }
        // Save the pre-auto status only on the first transition out of Active.
        if self.auto_state == AutoState::Active {
            self.pre_auto_away_status = self.user_status.clone();
        }
        self.auto_state = AutoState::AutoXa;
    }

    /// Any user activity — restore to the status we had before auto-away.
    pub fn on_activity_detected(&mut self) {
        if self.auto_state != AutoState::Active {
            self.user_status = self.pre_auto_away_status.clone();
            self.auto_state = AutoState::Active;
        }
    }

    /// Called on connect. Marks the session as online.
    ///
    /// Does NOT reset auto_state so idle state is preserved across reconnects.
    pub fn on_connected(&mut self) {
        self.online = true;
    }

    /// Called on disconnect.
    pub fn on_disconnected(&mut self) {
        self.online = false;
    }

    /// The effective presence to broadcast.
    ///
    /// - Returns `Offline` when not connected.
    /// - DND is never auto-overridden; it stays DND regardless of auto_state.
    /// - Otherwise, AutoAway → Away, AutoXa → ExtendedAway, Active → user_status.
    pub fn effective_status(&self) -> PresenceStatus {
        if !self.online {
            return PresenceStatus::Offline;
        }
        if self.user_status == PresenceStatus::DoNotDisturb {
            return PresenceStatus::DoNotDisturb;
        }
        match self.auto_state {
            AutoState::Active => self.user_status.clone(),
            AutoState::AutoAway => PresenceStatus::Away,
            AutoState::AutoXa => PresenceStatus::ExtendedAway,
        }
    }

    /// Build the `<presence>` stanza to send.
    ///
    /// Returns `None` when offline (no stanza should be sent).
    ///
    /// Stanza format:
    /// ```xml
    /// <presence xmlns="jabber:client">
    ///   <show>away</show>   <!-- away | xa | dnd — omitted for available -->
    ///   <status>Away</status>
    /// </presence>
    /// ```
    /// Available produces an empty `<presence/>` (no `<show>`, no `<status>`).
    pub fn build_presence_stanza(&self) -> Option<Element> {
        let status = self.effective_status();
        if status == PresenceStatus::Offline {
            return None;
        }

        let builder = Element::builder("presence", NS_CLIENT);

        let el = match status {
            PresenceStatus::Available => builder.build(),
            PresenceStatus::Away => builder
                .append(Element::builder("show", NS_CLIENT).append("away").build())
                .build(),
            PresenceStatus::ExtendedAway => builder
                .append(Element::builder("show", NS_CLIENT).append("xa").build())
                .build(),
            PresenceStatus::DoNotDisturb => builder
                .append(Element::builder("show", NS_CLIENT).append("dnd").build())
                .build(),
            PresenceStatus::Offline => unreachable!("handled above"),
        };

        Some(el)
    }
}

impl Default for PresenceMachine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn connected_machine() -> PresenceMachine {
        let mut m = PresenceMachine::new();
        m.on_connected();
        m
    }

    // 1. Initial state
    #[test]
    fn initial_state_is_active_available() {
        let mut m = PresenceMachine::new();
        // Before connecting, status should be Offline.
        assert_eq!(m.effective_status(), PresenceStatus::Offline);

        m.on_connected();
        assert_eq!(m.effective_status(), PresenceStatus::Available);
        assert_eq!(m.auto_state, AutoState::Active);
    }

    // 2. Idle → auto-away
    #[test]
    fn idle_transitions_to_auto_away() {
        let mut m = connected_machine();
        m.on_idle_detected();
        assert_eq!(m.effective_status(), PresenceStatus::Away);
        assert_eq!(m.auto_state, AutoState::AutoAway);
    }

    // 3. Sleep from Active → xa
    #[test]
    fn sleep_from_active_transitions_to_xa() {
        let mut m = connected_machine();
        m.on_sleep_detected();
        assert_eq!(m.effective_status(), PresenceStatus::ExtendedAway);
        assert_eq!(m.auto_state, AutoState::AutoXa);
    }

    // 4. Activity from AutoAway restores pre-auto status
    #[test]
    fn activity_from_auto_away_restores_pre_state() {
        let mut m = connected_machine();
        m.set_user_status(PresenceStatus::Away); // user manually set Away
        m.on_connected(); // reconnect keeps online = true
        m.on_idle_detected(); // auto-away on top of user Away
        assert_eq!(m.effective_status(), PresenceStatus::Away); // still Away (auto == user)

        // Now with Available
        let mut m2 = connected_machine();
        m2.on_idle_detected();
        assert_eq!(m2.effective_status(), PresenceStatus::Away);
        m2.on_activity_detected();
        assert_eq!(m2.effective_status(), PresenceStatus::Available);
        assert_eq!(m2.auto_state, AutoState::Active);
    }

    // 5. Activity from AutoXa restores pre-auto status
    #[test]
    fn activity_from_xa_restores_pre_state() {
        let mut m = connected_machine();
        m.on_idle_detected();
        m.on_sleep_detected(); // now AutoXa, pre_auto = Available
        assert_eq!(m.effective_status(), PresenceStatus::ExtendedAway);

        m.on_activity_detected();
        assert_eq!(m.effective_status(), PresenceStatus::Available);
        assert_eq!(m.auto_state, AutoState::Active);
    }

    // 6. DND ignores idle
    #[test]
    fn dnd_ignores_idle() {
        let mut m = connected_machine();
        m.set_user_status(PresenceStatus::DoNotDisturb);
        m.on_idle_detected();
        assert_eq!(m.effective_status(), PresenceStatus::DoNotDisturb);
        assert_eq!(m.auto_state, AutoState::Active);
    }

    // 7. DND ignores sleep
    #[test]
    fn dnd_ignores_sleep() {
        let mut m = connected_machine();
        m.set_user_status(PresenceStatus::DoNotDisturb);
        m.on_sleep_detected();
        assert_eq!(m.effective_status(), PresenceStatus::DoNotDisturb);
        assert_eq!(m.auto_state, AutoState::Active);
    }

    // 8. Offline when not connected
    #[test]
    fn effective_status_offline_when_not_connected() {
        let m = PresenceMachine::new(); // never connected
        assert_eq!(m.effective_status(), PresenceStatus::Offline);

        let mut m2 = connected_machine();
        m2.on_disconnected();
        assert_eq!(m2.effective_status(), PresenceStatus::Offline);
    }

    // 9. set_user_status resets auto_state
    #[test]
    fn set_user_status_resets_auto_state() {
        let mut m = connected_machine();
        m.on_idle_detected();
        assert_eq!(m.auto_state, AutoState::AutoAway);

        m.set_user_status(PresenceStatus::Available);
        assert_eq!(m.auto_state, AutoState::Active);
        assert_eq!(m.effective_status(), PresenceStatus::Available);
    }

    // 10. build_presence_stanza — Away has a <show>away</show> child
    #[test]
    fn build_presence_stanza_away_has_show_element() {
        let mut m = connected_machine();
        m.on_idle_detected(); // → Away

        let stanza = m.build_presence_stanza().expect("should produce a stanza");
        assert_eq!(stanza.name(), "presence");
        assert_eq!(stanza.ns(), "jabber:client");

        let show = stanza
            .get_child("show", "jabber:client")
            .expect("<show> child must be present");
        assert_eq!(show.text(), "away");
    }

    // Bonus: Available stanza is empty (no <show>)
    #[test]
    fn build_presence_stanza_available_has_no_show() {
        let m = connected_machine();
        let stanza = m.build_presence_stanza().expect("should produce a stanza");
        assert!(
            stanza.get_child("show", "jabber:client").is_none(),
            "Available presence must not have a <show> child"
        );
    }

    // Bonus: Offline returns None
    #[test]
    fn build_presence_stanza_returns_none_when_offline() {
        let m = PresenceMachine::new();
        assert!(m.build_presence_stanza().is_none());
    }

    // Bonus: auto_state is preserved across reconnect
    #[test]
    fn auto_state_preserved_across_reconnect() {
        let mut m = connected_machine();
        m.on_idle_detected(); // → AutoAway
        m.on_disconnected();
        assert_eq!(m.effective_status(), PresenceStatus::Offline);

        m.on_connected(); // reconnect — should NOT reset auto_state
        assert_eq!(m.auto_state, AutoState::AutoAway);
        assert_eq!(m.effective_status(), PresenceStatus::Away);
    }

    // Bonus: xa stanza has <show>xa</show>
    #[test]
    fn build_presence_stanza_xa_has_show_xa() {
        let mut m = connected_machine();
        m.on_sleep_detected();

        let stanza = m.build_presence_stanza().expect("stanza");
        let show = stanza
            .get_child("show", "jabber:client")
            .expect("<show> child");
        assert_eq!(show.text(), "xa");
    }

    // Bonus: dnd stanza has <show>dnd</show>
    #[test]
    fn build_presence_stanza_dnd_has_show_dnd() {
        let mut m = connected_machine();
        m.set_user_status(PresenceStatus::DoNotDisturb);

        let stanza = m.build_presence_stanza().expect("stanza");
        let show = stanza
            .get_child("show", "jabber:client")
            .expect("<show> child");
        assert_eq!(show.text(), "dnd");
    }
}
