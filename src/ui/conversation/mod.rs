// Task P2.3 — ChatView: message list (virtual scroll)
// Task P2.4 — MessageComposer: text input + send button
// Source reference: apps/fluux/src/components/ChatView.tsx
//                   apps/fluux/src/components/MessageComposer.tsx
// Scroll strategy: docs/SCROLL_STRATEGY.md

mod update;
mod view;

/// Encryption mode for a conversation (task-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionMode {
    #[default]
    Disabled,
    Omemo,
    OpenPgp,
    Pgp,
}

impl EncryptionMode {
    /// Returns true when any encryption is active.
    pub fn is_active(self) -> bool {
        self != Self::Disabled
    }

    /// Human-readable label for display.
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "None (plaintext)",
            Self::Omemo => "OMEMO",
            Self::OpenPgp => "OpenPGP",
            Self::Pgp => "PGP (legacy)",
        }
    }

    /// All modes in display order.
    pub const ALL: [EncryptionMode; 4] = [
        EncryptionMode::Disabled,
        EncryptionMode::Omemo,
        EncryptionMode::OpenPgp,
        EncryptionMode::Pgp,
    ];
}

use crate::xmpp::modules::link_preview::LinkPreview;
use iced::widget::image as iced_image;
use iced::widget::scrollable::{AbsoluteOffset, Id};

// G4: /me action message prefix (XEP-0245)
pub(crate) const ME_PREFIX: &str = "/me ";

// M4: maximum voice recording duration (5 minutes = 300 seconds)
pub(crate) const VOICE_MAX_SECS: u32 = 300;

// M4: Voice recording state machine
pub enum VoiceState {
    Idle,
    Recording(RecordingHandle),
    Encoding,
    Uploading,
}

impl std::fmt::Debug for VoiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "VoiceState::Idle"),
            Self::Recording(_) => write!(f, "VoiceState::Recording(...)"),
            Self::Encoding => write!(f, "VoiceState::Encoding"),
            Self::Uploading => write!(f, "VoiceState::Uploading"),
        }
    }
}

// Cloning VoiceState resets any in-progress recording to Idle.
// ConversationView derives Clone, so we provide a safe fallback.
impl Clone for VoiceState {
    fn clone(&self) -> Self {
        VoiceState::Idle
    }
}

// M4: Handle kept alive while recording is in progress.
// The cpal::Stream is !Send on some platforms, so it lives on a
// dedicated std::thread; this struct holds the control primitives.
pub struct RecordingHandle {
    pub rx: std::sync::mpsc::Receiver<Vec<i16>>,
    pub stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Keep the stream alive via a dedicated thread that owns it.
    // Dropping the JoinHandle is fine — the thread exits when the stop_flag is set.
    pub _thread: Option<std::thread::JoinHandle<()>>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl std::fmt::Debug for RecordingHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingHandle")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

/// I3/E4: A file staged for upload.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub path: std::path::PathBuf,
    pub name: String,
    pub size: u64,
    /// Upload progress 0–100.
    pub progress: u8,
    /// DC-17: thumbnail preview for image attachments (PNG bytes from thumbnail::generate).
    pub thumbnail: Option<Vec<u8>>,
}

pub(crate) fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            return Some(word.to_string());
        }
    }
    None
}

/// I4: Returns Some(url) if the body is a bare image URL (jpg/png/gif/webp).
pub(crate) fn extract_image_url(body: &str) -> Option<String> {
    let trimmed = body.trim();
    // Only treat the body as an image if it's a single URL (no surrounding text)
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() == 1 {
        let w = words[0].to_lowercase();
        if (w.starts_with("http://") || w.starts_with("https://"))
            && (w.ends_with(".jpg")
                || w.ends_with(".jpeg")
                || w.ends_with(".png")
                || w.ends_with(".gif")
                || w.ends_with(".webp"))
        {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// DC-17: generate a thumbnail for a local image file.
/// Returns `None` for non-image files or if generation fails.
pub(crate) fn thumbnail_for_path(path: &std::path::Path) -> Option<Vec<u8>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp") {
        return None;
    }
    crate::store::thumbnail::generate_from_path(path)
        .ok()
        .map(|t| t.data)
}

// M3: emoji picker data — common emoji grouped by category
pub(crate) const EMOJI_LIST: &[(&str, &[&str])] = &[
    (
        "Faces",
        &["😀", "😂", "😍", "😎", "🤔", "😢", "😡", "🥳", "😴", "🤯"],
    ),
    (
        "Hands",
        &["👍", "👎", "👋", "🤝", "👏", "🙏", "✌️", "🤞", "👌", "🤙"],
    ),
    (
        "Hearts",
        &["❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "💔", "❣️"],
    ),
    (
        "Objects",
        &["🎉", "🔥", "⭐", "💡", "🎵", "📱", "💻", "🌈", "🍕", "☕"],
    ),
];

pub(crate) fn is_me_action(body: &str) -> bool {
    body.len() >= ME_PREFIX.len() && body[..ME_PREFIX.len()].eq_ignore_ascii_case(ME_PREFIX)
}

/// A single message shown in the conversation view.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub id: String,
    pub from: String,
    pub body: String,
    pub own: bool,      // true if sent by this account
    pub timestamp: i64, // unix milliseconds (G5)
    /// G3: quoted message preview (id, preview text)
    pub reply_preview: Option<String>,
    /// E1: true if the message body has been corrected
    pub edited: bool,
    /// E2: true if the message has been retracted
    pub retracted: bool,
    /// OMEMO: true if the message was decrypted via OMEMO (XEP-0384)
    pub is_encrypted: bool,
}

/// M2/K5: message delivery/read state for own messages (shown as ✓ indicators)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MessageState {
    Sending,
    Sent,
    Delivered,
    Read,
}

#[derive(Debug, Clone)]
pub struct ConversationView {
    pub peer_jid: String,
    pub(crate) messages: Vec<DisplayMessage>,
    pub(crate) composer: String,
    pub(crate) scroll_id: Id,
    pub(crate) scroll_offset: AbsoluteOffset,
    pub(crate) own_jid: String,
    /// C4: whether the peer is currently blocked (shown in header)
    pub peer_blocked: bool,
    /// G3: current reply-to (msg_id, preview text)
    pub(crate) reply_to: Option<(String, String)>,
    /// J3: whether notifications are muted for this conversation
    pub is_muted: bool,
    /// L1: number of messages seen when conversation was last opened
    pub(crate) last_seen_count: usize,
    /// G9: search state
    pub(crate) search_open: bool,
    pub(crate) search_query: String,
    /// M3: emoji picker state
    pub(crate) emoji_picker_open: bool,
    /// E3: emoji reactions — msg_id → (jid → emojis)
    pub reactions:
        std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>,
    /// E5: link previews — msg_id → preview
    pub(crate) previews: std::collections::HashMap<String, LinkPreview>,
    /// E5: pending URL previews to fetch — msg_id → url
    pub(crate) pending_previews: std::collections::HashMap<String, String>,
    /// E1: currently editing — (msg_id, original_body)
    pub(crate) edit_mode: Option<(String, String)>,
    /// I4: loaded image attachment handles — msg_id → image handle
    pub(crate) attachments: std::collections::HashMap<String, iced_image::Handle>,
    /// I4: pending image URLs to fetch — msg_id → url
    pub(crate) pending_images: std::collections::HashMap<String, String>,
    /// I3/E4: files staged for upload
    pub pending_attachments: Vec<Attachment>,
    /// I2: drag-drop staging (path string shown to user)
    pub(crate) drag_drop_active: bool,
    /// M2: delivery/read state for own messages — msg_id → state
    pub(crate) message_states: std::collections::HashMap<String, MessageState>,
    /// M6: currently hovered message ID for showing action bar
    pub(crate) hovered_message: Option<String>,
    /// L2: @mention autocomplete — Some(prefix) when active, None when inactive
    pub(crate) mention_prefix: Option<String>,
    /// M4: voice recording state
    pub(crate) voice_state: VoiceState,
    /// M4: seconds elapsed since recording started (updated by VoiceTick)
    pub(crate) voice_elapsed_secs: u32,
    /// L3: Message ID currently being moderated
    pub pending_moderate_dialog: Option<String>,
    /// L3: Reason text input for message moderation
    pub moderate_reason_input: String,
    /// Task-15: per-conversation encryption mode
    pub encryption_mode: EncryptionMode,
    /// Task-15: whether the encryption popover is open
    pub encryption_popover_open: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code, clippy::enum_variant_names)]
pub enum Message {
    ComposerChanged(String),
    Send,
    Scrolled(AbsoluteOffset),
    ScrollToBottom,
    CopyToClipboard(String),         // G7: copy message body to clipboard
    Close,                           // G1: close this conversation
    BlockPeer,                       // C4: block the peer JID
    UnblockPeer,                     // C4: unblock the peer JID
    ComposingStarted,                // G2: user started typing
    ComposingPaused,                 // G2: user stopped typing
    ReplyTo(String, String),         // G3: (msg_id, preview)
    CancelReply,                     // G3: cancel current reply
    ToggleMute,                      // J3: toggle notification mute
    SearchToggled,                   // G9: toggle search bar
    SearchQueryChanged(String),      // G9: search input changed
    EmojiPickerToggled,              // M3: toggle emoji picker
    EmojiSelected(String),           // M3: insert emoji into composer
    SendReaction(String, String),    // E3: (msg_id, emoji)
    ToggleReaction(String, String),  // R1: (msg_id, emoji) — toggle own reaction
    RetractReaction(String, String), // R1: (msg_id, emoji) — retract own reaction
    LinkPreviewReady(String, LinkPreview), // E5: (msg_id, preview)
    StartEdit(String, String),       // E1: (msg_id, current_body) — populate composer for edit
    CancelEdit,                      // E1: cancel edit mode
    RetractMessage(String),          // E2: (msg_id) — retract own message
    ModerateMessage(String, Option<String>), // L3: (msg_id, reason) — moderator retract any message
    AttachmentLoaded(String, iced_image::Handle), // I4: (msg_id, image_handle)
    // E4/I3: file upload
    OpenFilePicker,                         // E4: open native file picker
    FilePicked(Option<std::path::PathBuf>), // E4: result from file picker
    RemoveAttachment(usize),                // I3: remove staged attachment by index
    AttachmentProgress(usize, u8),          // I3: (index, progress 0–100)
    // I1: clipboard paste
    PasteFromClipboard,
    ClipboardImageReady(Vec<u8>), // I1: PNG bytes from clipboard
    // I2: drag-drop
    FilesDropped(Vec<std::path::PathBuf>), // I2: files dropped onto composer
    // M2: delivery/read status updates
    MessageDelivered(String), // (msg_id) — K4 receipt
    MessageRead(String),      // (msg_id) — K5 displayed marker
    // M6: hover state for action bar
    SetHoveredMessage(Option<String>), // Some(msg_id) or None to clear
    // L2: @mention autocomplete
    MentionSelected(String), // nick string (without @)
    MentionDismissed,        // dismiss autocomplete without selecting
    // M4: voice recording
    StartRecording,                             // mic button clicked — begin capture
    StopRecording,                              // stop button clicked — encode + upload
    CancelRecording,                            // cancel button — drop buffer
    VoiceEncodingDone(std::path::PathBuf, u64), // (temp_path, byte_size) — ready to upload
    VoiceTick,                                  // fired every 100 ms to update elapsed timer
    // L3: message moderation dialog
    OpenModerateDialog(String),
    ModerateReasonChanged(String),
    SubmitModerate,
    DismissModerateDialog,
    /// OMEMO: open the trust/fingerprint dialog for the sender of a message
    OpenOmemoTrust(String), // peer_jid
    /// Task-15: set encryption mode via popover selector
    SetEncryptionMode(EncryptionMode),
    /// Task-15: toggle the encryption popover visibility
    ToggleEncryptionPopover,
}

impl ConversationView {
    pub fn new(peer_jid: String, own_jid: String) -> Self {
        Self {
            peer_jid,
            messages: vec![],
            composer: String::new(),
            scroll_id: Id::new("conversation"),
            scroll_offset: AbsoluteOffset::default(),
            own_jid,
            peer_blocked: false,
            reply_to: None,
            is_muted: false,
            last_seen_count: 0,
            search_open: false,
            search_query: String::new(),
            emoji_picker_open: false,
            reactions: std::collections::HashMap::new(),
            previews: std::collections::HashMap::new(),
            pending_previews: std::collections::HashMap::new(),
            edit_mode: None,
            attachments: std::collections::HashMap::new(),
            pending_images: std::collections::HashMap::new(),
            pending_attachments: vec![],
            drag_drop_active: false,
            message_states: std::collections::HashMap::new(),
            hovered_message: None,
            mention_prefix: None,
            voice_state: VoiceState::Idle,
            voice_elapsed_secs: 0,
            pending_moderate_dialog: None,
            moderate_reason_input: String::new(),
            encryption_mode: EncryptionMode::default(),
            encryption_popover_open: false,
        }
    }

    pub fn push_message(&mut self, msg: DisplayMessage) {
        // I4: detect image URLs before moving msg
        if let Some(url) = extract_image_url(&msg.body) {
            self.pending_images.insert(msg.id.clone(), url);
        } else if let Some(url) = extract_first_url(&msg.body) {
            self.pending_previews.insert(msg.id.clone(), url);
        }
        self.messages.push(msg);
    }

    pub fn take_pending_previews(&mut self) -> std::collections::HashMap<String, String> {
        std::mem::take(&mut self.pending_previews)
    }

    /// I4: take pending image URLs for spawning fetch tasks.
    pub fn take_pending_images(&mut self) -> std::collections::HashMap<String, String> {
        std::mem::take(&mut self.pending_images)
    }

    /// B4: Replace all messages with history loaded from DB.
    pub fn load_history(&mut self, msgs: Vec<DisplayMessage>) {
        self.messages = msgs;
    }

    /// L1: record how many messages existed when the conversation was opened.
    pub fn mark_seen(&mut self) {
        self.last_seen_count = self.messages.len();
    }

    pub fn take_draft(&mut self) -> String {
        std::mem::take(&mut self.composer)
    }

    /// E1: returns the current edit mode (msg_id, original_body) if active.
    pub fn take_edit_mode(&mut self) -> Option<(String, String)> {
        self.edit_mode.take()
    }

    /// E1: apply an in-place correction to a message body (by msg_id).
    pub fn apply_correction(&mut self, msg_id: &str, new_body: &str) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == msg_id) {
            m.body = new_body.to_string();
            m.edited = true;
        }
    }

    /// E2: mark a message as retracted (tombstone display).
    pub fn apply_retraction(&mut self, msg_id: &str) {
        if let Some(m) = self.messages.iter_mut().find(|m| m.id == msg_id) {
            m.retracted = true;
        }
    }

    pub fn messages(&self) -> &[DisplayMessage] {
        &self.messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, from: &str, body: &str, own: bool) -> DisplayMessage {
        DisplayMessage {
            id: id.into(),
            from: from.into(),
            body: body.into(),
            own,
            timestamp: 0,
            reply_preview: None,
            edited: false,
            retracted: false,
            is_encrypted: false,
        }
    }

    #[test]
    fn conversation_view_empty() {
        let cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        assert!(cv.messages().is_empty());
    }

    #[test]
    fn push_message_increments_count() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.push_message(make_msg("1", "alice@example.com", "Hello", false));
        assert_eq!(cv.messages().len(), 1);
    }

    #[test]
    fn take_draft_clears_composer() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.composer = "hello world".into();
        let draft = cv.take_draft();
        assert_eq!(draft, "hello world");
        assert!(cv.composer.is_empty());
    }

    #[test]
    fn send_clears_composer() {
        let mut cv = ConversationView::new("alice@example.com".into(), "me@example.com".into());
        cv.composer = "test message".into();
        let _ = cv.update(Message::Send);
        assert!(cv.composer.is_empty());
    }

    #[test]
    fn me_action_detection_case_insensitive() {
        assert!(is_me_action("/me waves"));
        assert!(is_me_action("/ME waves"));
        assert!(is_me_action("/Me waves"));
        assert!(!is_me_action("hello"));
        assert!(!is_me_action("/me")); // no trailing space + content
        assert!(!is_me_action("/menu"));
    }
}
