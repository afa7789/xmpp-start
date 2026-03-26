use iced::widget::scrollable::{self, AbsoluteOffset};
use iced::Task;

use super::{
    thumbnail_for_path, Attachment, ConversationView, Message, RecordingHandle, VoiceState,
    VOICE_MAX_SECS,
};

impl ConversationView {
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
            Message::OpenOmemoTrust(_) => Task::none(), // bubbled to ChatScreen
            Message::SetEncryptionMode(mode) => {
                self.encryption_mode = mode;
                self.encryption_popover_open = false;
                Task::none()
            }
            Message::ToggleEncryptionPopover => {
                self.encryption_popover_open = !self.encryption_popover_open;
                Task::none()
            }
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
                // DC-17: generate thumbnail preview for image files
                let thumbnail = thumbnail_for_path(&path);
                self.pending_attachments.push(Attachment {
                    path,
                    name,
                    size,
                    progress: 0,
                    thumbnail,
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
                    // DC-17: generate thumbnail from the PNG bytes we just wrote
                    let thumbnail = crate::store::thumbnail::generate(&bytes)
                        .ok()
                        .map(|t| t.data);
                    self.pending_attachments.push(Attachment {
                        path: tmp_path,
                        name: "clipboard_paste.png".into(),
                        size,
                        progress: 0,
                        thumbnail,
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
                    // DC-17: generate thumbnail preview for image files
                    let thumbnail = thumbnail_for_path(&path);
                    self.pending_attachments.push(Attachment {
                        path,
                        name,
                        size,
                        progress: 0,
                        thumbnail,
                    });
                }
                self.drag_drop_active = false;
                Task::none()
            }
            // M2: K4 delivery receipt — peer confirmed receipt of the message
            Message::MessageDelivered(msg_id) => {
                let current = self.message_states.get(&msg_id).copied();
                if current != Some(super::MessageState::Read) {
                    self.message_states
                        .insert(msg_id, super::MessageState::Delivered);
                }
                Task::none()
            }
            // M2: K5 read marker — peer displayed the message
            Message::MessageRead(msg_id) => {
                self.message_states
                    .insert(msg_id, super::MessageState::Read);
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
                    thumbnail: None, // WAV files don't have thumbnails
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
}
