// Task P2.3 — ChatView: message list (virtual scroll)
// Task P2.4 — MessageComposer: text input + send button
// Source reference: apps/fluux/src/components/ChatView.tsx
//                   apps/fluux/src/components/MessageComposer.tsx
// Scroll strategy: docs/SCROLL_STRATEGY.md

use iced::widget::image as iced_image;
use iced::widget::scrollable::{AbsoluteOffset, Id};
use iced::widget::text::Shaping;
use iced::widget::text::Span as IcedSpan;
use iced::{
    font,
    widget::{
        button, column, container, image, mouse_area, rich_text, row, scrollable, span, text,
        text_input, tooltip,
    },
    Alignment, Color, Element, Font, Length, Task,
};

use crate::ui::muc_panel::OccupantEntry;

use chrono::{TimeZone, Utc};

use crate::xmpp::modules::link_preview::LinkPreview;
use crate::ui::link_preview::render_preview_card;

// G4: /me action message prefix (XEP-0245)
const ME_PREFIX: &str = "/me ";

// M4: maximum voice recording duration (5 minutes = 300 seconds)
const VOICE_MAX_SECS: u32 = 300;

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
}

fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            return Some(word.to_string());
        }
    }
    None
}

/// I4: Returns Some(url) if the body is a bare image URL (jpg/png/gif/webp).
fn extract_image_url(body: &str) -> Option<String> {
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

// M3: emoji picker data — common emoji grouped by category
const EMOJI_LIST: &[(&str, &[&str])] = &[
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

fn is_me_action(body: &str) -> bool {
    body.len() >= ME_PREFIX.len() && body[..ME_PREFIX.len()].eq_ignore_ascii_case(ME_PREFIX)
}

use crate::ui::avatar::{jid_color, jid_initial};
use crate::ui::styling::{self, SpanStyle};

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
    messages: Vec<DisplayMessage>,
    pub(crate) composer: String,
    scroll_id: Id,
    scroll_offset: AbsoluteOffset,
    own_jid: String,
    /// C4: whether the peer is currently blocked (shown in header)
    pub peer_blocked: bool,
    /// G3: current reply-to (msg_id, preview text)
    reply_to: Option<(String, String)>,
    /// J3: whether notifications are muted for this conversation
    pub is_muted: bool,
    /// L1: number of messages seen when conversation was last opened
    last_seen_count: usize,
    /// G9: search state
    search_open: bool,
    search_query: String,
    /// M3: emoji picker state
    emoji_picker_open: bool,
    /// E3: emoji reactions — msg_id → (jid → emojis)
    pub reactions:
        std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>,
    /// E5: link previews — msg_id → preview
    previews: std::collections::HashMap<String, LinkPreview>,
    /// E5: pending URL previews to fetch — msg_id → url
    pending_previews: std::collections::HashMap<String, String>,
    /// E1: currently editing — (msg_id, original_body)
    edit_mode: Option<(String, String)>,
    /// G8: true while waiting for older MAM history to arrive
    pub loading_older: bool,
    /// I4: loaded image attachment handles — msg_id → image handle
    attachments: std::collections::HashMap<String, iced_image::Handle>,
    /// I4: pending image URLs to fetch — msg_id → url
    pending_images: std::collections::HashMap<String, String>,
    /// I3/E4: files staged for upload
    pub pending_attachments: Vec<Attachment>,
    /// I2: drag-drop staging (path string shown to user)
    drag_drop_active: bool,
    /// M2: delivery/read state for own messages — msg_id → state
    message_states: std::collections::HashMap<String, MessageState>,
    /// M6: currently hovered message ID for showing action bar
    hovered_message: Option<String>,
    /// L2: @mention autocomplete — Some(prefix) when active, None when inactive
    mention_prefix: Option<String>,
    /// M4: voice recording state
    voice_state: VoiceState,
    /// M4: seconds elapsed since recording started (updated by VoiceTick)
    voice_elapsed_secs: u32,
    /// L3: Message ID currently being moderated
    pub pending_moderate_dialog: Option<String>,
    /// L3: Reason text input for message moderation
    pub moderate_reason_input: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code, clippy::enum_variant_names)]
pub enum Message {
    ComposerChanged(String),
    Send,
    Scrolled(AbsoluteOffset),
    ScrollToBottom,
    CopyToClipboard(String),      // G7: copy message body to clipboard
    Close,                        // G1: close this conversation
    BlockPeer,                    // C4: block the peer JID
    UnblockPeer,                  // C4: unblock the peer JID
    ComposingStarted,             // G2: user started typing
    ComposingPaused,              // G2: user stopped typing
    ReplyTo(String, String),      // G3: (msg_id, preview)
    CancelReply,                  // G3: cancel current reply
    ToggleMute,                   // J3: toggle notification mute
    SearchToggled,                // G9: toggle search bar
    SearchQueryChanged(String),   // G9: search input changed
    EmojiPickerToggled,           // M3: toggle emoji picker
    EmojiSelected(String),        // M3: insert emoji into composer
    SendReaction(String, String),    // E3: (msg_id, emoji)
    ToggleReaction(String, String),  // R1: (msg_id, emoji) — toggle own reaction
    RetractReaction(String, String), // R1: (msg_id, emoji) — retract own reaction
    LinkPreviewReady(String, LinkPreview), // E5: (msg_id, preview)
    StartEdit(String, String),    // E1: (msg_id, current_body) — populate composer for edit
    CancelEdit,                   // E1: cancel edit mode
    RetractMessage(String),       // E2: (msg_id) — retract own message
    ModerateMessage(String, Option<String>), // L3: (msg_id, reason) — moderator retract any message
    RequestOlderHistory,          // G8: emitted on scroll to top to request older MAM history
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
            loading_older: false,
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

    /// G8: Prepend older messages at the front of the message list.
    #[allow(dead_code)]
    pub fn prepend_messages(&mut self, mut older: Vec<DisplayMessage>) {
        older.append(&mut self.messages);
        self.messages = older;
        self.loading_older = false;
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

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::ComposerChanged(v) => {
                let was_empty = self.composer.is_empty();
                self.composer = v;
                // L2: detect last `@` with no space after it to activate autocomplete
                self.mention_prefix = {
                    if let Some(at_pos) = self.composer.rfind('@') {
                        let after_at = &self.composer[at_pos + 1..];
                        if !after_at.contains(' ') {
                            Some(after_at.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if !self.composer.is_empty() && was_empty {
                    return Task::done(Message::ComposingStarted);
                } else if self.composer.is_empty() && !was_empty {
                    return Task::done(Message::ComposingPaused);
                }
                Task::none()
            }
            Message::Send => {
                self.composer.clear();
                self.reply_to = None;
                self.edit_mode = None;
                self.mention_prefix = None;
                // M4: if we were in Uploading state, reset voice state after send
                if matches!(self.voice_state, VoiceState::Uploading) {
                    self.voice_state = VoiceState::Idle;
                    self.voice_elapsed_secs = 0;
                }
                Task::none()
            }
            Message::Scrolled(offset) => {
                self.scroll_offset = offset;
                // G8: if scrolled to (or near) the top, request older history
                if offset.y < 20.0 && !self.loading_older && !self.messages.is_empty() {
                    self.loading_older = true;
                    return Task::done(Message::RequestOlderHistory);
                }
                Task::none()
            }
            Message::ScrollToBottom => {
                let bottom = AbsoluteOffset {
                    x: 0.0,
                    y: f32::MAX,
                };
                scrollable::scroll_to::<Message>(self.scroll_id.clone(), bottom)
            }
            Message::CopyToClipboard(text) => iced::clipboard::write::<Message>(text),
            Message::Close => Task::none(), // handled by ChatScreen
            Message::BlockPeer => Task::none(), // handled by ChatScreen → engine
            Message::UnblockPeer => Task::none(), // handled by ChatScreen → engine
            Message::ComposingStarted => Task::none(), // bubbled to ChatScreen
            Message::ComposingPaused => Task::none(), // bubbled to ChatScreen
            Message::ReplyTo(id, preview) => {
                self.reply_to = Some((id, preview));
                Task::none()
            }
            Message::CancelReply => {
                self.reply_to = None;
                Task::none()
            }
            Message::ToggleMute => Task::none(), // handled by ChatScreen → App
            Message::SearchToggled => {
                self.search_open = !self.search_open;
                if !self.search_open {
                    self.search_query.clear();
                }
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q;
                Task::none()
            }
            Message::EmojiPickerToggled => {
                self.emoji_picker_open = !self.emoji_picker_open;
                Task::none()
            }
            Message::EmojiSelected(emoji) => {
                self.composer.push_str(&emoji);
                self.emoji_picker_open = false;
                Task::none()
            }
            Message::SendReaction(_, _) => Task::none(), // bubbled to ChatScreen
            // R1: toggle reaction — retract if already sent, send otherwise
            Message::ToggleReaction(msg_id, emoji) => {
                let already = self
                    .reactions
                    .get(&msg_id)
                    .and_then(|by_jid| by_jid.get(&self.own_jid))
                    .is_some_and(|emojis| emojis.contains(&emoji));
                if already {
                    Task::done(Message::RetractReaction(msg_id, emoji))
                } else {
                    Task::done(Message::SendReaction(msg_id, emoji))
                }
            }
            Message::RetractReaction(_, _) => Task::none(), // R1: bubbled to ChatScreen
            Message::LinkPreviewReady(msg_id, preview) => {
                self.previews.insert(msg_id, preview);
                Task::none()
            }
            Message::StartEdit(id, body) => {
                self.composer = body.clone();
                self.edit_mode = Some((id, body));
                self.reply_to = None;
                Task::none()
            }
            Message::CancelEdit => {
                self.composer.clear();
                self.edit_mode = None;
                Task::none()
            }
            Message::RetractMessage(_) => Task::none(), // bubbled to ChatScreen
            Message::ModerateMessage(_, _) => Task::none(), // L3: bubbled to ChatScreen
            Message::OpenModerateDialog(msg_id) => {
                self.pending_moderate_dialog = Some(msg_id);
                self.moderate_reason_input.clear();
                Task::none()
            }
            Message::ModerateReasonChanged(reason) => {
                self.moderate_reason_input = reason;
                Task::none()
            }
            Message::SubmitModerate => {
                if let Some(msg_id) = self.pending_moderate_dialog.take() {
                    let reason = if self.moderate_reason_input.trim().is_empty() {
                        None
                    } else {
                        Some(self.moderate_reason_input.trim().to_string())
                    };
                    self.moderate_reason_input.clear();
                    return Task::done(Message::ModerateMessage(msg_id, reason));
                }
                Task::none()
            }
            Message::DismissModerateDialog => {
                self.pending_moderate_dialog = None;
                self.moderate_reason_input.clear();
                Task::none()
            }
            Message::RequestOlderHistory => Task::none(), // bubbled to ChatScreen
            Message::AttachmentLoaded(msg_id, handle) => {
                self.attachments.insert(msg_id, handle);
                Task::none()
            }
            // E4/I3: open native file picker via rfd
            Message::OpenFilePicker => Task::future(async {
                let path = rfd::AsyncFileDialog::new()
                    .set_title("Select file to send")
                    .pick_file()
                    .await
                    .map(|f| f.path().to_path_buf());
                Message::FilePicked(path)
            }),
            Message::FilePicked(Some(path)) => {
                let name = path
                    .file_name()
                    .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                self.pending_attachments.push(Attachment {
                    path,
                    name,
                    size,
                    progress: 0,
                });
                Task::none()
            }
            Message::FilePicked(None) => Task::none(),
            Message::RemoveAttachment(idx) => {
                if idx < self.pending_attachments.len() {
                    self.pending_attachments.remove(idx);
                }
                Task::none()
            }
            Message::AttachmentProgress(idx, pct) => {
                if let Some(a) = self.pending_attachments.get_mut(idx) {
                    a.progress = pct;
                }
                Task::none()
            }
            // I1: clipboard image paste
            Message::PasteFromClipboard => {
                Task::future(async {
                    // Try to read image bytes from arboard clipboard
                    let result = tokio::task::spawn_blocking(|| {
                        let mut clipboard = arboard::Clipboard::new().ok()?;
                        let img = clipboard.get_image().ok()?;
                        // Encode RGBA pixels as PNG
                        let mut png_bytes: Vec<u8> = Vec::new();
                        let encoder = ::image::codecs::png::PngEncoder::new(&mut png_bytes);
                        ::image::ImageEncoder::write_image(
                            encoder,
                            &img.bytes,
                            img.width as u32,
                            img.height as u32,
                            ::image::ExtendedColorType::Rgba8,
                        )
                        .ok()?;
                        Some(png_bytes)
                    })
                    .await;
                    match result {
                        Ok(Some(bytes)) => Message::ClipboardImageReady(bytes),
                        _ => Message::PasteFromClipboard, // no-op if nothing available
                    }
                })
            }
            Message::ClipboardImageReady(bytes) => {
                // Stage the clipboard image as a temp file attachment
                let tmp_path = std::env::temp_dir().join("clipboard_paste.png");
                if std::fs::write(&tmp_path, &bytes).is_ok() {
                    let size = bytes.len() as u64;
                    self.pending_attachments.push(Attachment {
                        path: tmp_path,
                        name: "clipboard_paste.png".into(),
                        size,
                        progress: 0,
                    });
                }
                Task::none()
            }
            // I2: drag-drop files
            Message::FilesDropped(paths) => {
                for path in paths {
                    let name = path
                        .file_name()
                        .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    self.pending_attachments.push(Attachment {
                        path,
                        name,
                        size,
                        progress: 0,
                    });
                }
                self.drag_drop_active = false;
                Task::none()
            }
            // M2: K4 delivery receipt — peer confirmed receipt of the message
            Message::MessageDelivered(msg_id) => {
                let current = self.message_states.get(&msg_id).copied();
                if current != Some(MessageState::Read) {
                    self.message_states.insert(msg_id, MessageState::Delivered);
                }
                Task::none()
            }
            // M2: K5 read marker — peer displayed the message
            Message::MessageRead(msg_id) => {
                self.message_states.insert(msg_id, MessageState::Read);
                Task::none()
            }
            // M6: hover state
            Message::SetHoveredMessage(msg_id) => {
                self.hovered_message = msg_id;
                Task::none()
            }
            // L2: autocomplete — replace the trailing @prefix with @nick
            Message::MentionSelected(nick) => {
                if let Some(at_pos) = self.composer.rfind('@') {
                    self.composer.truncate(at_pos);
                    self.composer.push('@');
                    self.composer.push_str(&nick);
                    self.composer.push(' ');
                }
                self.mention_prefix = None;
                Task::none()
            }
            Message::MentionDismissed => {
                self.mention_prefix = None;
                Task::none()
            }
            // M4: start recording — spawn a dedicated thread that owns the cpal stream
            Message::StartRecording => {
                use cpal::traits::{DeviceTrait, HostTrait};
                use std::sync::atomic::{AtomicBool, Ordering};
                use std::sync::{mpsc, Arc};

                let host = cpal::default_host();
                let device = match host.default_input_device() {
                    Some(d) => d,
                    None => {
                        tracing::warn!("M4: no default input device available");
                        return Task::none();
                    }
                };
                let config = match device.default_input_config() {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("M4: failed to get default input config: {e}");
                        return Task::none();
                    }
                };
                let sample_rate = config.sample_rate().0;
                let channels = config.channels();

                let (tx, rx) = mpsc::channel::<Vec<i16>>();
                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_thread = stop_flag.clone();

                let thread = std::thread::spawn(move || {
                    use cpal::traits::{DeviceTrait, StreamTrait};

                    let err_fn = |e| tracing::warn!("M4: cpal stream error: {e}");
                    // Build stream based on sample format
                    let stream = match config.sample_format() {
                        cpal::SampleFormat::I16 => device.build_input_stream(
                            &config.into(),
                            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                                let _ = tx.send(data.to_vec());
                            },
                            err_fn,
                            None,
                        ),
                        cpal::SampleFormat::F32 => {
                            let tx2 = tx.clone();
                            device.build_input_stream(
                                &config.into(),
                                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                                    let samples: Vec<i16> = data
                                        .iter()
                                        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                                        .collect();
                                    let _ = tx2.send(samples);
                                },
                                err_fn,
                                None,
                            )
                        }
                        cpal::SampleFormat::U16 => {
                            let tx3 = tx.clone();
                            device.build_input_stream(
                                &config.into(),
                                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                                    let samples: Vec<i16> =
                                        data.iter().map(|&s| (s as i32 - 32768) as i16).collect();
                                    let _ = tx3.send(samples);
                                },
                                err_fn,
                                None,
                            )
                        }
                        _ => {
                            tracing::warn!("M4: unsupported sample format");
                            return;
                        }
                    };
                    match stream {
                        Ok(s) => {
                            if let Err(e) = s.play() {
                                tracing::warn!("M4: failed to start stream: {e}");
                                return;
                            }
                            // Keep the stream alive until the stop flag is set
                            while !stop_flag_thread.load(Ordering::Relaxed) {
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                            // stream drops here, stopping capture
                        }
                        Err(e) => {
                            tracing::warn!("M4: failed to build input stream: {e}");
                        }
                    }
                });

                self.voice_elapsed_secs = 0;
                self.voice_state = VoiceState::Recording(RecordingHandle {
                    rx,
                    stop_flag,
                    _thread: Some(thread),
                    sample_rate,
                    channels,
                });
                Task::none()
            }

            // M4: stop recording — collect samples, encode to WAV in blocking thread
            Message::StopRecording => {
                let handle = match std::mem::replace(&mut self.voice_state, VoiceState::Encoding) {
                    VoiceState::Recording(h) => h,
                    other => {
                        self.voice_state = other;
                        return Task::none();
                    }
                };
                // Signal the recording thread to stop
                handle
                    .stop_flag
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                // Drain all buffered chunks
                let mut all_samples: Vec<i16> = Vec::new();
                // Give the thread a moment to flush its final chunk
                std::thread::sleep(std::time::Duration::from_millis(50));
                while let Ok(chunk) = handle.rx.try_recv() {
                    all_samples.extend(chunk);
                }
                let sample_rate = handle.sample_rate;
                let channels = handle.channels;
                // Encode to WAV in a blocking task, then emit VoiceEncodingDone
                Task::future(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let id = uuid::Uuid::new_v4().to_string();
                        let path = std::env::temp_dir().join(format!("voice_{}.wav", id));
                        let spec = hound::WavSpec {
                            channels,
                            sample_rate,
                            bits_per_sample: 16,
                            sample_format: hound::SampleFormat::Int,
                        };
                        let mut writer = hound::WavWriter::create(&path, spec)
                            .map_err(|e| std::io::Error::other(e.to_string()))?;
                        for sample in &all_samples {
                            writer
                                .write_sample(*sample)
                                .map_err(|e| std::io::Error::other(e.to_string()))?;
                        }
                        writer
                            .finalize()
                            .map_err(|e| std::io::Error::other(e.to_string()))?;
                        let size = std::fs::metadata(&path)?.len();
                        Ok::<(std::path::PathBuf, u64), std::io::Error>((path, size))
                    })
                    .await;
                    match result {
                        Ok(Ok((path, size))) => Message::VoiceEncodingDone(path, size),
                        Ok(Err(e)) => {
                            tracing::warn!("M4: WAV encoding failed: {e}");
                            Message::CancelRecording
                        }
                        Err(e) => {
                            tracing::warn!("M4: spawn_blocking panicked: {e}");
                            Message::CancelRecording
                        }
                    }
                })
            }

            // M4: cancel recording — drop buffer, return to Idle
            Message::CancelRecording => {
                if let VoiceState::Recording(ref handle) = self.voice_state {
                    handle
                        .stop_flag
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.voice_state = VoiceState::Idle;
                self.voice_elapsed_secs = 0;
                Task::none()
            }

            // M4: encoding done — stage the WAV as an attachment and trigger Send
            Message::VoiceEncodingDone(path, size) => {
                self.voice_state = VoiceState::Uploading;
                self.pending_attachments.push(Attachment {
                    name: "voice_message.wav".into(),
                    path,
                    size,
                    progress: 0,
                });
                // Reuse the existing Send path which picks up pending_attachments
                Task::done(Message::Send)
            }

            // M4: periodic tick — update elapsed counter; auto-stop at 5 min
            Message::VoiceTick => {
                if matches!(self.voice_state, VoiceState::Recording(_)) {
                    self.voice_elapsed_secs += 1;
                    if self.voice_elapsed_secs >= VOICE_MAX_SECS {
                        return Task::done(Message::StopRecording);
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(
        &self,
        avatars: &std::collections::HashMap<String, Vec<u8>>,
        time_format: crate::config::TimeFormat,
        occupants: &[OccupantEntry],
        own_nick: &str,
    ) -> Element<'_, Message> {
        let ts_format = match time_format {
            crate::config::TimeFormat::TwentyFourHour => "%H:%M",
            crate::config::TimeFormat::TwelveHour => "%I:%M %p",
        };

        // ---- Message list (G5: grouping + date separators) ----
        let mut rows: Vec<Element<Message>> = Vec::new();
        let mut prev_date: Option<chrono::NaiveDate> = None;
        let mut prev_sender: Option<String> = None;
        let mut prev_ts: Option<i64> = None;

        let query_lower = self.search_query.to_lowercase();
        for (msg_idx, m) in self.messages.iter().enumerate() {
            // G9: skip non-matching messages when searching
            if !query_lower.is_empty() && !m.body.to_lowercase().contains(&query_lower) {
                continue;
            }
            // L1: insert "New messages" separator before the first unseen message (only when not searching)
            if query_lower.is_empty() && self.last_seen_count > 0 && msg_idx == self.last_seen_count
            {
                let sep = container(text("── New messages ──").size(11))
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .padding([4, 0]);
                rows.push(sep.into());
            }
            let sender = if m.own {
                "You".to_string()
            } else {
                m.from.split('/').next().unwrap_or(&m.from).to_string()
            };

            // G5: date separator when calendar date changes
            let msg_date = Utc
                .timestamp_millis_opt(m.timestamp)
                .single()
                .map(|dt| dt.date_naive());

            if let Some(date) = msg_date {
                if prev_date != Some(date) {
                    let label = date.format("%b %-d").to_string();
                    let sep = container(text(format!("── {} ──", label)).size(11))
                        .width(Length::Fill)
                        .align_x(Alignment::Center)
                        .padding([4, 0]);
                    rows.push(sep.into());
                    prev_date = Some(date);
                }
            }

            // G5: suppress sender label for consecutive same-sender within 120s
            let same_sender = prev_sender.as_deref() == Some(sender.as_str());
            let within_120s = prev_ts.is_some_and(|pt| (m.timestamp - pt).abs() < 120_000);
            let show_sender = !(same_sender && within_120s);

            // E2: retracted messages show a tombstone
            if m.retracted {
                let tombstone = container(text("(message retracted)").size(12))
                    .width(Length::Fill)
                    .padding([2, 8]);
                rows.push(tombstone.into());
                prev_sender = Some(sender);
                prev_ts = Some(m.timestamp);
                continue;
            }

            // G4: /me action rendering
            let body_widget: Element<Message> = if is_me_action(&m.body) {
                let action_text = &m.body[ME_PREFIX.len()..];
                let action_str = format!("* {} {} *", sender, action_text);
                let italic_span: IcedSpan<'static, Message> = span(action_str).font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                });
                rich_text([italic_span]).size(14).into()
            } else if let Some(handle) = self.attachments.get(&m.id) {
                // I4: render inline image thumbnail (max 320px wide)
                image(handle.clone()).width(320).into()
            } else {
                let styled_spans = styling::parse(&m.body);
                build_styled_text(&styled_spans)
            };

            // G7: copy button with tooltip
            let copy_btn = tooltip(
                button(text("cp").size(10))
                    .on_press(Message::CopyToClipboard(m.body.clone()))
                    .padding([2, 6]),
                "Copy message",
                tooltip::Position::Top,
            );
            // G3: reply button with tooltip
            let msg_id = m.id.clone();
            let preview: String = m.body.chars().take(60).collect();
            let reply_btn = tooltip(
                button(text("re").size(10))
                    .on_press(Message::ReplyTo(msg_id, preview))
                    .padding([2, 4]),
                "Reply",
                tooltip::Position::Top,
            );
            // R1/M6: quick-react bar (5 emoji) — only visible on hover; toggles own reactions
            let is_hovered = self.hovered_message.as_ref() == Some(&m.id);
            let react_row: Element<Message> = if is_hovered {
                const QUICK_EMOJIS: [(&str, &str); 5] = [
                    ("👍", "Thumbs up"),
                    ("❤️", "Heart"),
                    ("😂", "Laugh"),
                    ("😮", "Wow"),
                    ("😢", "Sad"),
                ];
                let own_rxns: Vec<String> = self
                    .reactions
                    .get(&m.id)
                    .and_then(|by_jid| by_jid.get(&self.own_jid))
                    .cloned()
                    .unwrap_or_default();
                let mut quick_row: iced::widget::Row<Message> = row![].spacing(4);
                for (emoji, label) in QUICK_EMOJIS {
                    let already = own_rxns.contains(&emoji.to_string());
                    let tip = if already {
                        format!("{} (click to remove)", label)
                    } else {
                        label.to_string()
                    };
                    let mid = m.id.clone();
                    quick_row = quick_row.push(tooltip(
                        button(text(emoji).size(10).shaping(Shaping::Advanced))
                            .on_press(Message::ToggleReaction(mid, emoji.to_string()))
                            .padding([2, 4]),
                        text(tip).size(12),
                        tooltip::Position::Top,
                    ));
                }
                quick_row.into()
            } else {
                row![].spacing(4).into()
            };

            let edit_msg_id = m.id.clone();
            let edit_body = m.body.clone();
            let edit_btn = tooltip(
                button(text("✏").size(10))
                    .on_press(Message::StartEdit(edit_msg_id, edit_body))
                    .padding([2, 4]),
                "Edit message",
                tooltip::Position::Top,
            );
            // E2: retract button (own messages only)
            let retract_msg_id = m.id.clone();
            let retract_btn = tooltip(
                button(text("🗑").size(10))
                    .on_press(Message::RetractMessage(retract_msg_id))
                    .padding([2, 4]),
                "Retract message",
                tooltip::Position::Top,
            );
            // L3: moderate button — shown on hover, non-own messages, moderator only
            let is_moderator = occupants
                .iter()
                .any(|o| o.nick == own_nick && o.role == "Moderator");
            let moderate_btn: Option<iced::widget::Tooltip<Message>> =
                if is_hovered && is_moderator && !m.own && !m.retracted {
                    let mod_msg_id = m.id.clone();
                    Some(tooltip(
                        button(text("\u{1F6E1}").size(10))
                            .on_press(Message::OpenModerateDialog(mod_msg_id))
                            .padding([2, 4]),
                        "Moderate (remove) message",
                        tooltip::Position::Top,
                    ))
                } else {
                    None
                };

            let align = if m.own {
                Alignment::End
            } else {
                Alignment::Start
            };

            // G3: quoted block rendered inline in text_col below

            let row_elem: Element<Message> = if is_me_action(&m.body) {
                // /me: centered italic, no avatar, no sender label
                container(container(body_widget).padding([4, 12]))
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                    .into()
            } else if !m.own {
                // H5/H1: avatar + sender + body for incoming messages
                let from_bare = m.from.split('/').next().unwrap_or(&m.from);
                let avatar: Element<Message> = if let Some(png) = avatars.get(from_bare) {
                    let handle = iced_image::Handle::from_bytes(png.clone());
                    image(handle).width(24).height(24).into()
                } else {
                    let color = jid_color(from_bare);
                    let initial = jid_initial(from_bare).to_string();
                    container(text(initial).size(11))
                        .width(24)
                        .height(24)
                        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                            background: Some(iced::Background::Color(color)),
                            ..Default::default()
                        })
                        .align_x(Alignment::Center)
                        .align_y(Alignment::Center)
                        .into()
                };

                let text_col = if show_sender {
                    let mut header_row = row![text(sender.clone()).size(11).shaping(Shaping::Advanced), copy_btn, reply_btn,]
                        .spacing(8)
                        .align_y(Alignment::Center);
                    if let Some(btn) = moderate_btn {
                        header_row = header_row.push(btn);
                    }
                    let mut col = column![header_row].spacing(2).padding([0, 6]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        col = col.push(
                            container(text(format!("↩ {}", preview)).size(11)).padding([2, 6]),
                        );
                    }
                    col.push(body_widget)
                } else {
                    let mut col = column![].spacing(2).padding([0, 6]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        col = col.push(
                            container(text(format!("↩ {}", preview)).size(11)).padding([2, 6]),
                        );
                    }
                    col.push(body_widget)
                };

                let bubble = row![avatar, text_col].spacing(6).align_y(Alignment::Start);
                container(bubble)
                    .width(Length::Fill)
                    .align_x(align)
                    .padding([2, 8])
                    .into()
            } else {
                // Own message: right-aligned, no avatar
                let own_ts_label = if m.timestamp > 0 {
                    Utc.timestamp_millis_opt(m.timestamp)
                        .single()
                        .map(|dt| dt.format(ts_format).to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let edited_label: Option<Element<Message>> = if m.edited {
                    Some(text("(edited)").size(10).into())
                } else {
                    None
                };
                let text_col = if show_sender {
                    let mut col = column![
                        row![
                            text(sender.clone()).size(11).shaping(Shaping::Advanced),
                            copy_btn,
                            reply_btn,
                            edit_btn,
                            retract_btn
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        body_widget,
                    ]
                    .spacing(2)
                    .padding([6, 10]);
                    if let Some(lbl) = edited_label {
                        col = col.push(lbl);
                    }
                    col.push(text(own_ts_label).size(10))
                } else {
                    let mut col = column![body_widget].spacing(2).padding([2, 10]);
                    if let Some(lbl) = edited_label {
                        col = col.push(lbl);
                    }
                    col.push(text(own_ts_label).size(10))
                };
                container(text_col)
                    .width(Length::Fill)
                    .align_x(align)
                    .into()
            };

            // L2: wrap in amber highlight if own_nick is @-mentioned in this message
            let is_mentioned = !own_nick.is_empty() && m.body.contains(&format!("@{}", own_nick));
            let row_elem: Element<Message> = if is_mentioned {
                container(row_elem)
                    .width(Length::Fill)
                    .style(|_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgba(
                            0.98, 0.85, 0.20, 0.25,
                        ))),
                        ..Default::default()
                    })
                    .into()
            } else {
                row_elem
            };

            // M6: wrap in mouse_area for hover detection and add react row below
            let msg_id_for_hover = m.id.clone();
            let row_elem = mouse_area(row_elem)
                .on_enter(Message::SetHoveredMessage(Some(msg_id_for_hover.clone())))
                .on_exit(Message::SetHoveredMessage(None));
            rows.push(row_elem.into());
            // M6: render reaction buttons below message when hovered
            rows.push(react_row);

            // E3/R1: render reaction pills below the message bubble
            // R1: pills show who reacted (tooltip) and toggle own reaction on click
            if let Some(by_jid) = self.reactions.get(&m.id) {
                // Group: emoji → list of reactor display names
                let mut reactor_lists: std::collections::BTreeMap<&str, Vec<&str>> =
                    std::collections::BTreeMap::new();
                for (jid, emojis) in by_jid {
                    let display = jid.split('/').next().unwrap_or(jid.as_str());
                    for e in emojis {
                        reactor_lists.entry(e.as_str()).or_default().push(display);
                    }
                }
                if !reactor_lists.is_empty() {
                    let mut pill_row: iced::widget::Row<Message> =
                        row![].spacing(4).padding([0, 8]);
                    for (emoji, reactors) in &reactor_lists {
                        let emoji_str = emoji.to_string();
                        let label = format!("{} {}", emoji_str, reactors.len());
                        let tip = reactors.join(", ");
                        let mid = m.id.clone();
                        pill_row = pill_row.push(tooltip(
                            button(text(label).size(12).shaping(Shaping::Advanced))
                                .on_press(Message::ToggleReaction(mid, emoji_str))
                                .padding([2, 6]),
                            text(tip).size(12).shaping(Shaping::Advanced),
                            tooltip::Position::Top,
                        ));
                    }
                    let pill_align = if m.own {
                        Alignment::End
                    } else {
                        Alignment::Start
                    };
                    rows.push(
                        container(pill_row)
                            .width(Length::Fill)
                            .align_x(pill_align)
                            .into(),
                    );
                }
            }

            // E5: render link preview card below message
            if let Some(preview) = self.previews.get(&m.id) {
                let preview_card = render_preview_card(preview.clone(), m.own, None);
                rows.push(preview_card);
            }

            prev_sender = Some(sender);
            prev_ts = Some(m.timestamp);
        }

        let list_col = rows
            .into_iter()
            .fold(column![].spacing(4).padding(8), iced::widget::Column::push);

        let scroll_area = scrollable(list_col)
            .id(self.scroll_id.clone())
            .on_scroll(|vp| Message::Scrolled(vp.absolute_offset()))
            .height(Length::Fill)
            .width(Length::Fill);

        // ---- Jump-to-bottom button (only visible when not at bottom) ----
        let jump_btn = tooltip(
            button(text("↓").size(12))
                .on_press(Message::ScrollToBottom)
                .padding([4, 10]),
            "Jump to bottom",
            tooltip::Position::Top,
        );
        // M2: delivery/read status indicator — shown for the last own message
        let status_indicator: Element<Message> = {
            let last_own = self.messages.iter().rev().find(|m| m.own);
            if let Some(msg) = last_own {
                let state = self.message_states.get(&msg.id).copied();
                let label = match state {
                    None => "·", // sending
                    Some(MessageState::Sending) => "·",
                    Some(MessageState::Sent) => "✓",
                    Some(MessageState::Delivered) => "✓✓",
                    Some(MessageState::Read) => "✓✓",
                };
                let color = if state == Some(MessageState::Read) {
                    iced::Color::from_rgb(0.0, 0.67, 1.0) // blue for read
                } else {
                    iced::Color::from_rgb(0.5, 0.5, 0.5) // gray for sent/delivered
                };
                text(label).size(12).color(color).into()
            } else {
                text("").size(12).into()
            }
        };
        let scroll_bar = row![jump_btn, status_indicator]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding([2, 8]);

        // ---- Composer ----
        // G3: reply quote strip
        let reply_strip: Option<Element<Message>> = self.reply_to.as_ref().map(|(_id, preview)| {
            let cancel_btn = button(text("✕").size(10))
                .on_press(Message::CancelReply)
                .padding([2, 4]);
            let strip = row![
                text(format!("↩ {}", preview)).size(11).width(Length::Fill),
                cancel_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .padding([4, 8]);
            container(strip).width(Length::Fill).into()
        });

        // E1: edit-mode strip above composer
        let edit_strip: Option<Element<Message>> = self.edit_mode.as_ref().map(|(_id, _orig)| {
            let cancel_btn = button(text("✕").size(10))
                .on_press(Message::CancelEdit)
                .padding([2, 4]);
            let strip = row![
                text("✏ Editing message").size(11).width(Length::Fill),
                cancel_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .padding([4, 8]);
            container(strip).width(Length::Fill).into()
        });

        let can_send = !self.composer.trim().is_empty();
        let send_label = if self.edit_mode.is_some() {
            "Save"
        } else {
            "Send"
        };
        let send_btn = if can_send {
            button(send_label).on_press(Message::Send)
        } else {
            button(send_label)
        };

        // M3: emoji picker panel (rendered above composer when open)
        let emoji_panel: Option<Element<Message>> = if self.emoji_picker_open {
            let mut picker_col: iced::widget::Column<Message> = column![].spacing(4).padding(6);
            for (group_name, emojis) in EMOJI_LIST {
                picker_col = picker_col.push(text(*group_name).size(11));
                let mut row_acc: iced::widget::Row<Message> = row![].spacing(2);
                for (i, emoji) in emojis.iter().enumerate() {
                    let e = emoji.to_string();
                    row_acc = row_acc.push(
                        button(text(e.clone()).size(18))
                            .on_press(Message::EmojiSelected(e))
                            .padding([2, 4]),
                    );
                    if (i + 1) % 8 == 0 {
                        picker_col = picker_col.push(row_acc);
                        row_acc = row![].spacing(2);
                    }
                }
                // push any remaining emoji in the last partial row
                picker_col = picker_col.push(row_acc);
            }
            let panel = container(scrollable(picker_col).height(180))
                .width(Length::Fill)
                .padding([4, 8]);
            Some(panel.into())
        } else {
            None
        };

        // L2: @mention autocomplete panel — shown above composer when mention_prefix is Some
        let mention_panel: Option<Element<Message>> = if let Some(ref prefix) = self.mention_prefix
        {
            let prefix_lower = prefix.to_lowercase();
            let matches: Vec<String> = occupants
                .iter()
                .filter(|o| o.available && o.nick.to_lowercase().starts_with(&prefix_lower))
                .map(|o| o.nick.clone())
                .collect();
            if matches.is_empty() {
                None
            } else {
                let mut panel_col: iced::widget::Column<Message> =
                    column![].spacing(2).padding([4, 8]);
                // Dismiss button at the top
                panel_col = panel_col.push(
                    button(text("✕ Dismiss").size(10))
                        .on_press(Message::MentionDismissed)
                        .padding([2, 6]),
                );
                for nick in matches {
                    let nick_clone = nick.clone();
                    panel_col = panel_col.push(
                        button(text(format!("@{}", nick)).size(13))
                            .on_press(Message::MentionSelected(nick_clone))
                            .padding([4, 8])
                            .width(Length::Fill),
                    );
                }
                Some(
                    container(panel_col)
                        .width(Length::Fill)
                        .padding([2, 0])
                        .into(),
                )
            }
        } else {
            None
        };

        let emoji_btn = button(text("😊").size(14))
            .on_press(Message::EmojiPickerToggled)
            .padding([6, 8]);

        // E4/I3: paperclip button for file picker
        let attach_btn = tooltip(
            button(text("📎").size(14))
                .on_press(Message::OpenFilePicker)
                .padding([6, 8]),
            "Attach file",
            tooltip::Position::Top,
        );

        // M4: composer row switches to recording strip when recording is active
        let composer_row = match &self.voice_state {
            VoiceState::Idle => {
                // Normal composer with mic button on the right of attach
                let mic_btn = tooltip(
                    button(text("🎤").size(14))
                        .on_press(Message::StartRecording)
                        .padding([6, 8]),
                    "Record voice message",
                    tooltip::Position::Top,
                );
                row![
                    emoji_btn,
                    attach_btn,
                    mic_btn,
                    text_input("Type a message…", &self.composer)
                        .on_input(Message::ComposerChanged)
                        .on_submit(Message::Send)
                        .padding(10)
                        .width(Length::Fill),
                    send_btn.padding([10, 16]),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding([4, 8])
            }
            VoiceState::Recording(_) => {
                let mins = self.voice_elapsed_secs / 60;
                let secs = self.voice_elapsed_secs % 60;
                let elapsed_str = format!("🔴 {}:{:02}", mins, secs);
                row![
                    button(text("✕ Cancel").size(13))
                        .on_press(Message::CancelRecording)
                        .padding([8, 12]),
                    text(elapsed_str).size(14).width(Length::Fill),
                    button(text("■ Stop").size(13))
                        .on_press(Message::StopRecording)
                        .padding([8, 12]),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding([4, 8])
            }
            VoiceState::Encoding | VoiceState::Uploading => {
                row![text("Sending voice message…").size(13).width(Length::Fill),]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .padding([4, 8])
            }
        };

        // I3: pending attachments strip above composer
        let attachments_strip: Option<Element<Message>> = if !self.pending_attachments.is_empty() {
            let mut att_col: iced::widget::Column<Message> = column![].spacing(2).padding([4, 8]);
            for (i, att) in self.pending_attachments.iter().enumerate() {
                let size_kb = att.size / 1024;
                let label = format!("{} ({}KB)", att.name, size_kb);
                let remove_btn = button(text("✕").size(10))
                    .on_press(Message::RemoveAttachment(i))
                    .padding([2, 4]);
                let progress_bar = container(
                    container(text("").size(1))
                        .width(Length::Fixed(att.progress as f32 * 2.0))
                        .height(4)
                        .style(|_theme: &iced::Theme| iced::widget::container::Style {
                            background: Some(iced::Background::Color(iced::Color::from_rgb(
                                0.2, 0.7, 0.3,
                            ))),
                            ..Default::default()
                        }),
                )
                .width(200)
                .height(4)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgb(
                        0.3, 0.3, 0.3,
                    ))),
                    ..Default::default()
                });
                let att_row = row![
                    text(label).size(11).width(Length::Fill),
                    progress_bar,
                    remove_btn,
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                att_col = att_col.push(att_row);
            }
            Some(container(att_col).width(Length::Fill).into())
        } else {
            None
        };

        let close_btn = tooltip(
            button(text("×").size(14))
                .on_press(Message::Close)
                .padding([4, 10]),
            "Close conversation",
            tooltip::Position::Bottom,
        );
        let block_btn = if self.peer_blocked {
            tooltip(
                button(text("Unblock"))
                    .on_press(Message::UnblockPeer)
                    .padding([4, 8]),
                "Unblock this contact",
                tooltip::Position::Bottom,
            )
        } else {
            tooltip(
                button(text("Block"))
                    .on_press(Message::BlockPeer)
                    .padding([4, 8]),
                "Block this contact",
                tooltip::Position::Bottom,
            )
        };
        let mute_label = if self.is_muted { "Unmute" } else { "Mute" };
        let mute_tip = if self.is_muted {
            "Unmute notifications"
        } else {
            "Mute notifications"
        };
        let mute_btn = tooltip(
            button(text(mute_label))
                .on_press(Message::ToggleMute)
                .padding([4, 8]),
            mute_tip,
            tooltip::Position::Bottom,
        );
        let search_btn = tooltip(
            button(text("⌕").size(14))
                .on_press(Message::SearchToggled)
                .padding([4, 8]),
            "Search messages",
            tooltip::Position::Bottom,
        );
        let match_count = if !self.search_query.is_empty() {
            self.messages
                .iter()
                .filter(|m| {
                    m.body
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
                })
                .count()
        } else {
            0
        };
        let header_content: Element<Message> = if self.search_open {
            row![
                text_input("Search…", &self.search_query)
                    .on_input(Message::SearchQueryChanged)
                    .padding(6)
                    .width(Length::Fill),
                text(format!("{} results", match_count)).size(11),
                search_btn,
                close_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                text(format!("Chat with {}", self.peer_jid))
                    .size(14)
                    .width(Length::Fill),
                block_btn,
                mute_btn,
                search_btn,
                close_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        };
        let header = container(header_content)
            .padding([8, 12])
            .width(Length::Fill);

        let mut col = column![header, scroll_area, scroll_bar];
        if let Some(strip) = reply_strip {
            col = col.push(strip);
        }
        if let Some(strip) = edit_strip {
            col = col.push(strip);
        }
        if let Some(strip) = attachments_strip {
            col = col.push(strip);
        }
        if let Some(panel) = emoji_panel {
            col = col.push(panel);
        }
        if let Some(panel) = mention_panel {
            col = col.push(panel);
        }
        col = col.push(composer_row);

        let body = container(col).height(Length::Fill).width(Length::Fill);

        if self.pending_moderate_dialog.is_some() {
            let dialog = container(
                column![
                    text("Moderate Message").size(16),
                    text("Enter reason (optional):").size(14),
                    text_input("e.g. Inappropriate behavior", &self.moderate_reason_input)
                        .on_input(Message::ModerateReasonChanged)
                        .on_submit(Message::SubmitModerate)
                        .padding(8),
                    row![
                        button("Cancel").on_press(Message::DismissModerateDialog).padding([6, 12]),
                        button(text("Moderate").color(Color::from_rgb(1.0, 0.4, 0.4)))
                            .on_press(Message::SubmitModerate).padding([6, 12]),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                ]
                .spacing(12)
            )
            .padding(20)
            .style(|_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.15, 0.15, 0.15))),
                border: iced::Border {
                    color: Color::from_rgb(0.3, 0.3, 0.3),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 10.0,
                },
                ..Default::default()
            });

            iced::widget::stack![
                body,
                container(dialog)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center(Length::Fill)
                    .style(|_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.7))),
                        ..Default::default()
                    })
            ]
            .into()
        } else {
            body.into()
        }
    }
}

/// Map parsed `Span`s to an iced `rich_text` widget.
fn build_styled_text(spans: &[styling::Span]) -> Element<'static, Message> {
    let iced_spans: Vec<IcedSpan<'static, Message>> = spans
        .iter()
        .map(|s| {
            let t: IcedSpan<'static, Message> = span(s.text.clone());
            match s.style {
                SpanStyle::Plain => t,
                SpanStyle::Bold => t.font(Font {
                    weight: font::Weight::Bold,
                    ..Font::DEFAULT
                }),
                SpanStyle::Italic => t.font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                }),
                SpanStyle::Code => t
                    .font(Font::MONOSPACE)
                    .color(Color::from_rgb(0.2, 0.8, 0.4)),
                SpanStyle::Strike => t.strikethrough(true),
                SpanStyle::Quote => t.color(Color::from_rgb(0.6, 0.6, 0.6)).font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                }),
            }
        })
        .collect();
    rich_text(iced_spans).size(14).into()
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
