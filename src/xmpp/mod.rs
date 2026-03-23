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
    CatchupFinished {
        conversation_jid: String,
        fetched: usize,
    },

    // G2: typing indicator (XEP-0085)
    PeerTyping {
        jid: String,
        composing: bool,
    },

    // E4: Upload slot received (XEP-0363)
    UploadSlotReceived {
        put_url: String,
        get_url: String,
        headers: Vec<(String, String)>,
    },

    // H1: Avatar received from vCard (XEP-0153)
    AvatarReceived {
        jid: String,
        png_bytes: Vec<u8>,
    },

    // F1: raw XML stanza for the debug console panel
    ConsoleEntry {
        direction: String,
        xml: String,
    },
    // E3: Emoji reaction received (XEP-0444)
    ReactionReceived {
        msg_id: String,
        from: String,
        emojis: Vec<String>,
    },
}

/// Commands sent from the UI to the XMPP engine.
#[derive(Debug)]
pub enum XmppCommand {
    /// Start (or restart) a connection with the given credentials.
    Connect(ConnectConfig),
    /// C2: Update the user's own presence status.
    SetPresence(modules::presence_machine::PresenceStatus),
    /// Send a chat message to a JID.
    SendMessage { to: String, body: String },
    /// G2: Send a chat state notification (XEP-0085).
    SendChatState { to: String, composing: bool },
    /// H3: Add a contact to the roster.
    AddContact(String),
    /// Gracefully close the current session.
    #[allow(dead_code)]
    Disconnect,
    /// E4: Request an HTTP upload slot (XEP-0363).
    RequestUploadSlot { filename: String, size: u64, mime: String },
    /// H1: Fetch avatar for a JID (vCard-temp fallback).
    FetchAvatar(String),
    /// Block one or more JIDs (XEP-0191).
    BlockJid(String),
    /// Unblock a previously blocked JID (XEP-0191).
    UnblockJid(String),
    /// E3: Send an emoji reaction (XEP-0444).
    SendReaction { to: String, msg_id: String, emojis: Vec<String> },
}
