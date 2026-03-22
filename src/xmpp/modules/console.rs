#![allow(dead_code)]
// Task P6.3 — XMPP Console stanza log
//
// Circular buffer of raw XML stanzas for debugging.
// Pure data structure — no I/O, no async.

use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Direction of a stanza (sent by us or received from the server).
#[derive(Debug, Clone, PartialEq)]
pub enum StanzaDirection {
    Sent,
    Received,
}

/// A single logged stanza entry.
#[derive(Debug, Clone)]
pub struct StanzaEntry {
    pub direction: StanzaDirection,
    pub xml: String,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ---------------------------------------------------------------------------
// XmppConsole
// ---------------------------------------------------------------------------

/// Circular buffer of raw XML stanza strings for the XMPP debug console.
///
/// When `len() == max_entries`, the oldest entry is evicted on the next push.
pub struct XmppConsole {
    entries: VecDeque<StanzaEntry>,
    max_entries: usize,
}

impl XmppConsole {
    /// Create a new console with the given capacity.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Log a stanza that was sent to the server.
    pub fn push_sent(&mut self, xml: &str, timestamp_ms: u64) {
        self.push(StanzaDirection::Sent, xml, timestamp_ms);
    }

    /// Log a stanza that was received from the server.
    pub fn push_received(&mut self, xml: &str, timestamp_ms: u64) {
        self.push(StanzaDirection::Received, xml, timestamp_ms);
    }

    /// Returns entries in insertion order (oldest first).
    pub fn entries(&self) -> impl Iterator<Item = &StanzaEntry> {
        self.entries.iter()
    }

    /// Total number of entries currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no entries are stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Return an iterator over entries whose XML contains `query` (case-insensitive).
    pub fn search<'a>(&'a self, query: &'a str) -> impl Iterator<Item = &'a StanzaEntry> {
        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(move |e| e.xml.to_lowercase().contains(&query_lower))
    }

    // --- private helpers ---------------------------------------------------

    fn push(&mut self, direction: StanzaDirection, xml: &str, timestamp_ms: u64) {
        if self.entries.len() == self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(StanzaEntry {
            direction,
            xml: xml.to_string(),
            timestamp_ms,
        });
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
    fn push_and_len() {
        let mut console = XmppConsole::new(10);
        assert!(console.is_empty());

        console.push_sent("<presence/>", 1000);
        console.push_received("<presence type='unavailable'/>", 2000);

        assert_eq!(console.len(), 2);
        assert!(!console.is_empty());
    }

    // 2 -----------------------------------------------------------------------
    #[test]
    fn circular_buffer_drops_oldest() {
        let mut console = XmppConsole::new(3);

        console.push_sent("<a/>", 1);
        console.push_sent("<b/>", 2);
        console.push_sent("<c/>", 3);
        // Buffer is full — next push should evict "<a/>"
        console.push_sent("<d/>", 4);

        assert_eq!(console.len(), 3);
        let xmls: Vec<&str> = console.entries().map(|e| e.xml.as_str()).collect();
        assert_eq!(xmls, vec!["<b/>", "<c/>", "<d/>"]);
    }

    // 3 -----------------------------------------------------------------------
    #[test]
    fn search_finds_matching() {
        let mut console = XmppConsole::new(10);

        console.push_sent("<message to='alice@example.org'/>", 1);
        console.push_received("<presence from='bob@example.org'/>", 2);
        console.push_sent("<iq id='ping-1'/>", 3);

        let results: Vec<&StanzaEntry> = console.search("message").collect();
        assert_eq!(results.len(), 1);
        assert!(results[0].xml.contains("message"));

        // Case-insensitive
        let results_upper: Vec<&StanzaEntry> = console.search("MESSAGE").collect();
        assert_eq!(results_upper.len(), 1);
    }

    // 4 -----------------------------------------------------------------------
    #[test]
    fn clear_empties_buffer() {
        let mut console = XmppConsole::new(10);

        console.push_sent("<a/>", 1);
        console.push_received("<b/>", 2);
        assert_eq!(console.len(), 2);

        console.clear();
        assert_eq!(console.len(), 0);
        assert!(console.is_empty());
    }

    // 5 -----------------------------------------------------------------------
    #[test]
    fn entries_order_oldest_first() {
        let mut console = XmppConsole::new(10);

        console.push_sent("<first/>", 100);
        console.push_received("<second/>", 200);
        console.push_sent("<third/>", 300);

        let entries: Vec<&StanzaEntry> = console.entries().collect();
        assert_eq!(entries[0].timestamp_ms, 100);
        assert_eq!(entries[1].timestamp_ms, 200);
        assert_eq!(entries[2].timestamp_ms, 300);
        assert_eq!(entries[0].direction, StanzaDirection::Sent);
        assert_eq!(entries[1].direction, StanzaDirection::Received);
    }
}
