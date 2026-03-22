// XMPP engine — pure async, no UI dependencies.
// All modules communicate with the iced UI via channels (iced::subscription).
//
// Task P1: Core XMPP Engine
// Reference: packages/fluux-sdk/src/core/modules/ (TypeScript source)

pub mod connection;
pub mod modules;

/// Events emitted by the XMPP engine to the UI layer.
/// Sent through the iced subscription channel.
#[derive(Debug, Clone)]
pub enum XmppEvent {
    // Connection
    Connected,
    Disconnected { reason: String },
    Reconnecting { attempt: u32 },

    // Roster — Task P1.4
    // RosterReceived(Vec<RosterContact>),
    // PresenceUpdated { jid: String, show: PresenceShow },

    // Messages — Task P1.5
    // MessageReceived(IncomingMessage),
    // MessageSent { id: String },
    // TypingStarted { from: String },
    // TypingStopped { from: String },
}
