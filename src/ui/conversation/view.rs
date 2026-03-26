use iced::widget::image as iced_image;
use iced::widget::text::Shaping;
use iced::widget::text::Span as IcedSpan;
use iced::{
    font,
    widget::{
        button, column, container, image, mouse_area, rich_text, row, scrollable, span, text,
        text_input, tooltip,
    },
    Alignment, Element, Font, Length,
};

use chrono::{TimeZone, Utc};

use crate::ui::avatar::{jid_color, jid_initial};
use crate::ui::link_preview::{domain_label, render_preview_card};
use crate::ui::muc_panel::OccupantEntry;
use crate::ui::omemo_trust::encryption_badge;
use crate::ui::palette;
use crate::ui::styling::{self, SpanStyle};

use super::{
    is_me_action, ConversationView, Message, MessageState, VoiceState, EMOJI_LIST, ME_PREFIX,
};

impl ConversationView {
    pub fn view(
        &self,
        vctx: &super::super::ViewContext<'_>,
        occupants: &[OccupantEntry],
        own_nick: &str,
    ) -> Element<'_, Message> {
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
                let sep = container(text("-- New messages --").size(11))
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
                    let sep = container(text(format!("-- {} --", label)).size(11))
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

            // OMEMO: wrap body with lock/shield icon when the message was decrypted via OMEMO
            let body_widget: Element<Message> = if m.is_encrypted {
                let peer_jid_for_trust = m.from.split('/').next().unwrap_or(&m.from).to_string();
                let tip_text = if m.is_trusted {
                    "OMEMO encrypted (trusted) — click to view fingerprints"
                } else {
                    "OMEMO encrypted (untrusted) — click to view fingerprints"
                };
                let lock_badge = tooltip(
                    button(encryption_badge::<Message>(true, m.is_trusted))
                        .on_press(Message::OpenOmemoTrust(peer_jid_for_trust))
                        .padding([0, 4]),
                    tip_text,
                    tooltip::Position::Top,
                );
                row![lock_badge, body_widget]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into()
            } else {
                body_widget
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
                if m.own {
                    container(quick_row)
                        .width(Length::Fill)
                        .align_x(Alignment::End)
                        .into()
                } else {
                    quick_row.into()
                }
            } else {
                row![].spacing(4).into()
            };

            let edit_msg_id = m.id.clone();
            let edit_body = m.body.clone();
            let edit_btn = tooltip(
                button(text("✎").size(10).shaping(Shaping::Advanced))
                    .on_press(Message::StartEdit(edit_msg_id, edit_body))
                    .padding([2, 4]),
                "Edit message",
                tooltip::Position::Top,
            );
            // E2: retract button (own messages only)
            let retract_msg_id = m.id.clone();
            let retract_btn = tooltip(
                button(text("✕").size(10))
                    .on_press(Message::RetractMessage(retract_msg_id))
                    .padding([2, 4]),
                "Retract message",
                tooltip::Position::Top,
            );
            // P2/L3: moderate button — shown on hover for any MUC message when user is moderator
            let is_moderator = occupants
                .iter()
                .any(|o| o.nick == own_nick && o.role == "Moderator");
            let moderate_btn: Option<iced::widget::Tooltip<Message>> =
                if is_hovered && is_moderator && !m.retracted {
                    let mod_msg_id = m.id.clone();
                    Some(tooltip(
                        button(text("⊘").size(10).shaping(Shaping::Advanced))
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
                let avatar: Element<Message> = if let Some(png) = vctx.avatars.get(from_bare) {
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
                    let mut header_row = row![
                        text(sender.clone()).size(11).shaping(Shaping::Advanced),
                        copy_btn,
                        reply_btn,
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    if let Some(btn) = moderate_btn {
                        header_row = header_row.push(btn);
                    }
                    let mut col = column![header_row].spacing(4).padding([0, 6]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        let truncated: String = preview.chars().take(100).collect();
                        let quote = container(text(truncated).size(11)).padding([4, 8]).style(
                            |_theme: &iced::Theme| iced::widget::container::Style {
                                background: Some(iced::Background::Color(palette::QUOTE_BG)),
                                border: iced::Border {
                                    color: palette::QUOTE_BORDER,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..Default::default()
                            },
                        );
                        col = col.push(quote);
                    }
                    col.push(body_widget)
                } else {
                    let mut col = column![].spacing(4).padding([0, 6]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        let truncated: String = preview.chars().take(100).collect();
                        let quote = container(text(truncated).size(11)).padding([4, 8]).style(
                            |_theme: &iced::Theme| iced::widget::container::Style {
                                background: Some(iced::Background::Color(palette::QUOTE_BG)),
                                border: iced::Border {
                                    color: palette::QUOTE_BORDER,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..Default::default()
                            },
                        );
                        col = col.push(quote);
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
                    vctx.time_format.format_timestamp(m.timestamp)
                } else {
                    String::new()
                };
                let edited_label: Option<Element<Message>> = if m.edited {
                    Some(text("(edited)").size(10).into())
                } else {
                    None
                };
                let text_col = if show_sender {
                    let mut own_header = row![
                        text(sender.clone()).size(11).shaping(Shaping::Advanced),
                        copy_btn,
                        reply_btn,
                        edit_btn,
                        retract_btn
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    // P2: moderator retract button — also shown on own messages for moderators
                    if let Some(btn) = moderate_btn {
                        own_header = own_header.push(btn);
                    }
                    let mut col = column![own_header].spacing(4).padding([6, 10]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        let truncated: String = preview.chars().take(100).collect();
                        let quote = container(text(truncated).size(11)).padding([4, 8]).style(
                            |_theme: &iced::Theme| iced::widget::container::Style {
                                background: Some(iced::Background::Color(palette::QUOTE_BG)),
                                border: iced::Border {
                                    color: palette::QUOTE_BORDER,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..Default::default()
                            },
                        );
                        col = col.push(quote);
                    }
                    col = col.push(body_widget);
                    if let Some(lbl) = edited_label {
                        col = col.push(lbl);
                    }
                    col.push(text(own_ts_label).size(10))
                } else {
                    let mut col = column![].spacing(4).padding([2, 10]);
                    if let Some(preview) = m.reply_preview.as_ref() {
                        let truncated: String = preview.chars().take(100).collect();
                        let quote = container(text(truncated).size(11)).padding([4, 8]).style(
                            |_theme: &iced::Theme| iced::widget::container::Style {
                                background: Some(iced::Background::Color(palette::QUOTE_BG)),
                                border: iced::Border {
                                    color: palette::QUOTE_BORDER,
                                    width: 0.0,
                                    radius: 2.0.into(),
                                },
                                ..Default::default()
                            },
                        );
                        col = col.push(quote);
                    }
                    col = col.push(body_widget);
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
                        background: Some(iced::Background::Color(palette::MENTION_HIGHLIGHT)),
                        ..Default::default()
                    })
                    .into()
            } else {
                row_elem
            };

            // M6: wrap message + react_row together in a single mouse_area so that
            // moving the cursor into the reaction bar doesn't trigger on_exit from
            // the message, which would cause a flicker loop.
            let msg_id_for_hover = m.id.clone();
            let combined = column![row_elem, react_row].spacing(0);
            let hover_area = mouse_area(combined)
                .on_enter(Message::SetHoveredMessage(Some(msg_id_for_hover.clone())))
                .on_exit(Message::SetHoveredMessage(None));
            rows.push(hover_area.into());

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
                rows.push(domain_label(&preview.url));
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
                    palette::BRAND_BLUE
                } else {
                    palette::MUTED_TEXT
                };
                text(label)
                    .size(12)
                    .color(color)
                    .shaping(Shaping::Advanced)
                    .into()
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
            let truncated: String = preview.chars().take(100).collect();
            let cancel_btn = button(text("✕").size(10).shaping(Shaping::Advanced))
                .on_press(Message::CancelReply)
                .padding([2, 4]);
            let strip = row![
                text(format!("Replying: {}", truncated))
                    .size(11)
                    .width(Length::Fill),
                cancel_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .padding([4, 8]);
            container(strip)
                .width(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(palette::SURFACE)),
                    border: iced::Border {
                        color: palette::QUOTE_BORDER,
                        width: 1.0,
                        radius: 2.0.into(),
                    },
                    ..Default::default()
                })
                .into()
        });

        // E1: edit-mode strip above composer
        let edit_strip: Option<Element<Message>> = self.edit_mode.as_ref().map(|(_id, _orig)| {
            let cancel_btn = button(text("✕").size(10))
                .on_press(Message::CancelEdit)
                .padding([2, 4]);
            let strip = row![
                text("Ed: Editing message").size(11).width(Length::Fill),
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
                    button(text("X Dismiss").size(10))
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

        let emoji_btn = button(text("☺").size(18).shaping(Shaping::Advanced))
            .on_press(Message::EmojiPickerToggled)
            .padding([6, 8]);

        // E4/I3: paperclip button for file picker
        let attach_btn = tooltip(
            button(text("⊕").size(18).shaping(Shaping::Advanced))
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
                    button(text("♪").size(18).shaping(Shaping::Advanced))
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
                let elapsed_str = format!("REC {}:{:02}", mins, secs);
                row![
                    button(text("X Cancel").size(13))
                        .on_press(Message::CancelRecording)
                        .padding([8, 12]),
                    text(elapsed_str).size(14).width(Length::Fill),
                    button(text("[] Stop").size(13))
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
                            background: Some(iced::Background::Color(palette::SUCCESS_GREEN)),
                            ..Default::default()
                        }),
                )
                .width(200)
                .height(4)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(palette::PROGRESS_TRACK)),
                    ..Default::default()
                });
                // DC-17: show thumbnail preview if available
                if let Some(ref thumb_bytes) = att.thumbnail {
                    let handle = iced_image::Handle::from_bytes(thumb_bytes.clone());
                    att_col = att_col.push(iced::widget::image(handle).width(64).height(64));
                }
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
            button(text("✕").size(14))
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
            button(text("?").size(14))
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
            let mut header_row = row![
                text(format!("Chat with {}", self.peer_jid))
                    .size(14)
                    .width(Length::Fill),
                block_btn,
                mute_btn,
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            // OMEMO Phase 2: show per-conversation lock button only when OMEMO is globally enabled
            if vctx.omemo_enabled {
                let lock_icon = if self.is_encryption_enabled {
                    "🔒"
                } else {
                    "🔓"
                };
                let lock_tip = if self.is_encryption_enabled {
                    "Encryption enabled — click to disable"
                } else {
                    "Encryption disabled — click to enable"
                };
                let lock_btn = tooltip(
                    button(text(lock_icon).size(14).shaping(Shaping::Advanced))
                        .on_press(Message::ToggleEncryption)
                        .padding([4, 8]),
                    lock_tip,
                    tooltip::Position::Bottom,
                );
                header_row = header_row.push(lock_btn);
            }
            header_row = header_row.push(search_btn).push(close_btn);
            header_row.into()
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
                        button("Cancel")
                            .on_press(Message::DismissModerateDialog)
                            .padding([6, 12]),
                        button(text("Moderate").color(palette::MODERATE_RED))
                            .on_press(Message::SubmitModerate)
                            .padding([6, 12]),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                ]
                .spacing(12),
            )
            .padding(20)
            .style(|_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(palette::SURFACE)),
                border: iced::Border {
                    color: palette::BORDER_SUBTLE,
                    width: 1.0,
                    radius: 2.0.into(),
                },
                shadow: iced::Shadow {
                    color: palette::BACKDROP_DIM,
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
                        background: Some(iced::Background::Color(palette::BACKDROP_DARK)),
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
                SpanStyle::Code => t.font(Font::MONOSPACE).color(palette::CODE_GREEN),
                SpanStyle::Strike => t.strikethrough(true),
                SpanStyle::Quote => t.color(palette::QUOTE_TEXT).font(Font {
                    style: font::Style::Italic,
                    ..Font::DEFAULT
                }),
                SpanStyle::Link => t.color(palette::LINK_BLUE),
            }
        })
        .collect();
    rich_text(iced_spans).size(14).into()
}
