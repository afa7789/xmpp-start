// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use std::collections::HashMap;
use std::sync::Arc;

use iced::{Element, Subscription, Task};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

pub mod about;
pub mod avatar;
pub mod benchmark;
pub mod chat;
pub mod conversation;
pub mod data_forms;
mod login;
pub mod muc_panel;
pub mod settings;
pub mod sidebar;
pub mod styling;
pub mod toast;

pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;
pub use login::LoginScreen;

use crate::config::{self, Settings, Theme};
use crate::xmpp::{self, modules::presence_machine::PresenceStatus, XmppCommand, XmppEvent};
use toast::{Toast, ToastKind};

// F2: hardcoded command palette entries
const PALETTE_COMMANDS: &[&str] = &[
    "Open Settings",
    "Open About",
    "Toggle Console",
    "Add Contact",
    "Disconnect",
];

/// Top-level application state.
pub struct App {
    screen: Screen,
    xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
    settings: Settings,
    db: Arc<SqlitePool>,
    // J1: toast notifications
    toasts: Vec<Toast>,
    next_toast_id: u64,
    // F4: reconnect state
    reconnect_attempt: u32,
    last_connect_cfg: Option<crate::xmpp::ConnectConfig>,
    // H1: avatar cache (jid → png bytes)
    avatar_cache: HashMap<String, Vec<u8>>,
    // F1: debug console entries (direction, xml) and visibility flag
    console_entries: Vec<(String, String)>,
    show_console: bool,
    // F2: command palette
    show_palette: bool,
    palette_query: String,
    // E4: pending upload (target_jid, file_path) — set when RequestUploadSlot is sent
    pending_upload: Option<(String, std::path::PathBuf)>,
    // O2: own presence — skip notifications when DND
    own_presence: PresenceStatus,
    // S1: idle state tracking
    last_activity: std::time::Instant,
    idle_state: IdleState,
    // J10: MAM archiving default mode ("roster", "always", or "never")
    mam_default_mode: Option<String>,
}

/// S1: tracks which auto-away stage has been sent to the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleState {
    Active,
    AutoAway,
    AutoXa,
}

#[derive(Debug, Clone)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    Chat(chat::Message),
    GoToBenchmark,
    #[allow(dead_code)]
    ToggleTheme,
    XmppReady(mpsc::Sender<XmppCommand>),
    XmppEvent(XmppEvent),
    // J1: toast messages
    ShowToast(String, ToastKind),
    DismissToast(u64),
    // B4: messages loaded from DB for a conversation
    MessagesLoaded(String, Vec<crate::store::message_repo::Message>),
    // F3: settings panel
    GoToSettings,
    Settings(settings::Message),
    GoBack,
    // F1: toggle the XMPP debug console panel
    ToggleConsole,
    // F2: command palette
    TogglePalette,
    PaletteQuery(String),
    PaletteExecute(usize),
    // I2: file(s) dropped onto window
    FilesDropped(Vec<std::path::PathBuf>),
    // I1: paste from clipboard triggered
    PasteFromClipboard,
    // AUTH-2: logout button — disconnect and return to login screen
    Logout,
    // S1: periodic idle timer tick — checked to trigger auto-away
    IdleTick,
    // M7: about modal
    GoToAbout,
    About(about::Message),
}

enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    Chat(Box<ChatScreen>),
    Settings(Box<settings::SettingsScreen>, Box<Screen>),
    About(Box<about::AboutScreen>),
}

impl App {
    pub fn new_with_settings(settings: Settings, db: Arc<SqlitePool>) -> (Self, Task<Message>) {
        let mam_mode = settings.mam_default_mode.clone();
        let password = config::load_password(&settings.last_jid).unwrap_or_default();
        let login = LoginScreen::with_saved(
            settings.last_jid.clone(),
            password,
            settings.last_server.clone(),
        );
        (
            App {
                screen: Screen::Login(login),
                xmpp_tx: None,
                settings,
                db,
                toasts: vec![],
                next_toast_id: 0,
                reconnect_attempt: 0,
                last_connect_cfg: None,
                avatar_cache: HashMap::new(),
                console_entries: vec![],
                show_console: false,
                show_palette: false,
                palette_query: String::new(),
                pending_upload: None,
                own_presence: PresenceStatus::Available,
                last_activity: std::time::Instant::now(),
                idle_state: IdleState::Active,
                mam_default_mode: mam_mode,
            },
            Task::none(),
        )
    }

    pub fn iced_theme(&self) -> iced::Theme {
        match self.settings.theme {
            Theme::Dark => iced::Theme::Dark,
            Theme::Light => iced::Theme::Light,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // F2: command palette
            Message::TogglePalette => {
                self.show_palette = !self.show_palette;
                self.palette_query.clear();
                Task::none()
            }

            Message::PaletteQuery(q) => {
                self.palette_query = q;
                Task::none()
            }

            Message::PaletteExecute(i) => {
                self.show_palette = false;
                // S1: activity resets idle timer
                self.last_activity = std::time::Instant::now();
                if self.idle_state != IdleState::Active {
                    self.idle_state = IdleState::Active;
                    if let Some(ref tx) = self.xmpp_tx {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(XmppCommand::UserActive).await;
                        });
                    }
                }
                let filtered: Vec<&str> = PALETTE_COMMANDS
                    .iter()
                    .copied()
                    .filter(|cmd| {
                        cmd.to_lowercase()
                            .contains(&self.palette_query.to_lowercase())
                    })
                    .collect();
                if let Some(&label) = filtered.get(i) {
                    match label {
                        "Open Settings" => return self.update(Message::GoToSettings),
                        "Open About" => return self.update(Message::GoToAbout),
                        "Disconnect" => {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(crate::xmpp::XmppCommand::Disconnect).await;
                                });
                            }
                        }
                        _ => {}
                    }
                }
                self.palette_query.clear();
                Task::none()
            }

            Message::IdleTick => {
                let elapsed = self.last_activity.elapsed().as_secs();
                const IDLE_SECS: u64 = 300;
                const EXTENDED_SECS: u64 = 900;

                match self.idle_state {
                    IdleState::Active if elapsed >= EXTENDED_SECS => {
                        self.idle_state = IdleState::AutoXa;
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::UserExtendedIdle).await;
                            });
                        }
                    }
                    IdleState::Active if elapsed >= IDLE_SECS => {
                        self.idle_state = IdleState::AutoAway;
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::UserIdle).await;
                            });
                        }
                    }
                    IdleState::AutoAway if elapsed >= EXTENDED_SECS => {
                        self.idle_state = IdleState::AutoXa;
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::UserExtendedIdle).await;
                            });
                        }
                    }
                    _ => {}
                }
                Task::none()
            }

            // I2: route dropped files to active conversation
            Message::FilesDropped(paths) => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    if let Some(jid) = chat.active_jid().map(str::to_owned) {
                        return chat
                            .update(chat::Message::Conversation(
                                jid.clone(),
                                conversation::Message::FilesDropped(paths),
                            ))
                            .map(Message::Chat);
                    }
                }
                Task::none()
            }

            // I1: route clipboard paste to active conversation
            Message::PasteFromClipboard => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    if let Some(jid) = chat.active_jid().map(str::to_owned) {
                        return chat
                            .update(chat::Message::Conversation(
                                jid.clone(),
                                conversation::Message::PasteFromClipboard,
                            ))
                            .map(Message::Chat);
                    }
                }
                Task::none()
            }

            Message::ShowToast(body, kind) => {
                let id = self.next_toast_id;
                self.next_toast_id += 1;
                self.toasts.push(Toast { id, body, kind });
                // J1: auto-dismiss after 3 seconds
                Task::future(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    Message::DismissToast(id)
                })
            }

            Message::DismissToast(id) => {
                self.toasts.retain(|t| t.id != id);
                Task::none()
            }

            Message::MessagesLoaded(jid, rows) => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    let own_jid = chat.own_jid().to_string();
                    if let Some(convo) = chat.get_conversation_mut(&jid) {
                        let display: Vec<crate::ui::conversation::DisplayMessage> = rows
                            .into_iter()
                            .map(|r| crate::ui::conversation::DisplayMessage {
                                id: r.id,
                                from: r.from_jid.clone(),
                                body: r.body.unwrap_or_default(),
                                own: r.from_jid == own_jid,
                                timestamp: r.timestamp,
                                reply_preview: None,
                                edited: r.edited_body.is_some(),
                                retracted: r.retracted != 0,
                            })
                            .collect();
                        convo.load_history(display);
                        return convo
                            .update(crate::ui::conversation::Message::ScrollToBottom)
                            .map(move |m| {
                                Message::Chat(chat::Message::Conversation(jid.clone(), m))
                            });
                    }
                }
                Task::none()
            }

            Message::GoToSettings => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                self.screen = Screen::Settings(
                    Box::new(settings::SettingsScreen::new(self.settings.clone())),
                    Box::new(prev),
                );
                Task::none()
            }

            Message::GoToAbout => {
                self.screen = Screen::About(Box::new(about::AboutScreen::new()));
                Task::none()
            }

            Message::About(msg) => {
                if let Screen::About(ref mut about) = self.screen {
                    about.update(msg);
                    if let about::Message::Back = msg {
                        // For now, go back to the last active chat or login
                        if let Screen::Chat(_) = &self.screen {
                            // Already on chat, do nothing
                        } else {
                            self.screen = Screen::Chat(Box::new(ChatScreen::new(
                                "user@server".to_string(), // placeholder until connected
                            )));
                        }
                    }
                }
                Task::none()
            }

            Message::GoBack => {
                if let Screen::Settings(ref ss, _) = self.screen {
                    self.settings = ss.settings().clone();
                }
                if let Screen::Settings(_, prev) =
                    std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                {
                    self.screen = *prev;
                }
                Task::none()
            }

            Message::ToggleConsole => {
                self.show_console = !self.show_console;
                Task::none()
            }

            Message::Settings(smsg) => {
                let go_back = matches!(smsg, settings::Message::Back);
                if let Screen::Settings(ref mut ss, _) = self.screen {
                    let _ = ss.update(smsg);
                    self.settings = ss.settings().clone();
                }
                if go_back {
                    return self.update(Message::GoBack);
                }
                Task::none()
            }

            Message::ToggleTheme => {
                self.settings.theme = match self.settings.theme {
                    Theme::Dark => Theme::Light,
                    Theme::Light => Theme::Dark,
                };
                let _ = config::save(&self.settings);
                Task::none()
            }

            Message::GoToBenchmark => {
                self.screen = Screen::Benchmark(BenchmarkScreen::new());
                Task::none()
            }

            Message::Login(msg) => {
                if matches!(msg, login::Message::Connect) {
                    if let Screen::Login(ref mut login) = self.screen {
                        let cfg = login.connect_config();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            self.settings.last_jid = cfg.jid.clone();
                            self.settings.last_server = cfg.server.clone();
                            let _ = config::save(&self.settings);
                            self.last_connect_cfg = Some(cfg.clone());
                            self.reconnect_attempt = 0;
                            return Task::future(async move {
                                let _ = tx.send(XmppCommand::Connect(cfg)).await;
                                Message::Login(login::Message::Connecting)
                            });
                        }
                    }
                }

                if matches!(msg, login::Message::GoToBenchmark) {
                    self.screen = Screen::Benchmark(BenchmarkScreen::new());
                    return Task::none();
                }

                if let Screen::Login(login) = &mut self.screen {
                    login.update(msg).map(Message::Login)
                } else {
                    Task::none()
                }
            }

            Message::Benchmark(msg) => {
                if let Screen::Benchmark(bench) = &mut self.screen {
                    let go_back = matches!(msg, benchmark::Message::Back);
                    let task = bench.update(msg).map(Message::Benchmark);
                    if go_back {
                        self.screen = Screen::Login(LoginScreen::new());
                    }
                    task
                } else {
                    Task::none()
                }
            }

            Message::Logout => {
                // AUTH-2: disconnect, clear keychain if !remember_me, return to login screen
                if !self.settings.remember_me && !self.settings.last_jid.is_empty() {
                    config::delete_password(&self.settings.last_jid);
                }
                // Reset own presence
                self.own_presence = PresenceStatus::Available;
                let login = LoginScreen::with_saved(
                    self.settings.last_jid.clone(),
                    String::new(),
                    self.settings.last_server.clone(),
                );
                self.screen = Screen::Login(login);
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(XmppCommand::Disconnect).await;
                    });
                }
                Task::none()
            }

            Message::Chat(msg) => {
                // S1: any user interaction resets the idle timer
                self.last_activity = std::time::Instant::now();
                if self.idle_state != IdleState::Active {
                    self.idle_state = IdleState::Active;
                    if let Some(ref tx) = self.xmpp_tx {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(XmppCommand::UserActive).await;
                        });
                    }
                }
                // F3: intercept OpenSettings before delegating
                if let chat::Message::OpenSettings = msg {
                    return self.update(Message::GoToSettings);
                }
                // O2: intercept SetPresence to track own presence for DND notification suppression
                if let chat::Message::SetPresence(ref status) = msg {
                    self.own_presence = status.clone();
                }
                // J3: intercept ToggleMute to persist muted_jids
                if let chat::Message::ToggleMute(ref jid) = msg {
                    let jid_str = jid.clone();
                    if self.settings.muted_jids.contains(&jid_str) {
                        self.settings.muted_jids.remove(&jid_str);
                    } else {
                        self.settings.muted_jids.insert(jid_str);
                    }
                    let _ = config::save(&self.settings);
                }
                if let Screen::Chat(ref mut chat) = self.screen {
                    // B4+B6: if SelectContact, fire history load and mark-read
                    let selected_jid: Option<String> = if let chat::Message::Sidebar(
                        crate::ui::sidebar::Message::SelectContact(ref jid),
                    ) = msg
                    {
                        Some(jid.clone())
                    } else {
                        None
                    };
                    let history_task: Task<Message> = if let Some(ref jid) = selected_jid {
                        let jid = jid.clone();
                        let pool = self.db.clone();
                        Task::future(async move {
                            let rows =
                                crate::store::message_repo::find_by_conversation(&pool, &jid, 50)
                                    .await
                                    .unwrap_or_default();
                            Message::MessagesLoaded(jid, rows)
                        })
                    } else {
                        Task::none()
                    };
                    let mark_read_task: Task<Message> = if let Some(ref jid) = selected_jid {
                        if let Some(last_id) = chat.last_message_id(jid) {
                            let jid = jid.clone();
                            let pool = self.db.clone();
                            tokio::spawn(async move {
                                let _ = crate::store::conversation_repo::mark_read(
                                    &pool, &jid, &last_id,
                                )
                                .await;
                            });
                            Task::none()
                        } else {
                            Task::none()
                        }
                    } else {
                        Task::none()
                    };
                    let task = chat.update(msg).map(Message::Chat);
                    // E4: capture pending upload targets before draining commands
                    let upload_targets = chat.drain_upload_targets();
                    if !upload_targets.is_empty() {
                        // Store the first target (FIFO queue)
                        if self.pending_upload.is_none() {
                            self.pending_upload = upload_targets.into_iter().next();
                        }
                    }
                    let cmds = chat.drain_commands();
                    if !cmds.is_empty() {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                for cmd in cmds {
                                    let _ = tx.send(cmd).await;
                                }
                            });
                        }
                    }
                    Task::batch([history_task, mark_read_task, task])
                } else {
                    Task::none()
                }
            }

            Message::XmppReady(tx) => {
                tracing::debug!("xmpp command channel ready");
                self.xmpp_tx = Some(tx);
                Task::none()
            }

            Message::XmppEvent(event) => {
                match event {
                    XmppEvent::Connected { ref bound_jid } => {
                        tracing::info!("XMPP: online as {bound_jid}");
                        if let Screen::Login(ref login) = self.screen {
                            let cfg = login.connect_config();
                            if !cfg.password.is_empty() {
                                let _ = config::save_password(&cfg.jid, &cfg.password);
                            }
                        }
                        self.screen = Screen::Chat(Box::new(ChatScreen::new(bound_jid.clone())));
                        // A3: pre-populate sidebar from cached DB roster before server responds
                        let pool = self.db.clone();
                        let roster_prefill = Task::future(async move {
                            let contacts = crate::store::roster_repo::get_all(&pool)
                                .await
                                .unwrap_or_default();
                            let xmpp_contacts: Vec<crate::xmpp::RosterContact> = contacts
                                .into_iter()
                                .map(|c| crate::xmpp::RosterContact {
                                    jid: c.jid,
                                    name: c.name,
                                    subscription: c.subscription,
                                })
                                .collect();
                            Message::XmppEvent(XmppEvent::RosterReceived(xmpp_contacts))
                        });
                        let toast = self.update(Message::ShowToast(
                            format!("Connected as {}", bound_jid),
                            ToastKind::Success,
                        ));
                        return Task::batch([roster_prefill, toast]);
                    }
                    XmppEvent::Disconnected { ref reason } => {
                        tracing::warn!("XMPP: disconnected — {reason}");
                        if let Screen::Login(ref mut login) = self.screen {
                            login.on_error(reason.clone());
                        }
                        if matches!(self.screen, Screen::Chat(_)) {
                            self.screen = Screen::Login(LoginScreen::new());
                        }
                        // J1: show disconnect toast
                        let msg = Message::ShowToast(
                            format!("Disconnected: {}", reason),
                            ToastKind::Error,
                        );
                        return self.update(msg);
                    }
                    XmppEvent::Reconnecting { attempt } => {
                        tracing::info!("XMPP: reconnecting (attempt {attempt})");
                        self.reconnect_attempt = attempt;
                        let delay_secs = 2u64.pow(attempt.min(6));
                        if let (Some(cfg), Some(tx)) =
                            (self.last_connect_cfg.clone(), self.xmpp_tx.clone())
                        {
                            return Task::future(async move {
                                tokio::time::sleep(std::time::Duration::from_secs(delay_secs))
                                    .await;
                                let _ = tx.send(XmppCommand::Connect(cfg)).await;
                                Message::XmppEvent(XmppEvent::Reconnecting { attempt: 0 })
                            })
                            .discard();
                        }
                    }
                    XmppEvent::RosterReceived(ref contacts) => {
                        tracing::info!("XMPP: roster ({} contacts)", contacts.len());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.set_roster(contacts.clone());
                        }
                        let toast = self.update(Message::ShowToast(
                            format!("{} contacts loaded", contacts.len()),
                            ToastKind::Info,
                        ));
                        // A3: persist roster to DB
                        let pool = self.db.clone();
                        let contacts = contacts.clone();
                        // H1: fetch avatars for all roster contacts (fire-and-forget)
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            let jids: Vec<String> =
                                contacts.iter().map(|c| c.jid.clone()).collect();
                            tokio::spawn(async move {
                                for jid in jids {
                                    let _ = tx.send(XmppCommand::FetchAvatar(jid)).await;
                                }
                            });
                        }
                        // A3: persist roster to DB (fire-and-forget)
                        tokio::spawn(async move {
                            for c in &contacts {
                                let _ = crate::store::roster_repo::upsert(
                                    &pool,
                                    &crate::store::roster_repo::RosterContact {
                                        jid: c.jid.clone(),
                                        name: c.name.clone(),
                                        subscription: c.subscription.clone(),
                                        groups: None,
                                    },
                                )
                                .await;
                            }
                        });
                        return toast;
                    }
                    XmppEvent::MessageReceived(ref msg) => {
                        tracing::info!("XMPP: message from {}", msg.from);
                        let bare_from = msg.from.split('/').next().unwrap_or(&msg.from).to_string();
                        // A5: desktop notification — only for background conversations (J3: skip muted JIDs)
                        let is_active = if let Screen::Chat(ref chat) = self.screen {
                            chat.active_jid() == Some(bare_from.as_str())
                        } else {
                            false
                        };
                        // A5: fire desktop notification for background conversations (J3: skip muted, BUG-1: skip historical, O2: skip if DND)
                        let notif_task: Task<Message> = if self.settings.notifications_enabled
                            && !is_active
                            && !msg.is_historical
                            && !self.settings.muted_jids.contains(&bare_from)
                            && self.own_presence != PresenceStatus::DoNotDisturb
                        {
                            let notif_from = bare_from.clone();
                            let notif_body: String = msg.body.chars().take(100).collect();
                            tokio::spawn(async move {
                                let _ =
                                    crate::notifications::notify_message(&notif_from, &notif_body);
                            });
                            Task::none()
                        } else {
                            Task::none()
                        };
                        // A2: persist message + conversation to DB (fire-and-forget)
                        let pool = self.db.clone();
                        let from_jid = msg.from.clone();
                        let bare_jid = from_jid.split('/').next().unwrap_or(&from_jid).to_string();
                        let msg_id = msg.id.clone();
                        let body = msg.body.clone();
                        let ts = chrono::Utc::now().timestamp_millis();
                        tokio::spawn(async move {
                            let _ = crate::store::conversation_repo::upsert(&pool, &bare_jid).await;
                            let _ = crate::store::message_repo::insert(
                                &pool,
                                &crate::store::message_repo::Message {
                                    id: msg_id,
                                    conversation_jid: bare_jid,
                                    from_jid,
                                    body: Some(body),
                                    timestamp: ts,
                                    stanza_id: None,
                                    origin_id: None,
                                    state: "received".into(),
                                    edited_body: None,
                                    retracted: 0,
                                },
                            )
                            .await;
                        });

                        if let Screen::Chat(ref mut chat) = self.screen {
                            if let Some(preview_task) = chat.on_message_received(msg.clone()) {
                                return Task::batch([notif_task, preview_task.map(Message::Chat)]);
                            }
                            return notif_task;
                        }
                        return notif_task;
                    }
                    XmppEvent::PresenceUpdated { ref jid, available } => {
                        tracing::debug!("XMPP: presence {jid} available={available}");
                        // A4: forward to sidebar
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_presence(jid, available);
                        }
                    }
                    XmppEvent::PeerTyping { ref jid, composing } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let _ = chat.update(chat::Message::PeerTyping(jid.clone(), composing));
                        }
                    }
                    XmppEvent::AvatarReceived {
                        ref jid,
                        ref png_bytes,
                    } => {
                        tracing::debug!(
                            "H1: avatar received for {jid} ({} bytes)",
                            png_bytes.len()
                        );
                        self.avatar_cache.insert(jid.clone(), png_bytes.clone());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_avatar_received(jid.clone(), png_bytes.clone());
                        }
                    }
                    XmppEvent::CatchupFinished {
                        ref conversation_jid,
                        fetched,
                    } => {
                        tracing::info!(
                            "XMPP: MAM catchup complete for {conversation_jid} ({fetched} messages)"
                        );
                    }
                    XmppEvent::UploadSlotReceived {
                        put_url,
                        get_url,
                        headers,
                    } => {
                        tracing::info!("E4: upload slot received put={put_url} get={get_url}");
                        // E4: perform HTTP PUT and send get_url as message (fire-and-forget)
                        if let Some((target_jid, file_path)) = self.pending_upload.take() {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    let file_bytes = tokio::fs::read(&file_path).await;
                                    match file_bytes {
                                        Ok(bytes) => {
                                            let client = reqwest::Client::new();
                                            let mut req = client.put(&put_url).body(bytes);
                                            for (k, v) in &headers {
                                                req = req.header(k.as_str(), v.as_str());
                                            }
                                            match req.send().await {
                                                Ok(resp) if resp.status().is_success() => {
                                                    let _ = tx
                                                        .send(XmppCommand::SendMessage {
                                                            to: target_jid,
                                                            body: get_url,
                                                        })
                                                        .await;
                                                }
                                                Ok(resp) => {
                                                    tracing::warn!(
                                                        "E4: PUT failed: {}",
                                                        resp.status()
                                                    );
                                                }
                                                Err(e) => {
                                                    tracing::warn!("E4: PUT error: {e}");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "E4: failed to read file {:?}: {e}",
                                                file_path
                                            );
                                        }
                                    }
                                });
                            }
                        }
                    }
                    XmppEvent::ConsoleEntry { direction, xml } => {
                        self.console_entries.push((direction, xml));
                        if self.console_entries.len() > 200 {
                            self.console_entries.remove(0);
                        }
                    }
                    XmppEvent::ReactionReceived {
                        ref msg_id,
                        ref from,
                        ref emojis,
                    } => {
                        tracing::debug!("E3: reaction from {from} on {msg_id}: {:?}", emojis);
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_reaction_received(msg_id.clone(), from.clone(), emojis.clone());
                        }
                    }
                    XmppEvent::VCardReceived { jid, name, .. } => {
                        tracing::debug!("H4: vCard received for {jid}: name={:?}", name);
                    }
                    // J6: XEP-0084 PubSub avatar — store alongside vCard avatar
                    XmppEvent::AvatarUpdated { ref jid, ref data } => {
                        tracing::debug!(
                            "J6: PubSub avatar updated for {jid} ({} bytes)",
                            data.len()
                        );
                        self.avatar_cache.insert(jid.clone(), data.clone());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_avatar_received(jid.clone(), data.clone());
                        }
                    }
                    // K4: delivery receipt — update message state in conversation
                    XmppEvent::MessageDelivered { ref id, ref from } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_delivered(from, id.clone());
                        }
                    }
                    // K5: read marker — update message state in conversation
                    XmppEvent::MessageRead { ref id, ref from } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_read(from, id.clone());
                        }
                    }
                    // J10: MAM prefs received — persist to settings and update UI state
                    XmppEvent::MamPrefsReceived { ref default_mode } => {
                        tracing::debug!("J10: MAM prefs default_mode={default_mode}");
                        self.mam_default_mode = Some(default_mode.clone());
                        self.settings.mam_default_mode = Some(default_mode.clone());
                        let _ = config::save(&self.settings);
                    }
                    // K1: room config form received from server
                    XmppEvent::RoomConfigFormReceived { room_jid, config } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            return chat
                                .update(chat::Message::RoomConfigFormReceived(room_jid, config))
                                .map(Message::Chat);
                        }
                    }
                    // K1: room configuration accepted — room is now live
                    XmppEvent::RoomConfigured { room_jid } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            return chat
                                .update(chat::Message::RoomConfigured(room_jid))
                                .map(Message::Chat);
                        }
                    }
                    // K3: incoming room invitation received
                    XmppEvent::RoomInvitationReceived {
                        room_jid,
                        from_jid,
                        reason,
                    } => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let _ = chat.update(chat::Message::RoomInvitationReceived {
                                room_jid: room_jid.clone(),
                                from_jid: from_jid.clone(),
                                reason: reason.clone(),
                            });
                        }
                        let body = format!("{} invited you to {}", from_jid, room_jid);
                        return self.update(Message::ShowToast(body, ToastKind::Info));
                    }
                    // K2: room list received from MUC service
                    XmppEvent::RoomListReceived(rooms) => {
                        tracing::info!("k2: {} public rooms received", rooms.len());
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let _ = chat.update(chat::Message::RoomListReceived(rooms));
                        }
                    }
                    XmppEvent::BookmarksReceived(bookmarks) => {
                        tracing::info!("D4: {} bookmark(s) received", bookmarks.len());
                        // D4: autojoin rooms — send JoinRoom for each autojoin bookmark
                        let autojoin: Vec<_> = bookmarks.iter().filter(|b| b.autojoin).collect();
                        if !autojoin.is_empty() {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                let cmds: Vec<XmppCommand> = autojoin
                                    .into_iter()
                                    .map(|b| XmppCommand::JoinRoom {
                                        jid: b.jid.clone(),
                                        nick: b.nick.clone().unwrap_or_else(|| {
                                            // Default nick: local part of our JID
                                            "me".to_string()
                                        }),
                                    })
                                    .collect();
                                tokio::spawn(async move {
                                    for cmd in cmds {
                                        let _ = tx.send(cmd).await;
                                    }
                                });
                            }
                        }
                    }
                    // L3: XEP-0425 — message was moderated in a MUC room
                    XmppEvent::MessageModerated {
                        ref room_jid,
                        ref message_id,
                    } => {
                        tracing::info!("muc: message {} moderated in {}", message_id, room_jid);
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.on_message_moderated(room_jid, message_id);
                        }
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        use iced::widget::{
            button, column, container, row, scrollable, stack, text, text_input, Space,
        };
        use iced::{Alignment, Color, Length};

        let screen_view: Element<Message> = match &self.screen {
            Screen::Login(login) => login.view().map(Message::Login),
            Screen::Benchmark(bench) => bench.view().map(Message::Benchmark),
            Screen::Chat(chat) => chat.view(self.settings.time_format).map(Message::Chat),
            Screen::Settings(ss, _) => ss.view().map(Message::Settings),
            Screen::About(about) => about.view().map(Message::About),
        };

        // F1: build the XML toggle button (always visible, bottom-left corner)
        let console_btn_label = if self.show_console { "XML [on]" } else { "XML" };
        let console_toggle = button(text(console_btn_label).size(11))
            .on_press(Message::ToggleConsole)
            .padding([3, 6]);
        let console_btn_overlay = container(console_toggle)
            .align_x(Alignment::Start)
            .align_y(Alignment::End)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4);

        // Build layers: screen + optional palette + optional console panel + button + optional toasts
        let mut layers: Vec<Element<Message>> = vec![screen_view];

        // F4: reconnect banner — shown at top of screen when reconnecting
        if self.reconnect_attempt > 0 {
            let banner_text = format!("Reconnecting (attempt {})…", self.reconnect_attempt);
            let banner = container(text(banner_text).size(12).color(Color::WHITE))
                .width(Length::Fill)
                .padding([4, 12])
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.8, 0.4, 0.0))),
                    ..Default::default()
                });
            let banner_overlay = container(column![banner])
                .align_x(Alignment::Center)
                .align_y(Alignment::Start)
                .width(Length::Fill)
                .height(Length::Fill);
            layers.push(banner_overlay.into());
        }

        // F2: palette overlay
        if self.show_palette {
            let filtered: Vec<(usize, &str)> = PALETTE_COMMANDS
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, cmd)| {
                    cmd.to_lowercase()
                        .contains(&self.palette_query.to_lowercase())
                })
                .collect();

            let input = text_input("Search commands...", &self.palette_query)
                .id(iced::widget::text_input::Id::new("palette_input"))
                .on_input(Message::PaletteQuery)
                .on_submit(if filtered.is_empty() {
                    Message::TogglePalette
                } else {
                    Message::PaletteExecute(0)
                })
                .padding(10)
                .size(16);

            let cmd_buttons: Vec<Element<Message>> = filtered
                .iter()
                .map(|(i, label)| {
                    button(text(*label).size(14))
                        .on_press(Message::PaletteExecute(*i))
                        .width(Length::Fill)
                        .padding([8, 12])
                        .into()
                })
                .collect();

            let cmd_list = cmd_buttons
                .into_iter()
                .fold(column![].spacing(2), iced::widget::Column::push);

            let palette_box =
                container(column![input, scrollable(cmd_list).height(300)].spacing(8))
                    .width(480)
                    .padding(16)
                    .style(|theme: &iced::Theme| {
                        let palette = theme.extended_palette();
                        iced::widget::container::Style {
                            background: Some(iced::Background::Color(
                                palette.background.base.color,
                            )),
                            border: iced::Border {
                                color: palette.primary.base.color,
                                width: 1.0,
                                radius: 8.0.into(),
                            },
                            shadow: iced::Shadow {
                                color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                                offset: iced::Vector::new(0.0, 4.0),
                                blur_radius: 16.0,
                            },
                            ..Default::default()
                        }
                    });

            // Dark semi-transparent backdrop + centered palette
            let backdrop = container(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.5,
                    ))),
                    ..Default::default()
                });

            let overlay = container(
                column![
                    Space::new(Length::Fill, Length::Fixed(80.0)),
                    row![
                        Space::new(Length::Fill, Length::Shrink),
                        palette_box,
                        Space::new(Length::Fill, Length::Shrink),
                    ]
                    .width(Length::Fill),
                ]
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill);

            let palette_layer: Element<Message> = stack![backdrop, overlay].into();
            layers.push(palette_layer);
        }

        // J1: build toast overlay
        if self.toasts.is_empty() {
            // Re-stack layers built so far
            return if layers.len() == 1 {
                layers.remove(0)
            } else {
                let base = layers.remove(0);
                let top = layers.remove(0);
                stack![base, top].into()
            };
        }

        // J1: build toast column at bottom-right
        let toast_items: Vec<Element<Message>> = self
            .toasts
            .iter()
            .map(|t| {
                let bg = match t.kind {
                    ToastKind::Error => Color::from_rgb(0.8, 0.2, 0.2),
                    ToastKind::Success => Color::from_rgb(0.2, 0.7, 0.3),
                    ToastKind::Info => Color::from_rgb(0.2, 0.4, 0.8),
                };
                let dismiss_btn = button(text("x").size(10))
                    .on_press(Message::DismissToast(t.id))
                    .padding([2, 4]);
                let toast_row = row![
                    text(t.body.clone())
                        .size(12)
                        .color(Color::WHITE)
                        .width(Length::Fill),
                    dismiss_btn,
                ]
                .spacing(4)
                .align_y(Alignment::Center);
                container(toast_row)
                    .padding([6, 10])
                    .width(280)
                    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(bg)),
                        ..Default::default()
                    })
                    .into()
            })
            .collect();

        let toast_col = toast_items
            .into_iter()
            .fold(column![].spacing(4), iced::widget::Column::push);

        let toast_overlay = container(toast_col)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(4);

        // F1: console panel overlay (bottom-left, above console button)
        if self.show_console {
            let entry_rows: Vec<Element<Message>> = self
                .console_entries
                .iter()
                .map(|(dir, xml)| {
                    let prefix = if dir == "sent" { "[sent]" } else { "[recv]" };
                    let snippet: String = xml.chars().take(120).collect();
                    let line = format!("{prefix} {snippet}");
                    text(line).size(10).font(iced::Font::MONOSPACE).into()
                })
                .collect();

            let entries_col = entry_rows
                .into_iter()
                .fold(column![].spacing(1), iced::widget::Column::push);

            let scroll = scrollable(entries_col)
                .height(Length::Fill)
                .width(Length::Fill);

            let panel = container(scroll)
                .height(300)
                .width(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.85,
                    ))),
                    ..Default::default()
                })
                .padding([4, 8]);

            let panel_overlay = container(panel)
                .align_x(Alignment::Start)
                .align_y(Alignment::End)
                .width(Length::Fill)
                .height(Length::Fill);

            layers.push(panel_overlay.into());
        }

        // F1: console button always visible
        layers.push(console_btn_overlay.into());

        // J1: toast overlay
        layers.push(toast_overlay.into());

        stack(layers).into()
    }

    pub fn subscription() -> Subscription<Message> {
        let xmpp_sub = xmpp::subscription::xmpp_subscription();
        // S1: periodic idle tick — fires every 30s so App can check auto-away conditions
        let idle_sub =
            iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::IdleTick);
        // F2: keyboard shortcut — Cmd+K / Ctrl+K to toggle palette, Escape to close
        // I1: Cmd+V / Ctrl+V to paste from clipboard
        let kb_sub = iced::keyboard::on_key_press(|key, modifiers| {
            use iced::keyboard::Key;
            if modifiers.command() {
                if key == Key::Character("k".into()) {
                    return Some(Message::TogglePalette);
                }
                if key == Key::Character("v".into()) {
                    return Some(Message::PasteFromClipboard);
                }
            }
            if key == Key::Named(iced::keyboard::key::Named::Escape) {
                return Some(Message::TogglePalette);
            }
            None
        });
        // I2: file drop subscription
        let drop_sub = iced::event::listen_with(|event, _status, _id| {
            use iced::Event;
            if let Event::Window(iced::window::Event::FileDropped(path)) = event {
                return Some(Message::FilesDropped(vec![path]));
            }
            None
        });
        // M4: periodic voice tick — fires every second to update the elapsed timer
        let voice_tick_sub = iced::time::every(std::time::Duration::from_secs(1))
            .map(|_| Message::Chat(chat::Message::VoiceTick));
        Subscription::batch([xmpp_sub, kb_sub, drop_sub, idle_sub, voice_tick_sub])
    }
}
