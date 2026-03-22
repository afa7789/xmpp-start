// XMPP engine — pure async, no UI dependencies.
// All modules communicate with the iced UI via channels (iced::subscription).
//
// Task P1: Core XMPP Engine
// Reference: packages/fluux-sdk/src/core/modules/ (TypeScript source)

pub mod connection;
pub mod engine;
pub mod modules;
pub mod subscription;

pub use connection::ConnectConfig;

// ---------------------------------------------------------------------------
// Data types exchanged between engine and UI
// ---------------------------------------------------------------------------

/// A contact from the user's roster (RFC 6121).
#[derive(Debug, Clone)]
pub struct RosterContact {
    pub jid: String,
    pub name: Option<String>,
    #[allow(dead_code)]
    pub subscription: String,
}

/// A chat message received from the server.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub id: String,
    pub from: String,
    pub body: String,
}

/// Events emitted by the XMPP engine to the UI layer.
/// Sent through the iced subscription channel.
#[derive(Debug, Clone)]
pub enum XmppEvent {
    // Connection lifecycle
    Connected {
        bound_jid: String,
    },
    Disconnected {
        reason: String,
    },
    Reconnecting {
        attempt: u32,
    },

    // Roster — P1.4
    RosterReceived(Vec<RosterContact>),

    // Messages — P1.5
    MessageReceived(IncomingMessage),

    // Presence — P1.4b
    PresenceUpdated {
        jid: String,
        available: bool,
    },

    // MAM catchup — P4.3
    /// Emitted when the <fin> for a per-conversation MAM catchup query arrives.
    /// `fetched` is the number of archived messages received in this round.
    #[allow(dead_code)]
    CatchupFinished {
        conversation_jid: String,
        fetched: usize,
    },

    // G2: typing indicator (XEP-0085)
    PeerTyping {
        jid: String,
        composing: bool,
    },
}

/// Commands sent from the UI to the XMPP engine.
#[derive(Debug)]
pub enum XmppCommand {
    /// Start (or restart) a connection with the given credentials.
    Connect(ConnectConfig),
    /// Send a chat message to a JID.
    SendMessage { to: String, body: String },
    /// G2: Send a chat state notification (XEP-0085).
    SendChatState { to: String, composing: bool },
    /// Gracefully close the current session.
    #[allow(dead_code)]
    Disconnect,
}
