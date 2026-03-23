// Task P2.3 — ChatView: message list (virtual scroll)
// Task P2.4 — MessageComposer: text input + send button
// Source reference: apps/fluux/src/components/ChatView.tsx
//                   apps/fluux/src/components/MessageComposer.tsx
// Scroll strategy: docs/SCROLL_STRATEGY.md

use iced::widget::image as iced_image;
use iced::widget::scrollable::{AbsoluteOffset, Id};
use iced::widget::text::Span as IcedSpan;
use iced::{
    font,
    widget::{
        button, column, container, image, rich_text, row, scrollable, span, text, text_input,
        tooltip,
    },
    Alignment, Color, Element, Font, Length, Task,
};

use chrono::{TimeZone, Utc};

use crate::xmpp::modules::link_preview::LinkPreview;
use ::image::ImageEncoder;

// G4: /me action message prefix (XEP-0245)
const ME_PREFIX: &str = "/me ";

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
}

#[derive(Debug, Clone)]
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
    SendReaction(String, String), // E3: (msg_id, emoji)
    LinkPreviewReady(String, LinkPreview), // E5: (msg_id, preview)
    StartEdit(String, String),    // E1: (msg_id, current_body) — populate composer for edit
    CancelEdit,                   // E1: cancel edit mode
    RetractMessage(String),       // E2: (msg_id) — retract own message
    RequestOlderHistory,          // G8: emitted on scroll to top to request older MAM history
    AttachmentLoaded(String, iced_image::Handle), // I4: (msg_id, image_handle)
    // E4/I3: file upload
    OpenFilePicker,                              // E4: open native file picker
    FilePicked(Option<std::path::PathBuf>),      // E4: result from file picker
    RemoveAttachment(usize),                     // I3: remove staged attachment by index
    AttachmentProgress(usize, u8),              // I3: (index, progress 0–100)
    // I1: clipboard paste
    PasteFromClipboard,
    ClipboardImageReady(Vec<u8>),               // I1: PNG bytes from clipboard
    // I2: drag-drop
    FilesDropped(Vec<std::path::PathBuf>),       // I2: files dropped onto composer
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
            Message::RequestOlderHistory => Task::none(), // bubbled to ChatScreen
            Message::AttachmentLoaded(msg_id, handle) => {
                self.attachments.insert(msg_id, handle);
                Task::none()
            }
            // E4/I3: open native file picker via rfd
            Message::OpenFilePicker => {
                Task::future(async {
                    let path = rfd::AsyncFileDialog::new()
                        .set_title("Select file to send")
                        .pick_file()
                        .await
                        .map(|f| f.path().to_path_buf());
                    Message::FilePicked(path)
                })
            }
            Message::FilePicked(Some(path)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
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
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file".into());
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
        }
    }

    pub fn view(&self, avatars: &std::collections::HashMap<String, Vec<u8>>) -> Element<'_, Message> {
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
            // E3: quick-react button (👍)
            let react_msg_id = m.id.clone();
            let react_btn = tooltip(
                button(text("👍").size(10))
                    .on_press(Message::SendReaction(react_msg_id, "👍".to_string()))
                    .padding([2, 4]),
                "React",
                tooltip::Position::Top,
            );
            // E1: edit button (own messages only)
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
                    let mut col = column![row![
                        text(sender.clone()).size(11),
                        copy_btn,
                        reply_btn,
                        react_btn
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),]
                    .spacing(2)
                    .padding([0, 6]);
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
                        .map(|dt| dt.format("%H:%M").to_string())
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
                            text(sender.clone()).size(11),
                            copy_btn,
                            reply_btn,
                            react_btn,
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

            rows.push(row_elem);

            // E3: render reaction pills below the message bubble
            if let Some(by_jid) = self.reactions.get(&m.id) {
                // Group emojis across all JIDs and count
                let mut counts: std::collections::BTreeMap<&str, usize> =
                    std::collections::BTreeMap::new();
                for emojis in by_jid.values() {
                    for e in emojis {
                        *counts.entry(e.as_str()).or_insert(0) += 1;
                    }
                }
                if !counts.is_empty() {
                    let mut pill_row: iced::widget::Row<Message> =
                        row![].spacing(4).padding([0, 8]);
                    for (emoji, count) in &counts {
                        let emoji_str = emoji.to_string();
                        let label = format!("{} {}", emoji_str, count);
                        pill_row = pill_row.push(container(text(label).size(12)).padding([2, 6]));
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
                let preview_card = render_preview_card(preview.clone(), m.own);
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
        let scroll_bar = row![jump_btn]
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
        let send_label = if self.edit_mode.is_some() { "Save" } else { "Send" };
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

        let composer_row = row![
            emoji_btn,
            attach_btn,
            text_input("Type a message…", &self.composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::Send)
                .padding(10)
                .width(Length::Fill),
            send_btn.padding([10, 16]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([4, 8]);

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
                            background: Some(iced::Background::Color(
                                iced::Color::from_rgb(0.2, 0.7, 0.3),
                            )),
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
        col.push(composer_row).height(Length::Fill).into()
    }
}

fn render_preview_card(preview: LinkPreview, own: bool) -> Element<'static, Message> {
    let mut card_col: iced::widget::Column<Message> = column![].spacing(4).padding([8, 10]);

    if let Some(ref site_name) = preview.site_name {
        card_col = card_col.push(
            text(site_name.clone())
                .size(10)
                .color(Color::from_rgb(0.5, 0.5, 0.5)),
        );
    }

    if let Some(ref title) = preview.title {
        card_col = card_col.push(text(title.clone()).size(13).font(Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        }));
    }

    if let Some(ref desc) = preview.description {
        let desc_text: String = desc.chars().take(150).collect();
        card_col = card_col.push(text(desc_text).size(12));
    }

    if let Some(ref image_url) = preview.image_url {
        card_col = card_col.push(
            text(image_url.clone())
                .size(10)
                .color(Color::from_rgb(0.4, 0.6, 1.0)),
        );
    }

    let card = container(card_col)
        .width(300)
        .style(|_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.15, 0.15, 0.18))),
            border: iced::Border {
                color: Color::from_rgb(0.3, 0.3, 0.35),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    let align = if own {
        Alignment::End
    } else {
        Alignment::Start
    };
    container(card).width(Length::Fill).align_x(align).into()
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
