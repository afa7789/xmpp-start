// Task P1.6 — XEP-0198 Stream Management stanza tracker
// XEP reference: https://xmpp.org/extensions/xep-0198.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - record sent/received stanzas
//   - build <a> and <r> nonzas
//   - get the pending retransmit queue on reconnect

use std::collections::VecDeque;
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
    /// Increments the inbound counter `h`.
    pub fn on_stanza_received(&mut self) {
        self.h += 1;
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
    pub fn build_request(&self) -> Element {
        Element::builder("r", NS_SM).build()
    }

    /// Returns the number of stanzas in the unacked queue.
    pub fn pending_count(&self) -> usize {
        self.unacked.len()
    }

    /// Returns the current inbound stanza counter (`h`).
    pub fn h(&self) -> u32 {
        self.h
    }

    /// Resets all counters and clears the unacked queue for a new session.
    pub fn reset(&mut self) {
        self.stanzas_sent = 0;
        self.stanzas_acked = 0;
        self.server_h = 0;
        self.h = 0;
        self.unacked.clear();
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
}
