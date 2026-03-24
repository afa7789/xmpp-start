// XMPP engine — pure async, no UI dependencies.
// All modules communicate with the iced UI via channels (iced::subscription).
//
// Task P1: Core XMPP Engine
// Reference: packages/fluux-sdk/src/core/modules/ (TypeScript source)

pub mod connection;
pub mod engine;
pub mod modules;
pub mod multi_engine;
pub mod subscription;

use tokio_xmpp::minidom::Element;

pub use connection::ConnectConfig;

// ---------------------------------------------------------------------------
// MULTI: Account identity newtype
// ---------------------------------------------------------------------------

/// Opaque identifier for a configured XMPP account.
/// Wraps the bare JID string so call-sites can't accidentally pass arbitrary strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(pub String);

impl AccountId {
    #[allow(dead_code)]
    pub fn new(jid: impl Into<String>) -> Self {
        Self(jid.into())
    }

    #[allow(dead_code)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------------------------------------------------------------------------
// MULTI: Account-scoped event wrapper
// ---------------------------------------------------------------------------

/// Wraps an [`XmppEvent`] with the [`AccountId`] that produced it.
///
/// When multiple engine instances are running concurrently the UI needs to
/// know which account an event came from so it can update the right account state.
#[derive(Debug, Clone)]
pub struct AccountEvent {
    /// The account this event originated from.
    pub account_id: AccountId,
    /// The event payload.
    pub event: XmppEvent,
}

impl AccountEvent {
    /// Convenience constructor.
    #[allow(dead_code)]
    pub fn new(account_id: AccountId, event: XmppEvent) -> Self {
        Self { account_id, event }
    }
}

impl std::fmt::Display for AccountEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {:?}", self.account_id, self.event)
    }
}

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
    /// True when this message was retrieved from MAM archive history.
    /// Notifications and sounds should be suppressed for historical messages.
    pub is_historical: bool,
}

/// Events emitted by the XMPP engine to the UI layer.
/// Sent through the iced subscription channel.
#[derive(Debug, Clone)]
#[allow(dead_code)]
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

    // H4: vCard received
    VCardReceived {
        jid: String,
        name: Option<String>,
        email: Option<String>,
    },

    // D4: bookmarks loaded from server (XEP-0048)
    BookmarksReceived(Vec<modules::bookmarks::Bookmark>),

    // J6: XEP-0084 PubSub avatar received (modern path)
    AvatarUpdated {
        jid: String,
        data: Vec<u8>,
    },

    // K4: XEP-0184 delivery receipt — recipient confirmed message received
    MessageDelivered {
        id: String,
        from: String,
    },

    // K5: XEP-0333 read marker — recipient has displayed the message
    MessageRead {
        id: String,
        from: String,
    },

    // J10: MAM archiving preferences received
    MamPrefsReceived {
        default_mode: String,
    },

    // K1: Server returned a config form for a newly created room.
    RoomConfigFormReceived {
        room_jid: String,
        /// Pre-parsed defaults from the server's form.
        config: modules::muc_config::MucRoomConfig,
    },
    // K1: Room configuration was accepted by the server — room is now live.
    RoomConfigured {
        room_jid: String,
    },
    // L3: A message in a MUC room was moderated (tombstoned by a moderator).
    MessageModerated {
        room_jid: String,
        message_id: String,
    },

    // K3: Incoming room invitation (XEP-0249 direct or XEP-0045 mediated)
    RoomInvitationReceived {
        room_jid: String,
        from_jid: String,
        reason: Option<String>,
    },
    // K2: Room list received from MUC service (disco#items result)
    RoomListReceived(Vec<modules::disco::DiscoItem>),

    // J9: XEP-0077 Account Registration
    RegistrationFormReceived {
        server: String,
        form: Element,
    },
    RegistrationSuccess,
    RegistrationFailure(String),

    // K2: Own vCard fetched successfully (XEP-0054).
    OwnVCardReceived(modules::vcard_edit::VCardFields),
    // K2: Own vCard published successfully.
    OwnVCardSaved,

    // L4: Ad-hoc command result received (XEP-0050).
    AdhocCommandResult(modules::adhoc::CommandResponse),
    // L4: Ad-hoc command discovery result — list of (node, name) pairs.
    AdhocCommandsDiscovered {
        from_jid: String,
        commands: Vec<(String, String)>,
    },

    // L3: Peer published a new location (XEP-0080).
    LocationReceived {
        from: String,
        location: modules::geoloc::GeoLocation,
    },

    // Q2: Bits of Binary data received in response to a request (XEP-0231).
    BobReceived(modules::bob::BobData),

    // MULTI: Account management events.
    /// The active account has been switched to the given account.
    AccountSwitched(AccountId),

    // MEMO: OMEMO E2E encryption events (XEP-0384)

    /// A peer's OMEMO device list was received or updated via PEP.
    OmemoDeviceListReceived {
        jid: String,
        devices: Vec<u32>,
    },

    /// An incoming OMEMO-encrypted message was successfully decrypted.
    OmemoMessageDecrypted {
        from: String,
        body: String,
    },

    /// A new unrecognized device appeared for `jid` and needs trust resolution.
    OmemoKeyExchangeNeeded {
        jid: String,
    },

    // L1: Sticker packs (XEP-0449)
    /// A sticker pack was received from PubSub.
    StickerPackReceived(modules::stickers::StickerPack),
}

/// Commands sent from the UI to the XMPP engine.
#[derive(Debug)]
#[allow(dead_code)]
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
    /// H3: Remove a contact from the roster.
    RemoveContact(String),
    /// H3: Rename a contact in the roster.
    RenameContact { jid: String, name: String },
    /// Gracefully close the current session.
    #[allow(dead_code)]
    Disconnect,
    /// E4: Request an HTTP upload slot (XEP-0363).
    RequestUploadSlot {
        filename: String,
        size: u64,
        mime: String,
    },
    /// H1: Fetch avatar for a JID (vCard-temp fallback).
    FetchAvatar(String),
    /// H2: Publish own avatar via PubSub (XEP-0084).
    SetAvatar {
        /// Raw image bytes.
        data: Vec<u8>,
        /// MIME type (e.g., "image/png").
        mime_type: String,
    },
    /// Block one or more JIDs (XEP-0191).
    BlockJid(String),
    /// Unblock a previously blocked JID (XEP-0191).
    UnblockJid(String),
    /// E3: Send an emoji reaction (XEP-0444).
    SendReaction {
        to: String,
        msg_id: String,
        emojis: Vec<String>,
    },
    /// E1: Send a message correction (XEP-0308).
    SendCorrection {
        to: String,
        original_id: String,
        new_body: String,
    },
    /// E2: Send a message retraction (XEP-0424).
    SendRetraction { to: String, origin_id: String },
    /// H4: Fetch vCard for a JID.
    FetchVCard(String),
    /// G8: Fetch older MAM history before a given message ID.
    FetchHistory {
        jid: String,
        before_id: Option<String>,
    },
    /// D3: Join a MUC room with the given nickname (XEP-0045).
    JoinRoom { jid: String, nick: String },
    /// D3: Leave a MUC room (XEP-0045).
    LeaveRoom(String),
    /// K5: Send an XEP-0333 displayed marker to indicate a message was read.
    SendDisplayed { to: String, id: String },
    /// J10: Set MAM archiving preferences.
    SetMamPrefs { default_mode: String },
    /// S1: User has been idle for ~5 minutes — trigger auto-away.
    UserIdle,
    /// S1: User has been idle for ~15 minutes — trigger extended away.
    UserExtendedIdle,
    /// S1: User is active again — restore pre-idle presence.
    UserActive,
    /// K3: Send a room invitation (XEP-0045 + XEP-0249).
    SendRoomInvitation {
        room: String,
        user: String,
        reason: Option<String>,
    },
    /// K2: Browse public rooms — send disco#items to MUC service.
    FetchRoomList,
    /// K7: Enable push notifications (XEP-0357).
    EnablePush {
        /// The push service JID (e.g., "push.example.com").
        service_jid: String,
    },
    /// K7: Disable push notifications (XEP-0357).
    DisablePush {
        /// The push service JID to disable.
        service_jid: String,
    },
    /// K7: Disable all push notifications (XEP-0357).
    DisableAllPush,
    /// K1: Create a new MUC room by joining it (XEP-0045 §8).
    /// The engine joins the room; if the server returns status 201 (room created),
    /// it requests the config form automatically.
    CreateRoom {
        /// Local part of the room JID (before the '@').
        local: String,
        /// Conference service domain (e.g. "conference.example.com").
        service: String,
        /// Nickname to use in the new room.
        nick: String,
    },
    /// K1: Submit a room configuration form (XEP-0045 §9).
    ConfigureRoom {
        room_jid: String,
        config: crate::xmpp::modules::muc_config::MucRoomConfig,
    },
    /// L3: Send a moderation action (XEP-0425) in a MUC room.
    ModerateMessage {
        room_jid: String,
        message_id: String,
        reason: Option<String>,
    },
    /// J9: Register a new account (XEP-0077).
    Register(ConnectConfig),
    /// J9: Submit a registration form.
    SubmitRegistration {
        server: String,
        form: Element,
    },
    /// K2: Fetch the logged-in user's own vCard (XEP-0054).
    FetchOwnVCard,
    /// K2: Publish updated vCard fields for the logged-in user (XEP-0054).
    SetOwnVCard(modules::vcard_edit::VCardFields),
    /// L4: Execute an ad-hoc command on `to_jid` (XEP-0050).
    ExecuteAdhocCommand {
        to_jid: String,
        node: String,
    },
    /// L4: Continue an in-progress ad-hoc command session with filled fields.
    ContinueAdhocCommand {
        to_jid: String,
        node: String,
        session_id: String,
        fields: Vec<modules::adhoc::DataField>,
    },
    /// L4: Cancel an in-progress ad-hoc command session.
    CancelAdhocCommand {
        to_jid: String,
        node: String,
        session_id: String,
    },
    /// L4: Discover ad-hoc commands available on a JID (disco#items with commands node).
    DiscoverAdhocCommands {
        target_jid: String,
    },

    /// L5: Report a JID as a spammer (XEP-0377).
    ReportSpam {
        jid: String,
        reason: Option<String>,
    },

    /// L3: Publish the user's current location via PEP (XEP-0080).
    PublishLocation(modules::geoloc::GeoLocation),

    /// Q2: Request a Bits of Binary data element from a peer (XEP-0231).
    RequestBob {
        cid: String,
        from: String,
    },

    // MULTI: Account management commands.
    /// Switch the active account to the given account ID.
    SwitchAccount(AccountId),
    /// Add a new account to the engine's account pool.
    AddAccount(crate::config::AccountConfig),
    /// Remove an account from the engine's account pool.
    RemoveAccount(AccountId),
    /// Establish the XMPP connection for an already-registered account.
    ConnectAccount(AccountId),
    /// Gracefully disconnect the XMPP connection for an account without
    /// removing it from the pool (it can be reconnected later).
    DisconnectAccount(AccountId),

    // MEMO: OMEMO E2E encryption commands (XEP-0384)

    /// Generate an identity key pair, publish the device list, and publish the pre-key bundle.
    /// This enables OMEMO for the current account.
    OmemoEnable,

    /// Encrypt `body` for all trusted devices of `to` and send the result.
    OmemoEncryptMessage { to: String, body: String },

    /// Mark `device_id` for `jid` as trusted by the user.
    OmemoTrustDevice { jid: String, device_id: u32 },

    // L1: Sticker packs (XEP-0449)
    /// Send a sticker to `to` from the given pack.
    SendSticker {
        to: String,
        pack_id: String,
        sticker: modules::stickers::Sticker,
    },
}
