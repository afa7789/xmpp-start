// Task P1.6 — XEP-0198 Stream Management stanza tracker
// XEP reference: https://xmpp.org/extensions/xep-0198.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - record sent/received stanzas
//   - build <a> and <r> nonzas
//   - get the pending retransmit queue on reconnect

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio_xmpp::minidom::Element;

const NS_SM: &str = "urn:xmpp:sm:3";

/// XEP-0198 stanza acknowledgement tracker.
///
/// Tracks outbound and inbound stanza counts and maintains an unacknowledged
/// queue for retransmission on reconnect.
pub struct StreamMgmt {
    /// Number of stanzas sent to the server (outbound counter).
    stanzas_sent: u32,
    /// Last h value acknowledged by the server.
    stanzas_acked: u32,
    /// Last h value reported by the server in an <a> or <r>.
    server_h: u32,
    /// Number of stanzas received from the server (our inbound counter, `h`).
    h: u32,
    /// Stanzas sent but not yet acknowledged — used for retransmission.
    unacked: VecDeque<Element>,
    /// Time of the last `<a>` sent (for coalescing).
    last_ack_sent_at: Option<Instant>,
    /// True when an ack is due but not yet sent (coalescing pending).
    pending_ack: bool,
}

impl StreamMgmt {
    /// Creates a new tracker with all counters at zero.
    pub fn new() -> Self {
        Self {
            stanzas_sent: 0,
            stanzas_acked: 0,
            server_h: 0,
            h: 0,
            unacked: VecDeque::new(),
            last_ack_sent_at: None,
            pending_ack: false,
        }
    }

    /// Records that a stanza was sent to the server.
    ///
    /// Increments `stanzas_sent` and pushes `el` onto the unacked queue.
    pub fn on_stanza_sent(&mut self, el: Element) {
        self.stanzas_sent += 1;
        self.unacked.push_back(el);
    }

    /// Records that a stanza was received from the server.
    ///
    /// Increments the inbound counter `h` and marks an ack as pending for
    /// coalesced delivery.
    pub fn on_stanza_received(&mut self) {
        self.h += 1;
        self.pending_ack = true;
    }

    /// Processes an `<a h='...'/>` from the server.
    ///
    /// Drains all unacked stanzas up to (and including) position `h` and
    /// updates `stanzas_acked`.
    pub fn on_ack_received(&mut self, h: u32) {
        let to_drain = h.saturating_sub(self.stanzas_acked) as usize;
        let drain_count = to_drain.min(self.unacked.len());
        self.unacked.drain(..drain_count);
        self.stanzas_acked = h;
        self.server_h = h;
    }

    /// Builds an `<a xmlns='urn:xmpp:sm:3' h='{self.h}'/>` nonza.
    pub fn build_ack(&self) -> Element {
        Element::builder("a", NS_SM)
            .attr("h", self.h.to_string())
            .build()
    }

    /// Builds a `<r xmlns='urn:xmpp:sm:3'/>` nonza (request ack from server).
    #[allow(dead_code)] // used in stream resumption (future work)
    pub fn build_request(&self) -> Element {
        Element::builder("r", NS_SM).build()
    }

    /// Returns `Some(ack_element)` when a coalesced `<a>` should be sent.
    ///
    /// An ack is emitted only when `pending_ack` is true **and** at least 250ms
    /// have elapsed since the last ack was sent (or no ack has been sent yet).
    /// Callers should invoke this after every inbound stanza and send the
    /// returned element when `Some`.
    pub fn maybe_send_ack(&mut self) -> Option<Element> {
        if !self.pending_ack {
            return None;
        }
        let window_elapsed = self
            .last_ack_sent_at
            .map_or(true, |t| t.elapsed() >= Duration::from_millis(250));
        if window_elapsed {
            let ack = self.build_ack();
            self.last_ack_sent_at = Some(Instant::now());
            self.pending_ack = false;
            Some(ack)
        } else {
            None
        }
    }

    /// Force-flushes a pending ack immediately, bypassing the 250ms timer.
    ///
    /// Returns `Some(ack_element)` if an ack was pending, otherwise `None`.
    /// Useful before sending a stanza to ensure the server has our latest `h`.
    pub fn flush_ack(&mut self) -> Option<Element> {
        if self.pending_ack {
            let ack = self.build_ack();
            self.last_ack_sent_at = Some(Instant::now());
            self.pending_ack = false;
            Some(ack)
        } else {
            None
        }
    }

    /// Returns `true` if the unacked queue has drifted more than 50 stanzas
    /// ahead of the server's last acknowledged position.
    ///
    /// When this returns `true` the engine should log a warning and consider
    /// reconnecting to resync state.
    pub fn has_queue_desync(&self) -> bool {
        self.stanzas_sent.saturating_sub(self.stanzas_acked) > 50
    }

    /// Returns the number of stanzas in the unacked queue.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.unacked.len()
    }

    /// Returns the current inbound stanza counter (`h`).
    #[allow(dead_code)]
    pub fn h(&self) -> u32 {
        self.h
    }

    /// Resets all counters and clears the unacked queue for a new session.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.stanzas_sent = 0;
        self.stanzas_acked = 0;
        self.server_h = 0;
        self.h = 0;
        self.unacked.clear();
        self.last_ack_sent_at = None;
        self.pending_ack = false;
    }

    /// Returns a reference to the unacked stanza queue for retransmission.
    pub fn unacked_stanzas(&self) -> &VecDeque<Element> {
        &self.unacked
    }
}

impl Default for StreamMgmt {
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

    fn make_element(name: &str) -> Element {
        Element::builder(name, "jabber:client").build()
    }

    #[test]
    fn new_starts_at_zero() {
        let sm = StreamMgmt::new();
        assert_eq!(sm.stanzas_sent, 0);
        assert_eq!(sm.stanzas_acked, 0);
        assert_eq!(sm.server_h, 0);
        assert_eq!(sm.h(), 0);
        assert_eq!(sm.pending_count(), 0);
    }

    #[test]
    fn on_stanza_sent_increments_count_and_pushes_unacked() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_sent(make_element("message"));
        assert_eq!(sm.stanzas_sent, 1);
        assert_eq!(sm.pending_count(), 1);

        sm.on_stanza_sent(make_element("message"));
        assert_eq!(sm.stanzas_sent, 2);
        assert_eq!(sm.pending_count(), 2);
    }

    #[test]
    fn on_stanza_received_increments_h() {
        let mut sm = StreamMgmt::new();
        assert_eq!(sm.h(), 0);
        sm.on_stanza_received();
        assert_eq!(sm.h(), 1);
        sm.on_stanza_received();
        sm.on_stanza_received();
        assert_eq!(sm.h(), 3);
    }

    #[test]
    fn on_ack_received_drains_unacked_correctly() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_sent(make_element("m1"));
        sm.on_stanza_sent(make_element("m2"));
        sm.on_stanza_sent(make_element("m3"));
        assert_eq!(sm.pending_count(), 3);

        // Server acks the first two.
        sm.on_ack_received(2);
        assert_eq!(sm.pending_count(), 1);
        assert_eq!(sm.stanzas_acked, 2);
    }

    #[test]
    fn on_ack_received_drains_all_when_fully_acked() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_sent(make_element("m1"));
        sm.on_stanza_sent(make_element("m2"));

        sm.on_ack_received(2);
        assert_eq!(sm.pending_count(), 0);
        assert_eq!(sm.stanzas_acked, 2);
    }

    #[test]
    fn build_ack_returns_element_with_correct_h_attribute() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_received();
        sm.on_stanza_received();
        sm.on_stanza_received();

        let ack = sm.build_ack();
        assert_eq!(ack.name(), "a");
        assert_eq!(ack.ns(), NS_SM);
        assert_eq!(ack.attr("h"), Some("3"));
    }

    #[test]
    fn build_request_returns_r_with_correct_namespace() {
        let sm = StreamMgmt::new();
        let r = sm.build_request();
        assert_eq!(r.name(), "r");
        assert_eq!(r.ns(), NS_SM);
    }

    #[test]
    fn pending_count_is_accurate() {
        let mut sm = StreamMgmt::new();
        assert_eq!(sm.pending_count(), 0);
        sm.on_stanza_sent(make_element("msg"));
        assert_eq!(sm.pending_count(), 1);
        sm.on_ack_received(1);
        assert_eq!(sm.pending_count(), 0);
    }

    #[test]
    fn reset_clears_all_state() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_sent(make_element("msg"));
        sm.on_stanza_received();
        sm.on_stanza_received();
        sm.on_ack_received(1);

        sm.reset();

        assert_eq!(sm.stanzas_sent, 0);
        assert_eq!(sm.stanzas_acked, 0);
        assert_eq!(sm.server_h, 0);
        assert_eq!(sm.h(), 0);
        assert_eq!(sm.pending_count(), 0);
    }

    #[test]
    fn unacked_stanzas_returns_remaining_queue() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_sent(make_element("m1"));
        sm.on_stanza_sent(make_element("m2"));
        sm.on_stanza_sent(make_element("m3"));
        sm.on_ack_received(1);

        let queue = sm.unacked_stanzas();
        assert_eq!(queue.len(), 2);
    }

    // ------------------------------------------------------------------
    // P1.6b — ack coalescing + queue desync tests
    // ------------------------------------------------------------------

    #[test]
    fn maybe_send_ack_returns_none_when_no_pending_ack() {
        let mut sm = StreamMgmt::new();
        // No stanzas received, so pending_ack is false.
        assert!(sm.maybe_send_ack().is_none());
    }

    #[test]
    fn maybe_send_ack_returns_ack_after_250ms() {
        use std::thread::sleep;

        let mut sm = StreamMgmt::new();
        sm.on_stanza_received();

        // Within the debounce window the first call may or may not fire
        // depending on scheduling, but after sleeping >250ms it must fire.
        // Force the window by setting last_ack_sent_at to a past instant.
        sm.last_ack_sent_at = Some(Instant::now() - Duration::from_millis(300));

        let ack = sm.maybe_send_ack();
        assert!(ack.is_some());
        let ack = ack.unwrap();
        assert_eq!(ack.name(), "a");
        assert_eq!(ack.ns(), NS_SM);
        assert_eq!(ack.attr("h"), Some("1"));
        // pending_ack should now be cleared.
        assert!(!sm.pending_ack);

        // A second call without any new inbound stanza returns None.
        sleep(Duration::from_millis(260));
        assert!(sm.maybe_send_ack().is_none());
    }

    #[test]
    fn maybe_send_ack_respects_debounce_window() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_received();

        // Record a very recent last_ack_sent_at so the window has not elapsed.
        sm.last_ack_sent_at = Some(Instant::now());

        // Should be None because the 250ms window has not elapsed.
        assert!(sm.maybe_send_ack().is_none());
        // pending_ack must still be true — ack was not consumed.
        assert!(sm.pending_ack);
    }

    #[test]
    fn flush_ack_returns_ack_immediately() {
        let mut sm = StreamMgmt::new();
        sm.on_stanza_received();
        sm.on_stanza_received();

        // Set last_ack_sent_at to now so maybe_send_ack would be blocked.
        sm.last_ack_sent_at = Some(Instant::now());

        let ack = sm.flush_ack();
        assert!(ack.is_some());
        let ack = ack.unwrap();
        assert_eq!(ack.name(), "a");
        assert_eq!(ack.attr("h"), Some("2"));
        assert!(!sm.pending_ack);
    }

    #[test]
    fn flush_ack_returns_none_when_no_pending() {
        let mut sm = StreamMgmt::new();
        // No stanzas received.
        assert!(sm.flush_ack().is_none());
    }

    #[test]
    fn has_queue_desync_triggers_above_50() {
        let mut sm = StreamMgmt::new();
        // Send 51 stanzas, ack none.
        for _ in 0..51 {
            sm.on_stanza_sent(make_element("msg"));
        }
        assert!(sm.has_queue_desync());
    }

    #[test]
    fn has_queue_desync_false_within_limit() {
        let mut sm = StreamMgmt::new();
        // Send exactly 50 stanzas, ack none — should not trigger.
        for _ in 0..50 {
            sm.on_stanza_sent(make_element("msg"));
        }
        assert!(!sm.has_queue_desync());

        // Ack all — still no desync.
        sm.on_ack_received(50);
        assert!(!sm.has_queue_desync());
    }
}
