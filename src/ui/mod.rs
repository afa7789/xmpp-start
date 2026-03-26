// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::{Element, Subscription, Task};
use sqlx::SqlitePool;
use tokio::sync::mpsc;

pub mod about;
pub mod account_details;
pub mod account_state;
pub mod account_switcher;
pub mod adhoc;
pub mod avatar;
pub mod benchmark;
pub mod blocklist;
pub mod chat;
pub mod conversation;
pub mod data_forms;
pub mod link_preview;
mod login;
pub mod muc_panel;
pub(crate) mod navigation;
pub mod omemo_trust;
pub mod palette;
pub mod settings;
pub mod sidebar;
pub mod spam_report;
pub mod styles;
pub mod styling;
pub(crate) mod subscriptions;
pub mod toast;
pub mod vcard_editor;
pub(crate) mod xmpp_events;

pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;
pub use login::LoginScreen;

/// Shared view parameters passed down the view chain to avoid threading
/// individual fields through every `view()` signature.
#[allow(dead_code)]
pub struct ViewContext<'a> {
    pub avatars: &'a HashMap<String, Vec<u8>>,
    pub time_format: crate::config::TimeFormat,
    pub own_jid: &'a str,
    pub omemo_enabled: bool,
}

use crate::config::{self, Settings, Theme};
use crate::xmpp::multi_engine::MultiEngineManager;
use crate::xmpp::{
    self, modules::command_palette, modules::console::XmppConsole,
    modules::presence_machine::PresenceStatus, modules::xmpp_uri, AccountId, XmppCommand,
    XmppEvent,
};
use account_state::AccountStateManager;
use toast::{Toast, ToastKind};

// F2: command palette entries — built once and searched via command_palette::search().
fn palette_commands() -> Vec<command_palette::Command> {
    use command_palette::Command;
    vec![
        Command {
            id: "open-settings".into(),
            label: "Open Settings".into(),
            description: "Open the settings panel".into(),
            keywords: vec!["preferences".into(), "config".into()],
        },
        Command {
            id: "open-about".into(),
            label: "Open About".into(),
            description: "Show app info".into(),
            keywords: vec!["info".into(), "version".into()],
        },
        Command {
            id: "edit-profile".into(),
            label: "Edit Profile".into(),
            description: "Edit your vCard profile".into(),
            keywords: vec!["vcard".into(), "avatar".into()],
        },
        Command {
            id: "adhoc-commands".into(),
            label: "Ad-hoc Commands".into(),
            description: "Run server ad-hoc commands (XEP-0050)".into(),
            keywords: vec!["adhoc".into(), "server".into()],
        },
        Command {
            id: "toggle-console".into(),
            label: "Toggle Console".into(),
            description: "Show or hide the XMPP debug console".into(),
            keywords: vec!["xml".into(), "debug".into(), "stanza".into()],
        },
        Command {
            id: "add-contact".into(),
            label: "Add Contact".into(),
            description: "Add a new roster contact".into(),
            keywords: vec!["roster".into(), "friend".into()],
        },
        Command {
            id: "switch-account".into(),
            label: "Switch Account".into(),
            description: "Switch to a different XMPP account".into(),
            keywords: vec!["account".into(), "multi".into()],
        },
        Command {
            id: "report-spam".into(),
            label: "Report Spam".into(),
            description: "Report a JID as a spammer".into(),
            keywords: vec!["spam".into(), "block".into()],
        },
        Command {
            id: "disconnect".into(),
            label: "Disconnect".into(),
            description: "Gracefully disconnect from the server".into(),
            keywords: vec!["logout".into(), "quit".into()],
        },
        // DC-11: open a chat / room from an xmpp: URI typed in the palette search box
        Command {
            id: "open-xmpp-uri".into(),
            label: "Open XMPP URI".into(),
            description: "Open a chat or room from an xmpp: URI".into(),
            keywords: vec!["uri".into(), "link".into(), "xmpp".into()],
        },
    ]
}

// DC-21: shared optional receiver for the multi-account event channel
type MultiEventRx =
    std::sync::Arc<std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<(AccountId, XmppEvent)>>>>;

/// Top-level application state.
pub struct App {
    pub(crate) screen: Screen,
    pub(crate) xmpp_tx: Option<mpsc::Sender<XmppCommand>>,
    pub(crate) settings: Settings,
    pub(crate) db: Arc<SqlitePool>,
    // J1: toast notifications
    toasts: Vec<Toast>,
    next_toast_id: u64,
    // F4: reconnect state
    pub(crate) reconnect_attempt: u32,
    pub(crate) last_connect_cfg: Option<crate::xmpp::ConnectConfig>,
    // H1: avatar cache (jid → png bytes)
    pub(crate) avatar_cache: HashMap<String, Vec<u8>>,
    // H1: JIDs for which a FetchAvatar command has already been sent this session
    pub(crate) avatar_fetching: HashSet<String>,
    // F1: debug console — circular buffer of raw XML stanzas and visibility flag
    pub(crate) xmpp_console: XmppConsole,
    show_console: bool,
    // F2: command palette
    show_palette: bool,
    palette_query: String,
    // E4: pending upload (target_jid, file_path) — set when RequestUploadSlot is sent
    pub(crate) pending_upload: Option<(String, std::path::PathBuf)>,
    // O2: own presence — skip notifications when DND
    pub(crate) own_presence: PresenceStatus,
    // S1: idle state tracking
    last_activity: std::time::Instant,
    idle_state: IdleState,
    // J10: MAM archiving default mode ("roster", "always", or "never")
    pub(crate) mam_default_mode: Option<String>,
    // AUTH-1: pending auto-connect config — consumed when XmppReady fires
    auto_connect_cfg: Option<crate::xmpp::ConnectConfig>,
    // L5: spam report modal — Some when open
    spam_report_modal: Option<spam_report::SpamReportModal>,
    // MULTI: per-account UI state manager
    pub(crate) account_state_mgr: AccountStateManager,
    // DC-21: multi-engine manager for additional accounts
    pub(crate) multi_engine: MultiEngineManager,
    // DC-21: tx end of the multi-account event channel
    pub(crate) multi_event_tx: tokio::sync::mpsc::Sender<(AccountId, XmppEvent)>,
    // DC-21: rx end, shared so the iced subscription can take it once
    multi_event_rx: MultiEventRx,
    // DC-21: true while navigating to Login to add a second account
    pub(crate) is_adding_account: bool,
    // MEMO: OMEMO activation state (persists across screen transitions)
    pub(crate) omemo_enabled: bool,
    pub(crate) omemo_device_id: Option<u32>,
    // MEMO: trust dialog — Some when the OMEMO trust overlay is open
    pub(crate) omemo_trust_modal: Option<omemo_trust::OmemoTrustScreen>,
    // MEMO: cached per-peer device lists received via PEP (jid -> device ids)
    pub(crate) omemo_peer_devices: HashMap<String, Vec<u32>>,
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
    // K2: vCard editor
    GoToVCardEditor,
    VCardEditor(vcard_editor::Message),
    // L4: ad-hoc commands screen
    GoToAdhoc,
    Adhoc(adhoc::Message),
    // L5: spam report modal
    OpenSpamReport(String), // jid to pre-fill
    SpamReport(spam_report::Message),
    // MULTI: account switcher screen
    GoToAccountSwitcher,
    AccountSwitcher(account_switcher::Message),
    // DC-11: handle an xmpp: deep-link URI
    HandleXmppUri(String),
    // A3: seed conversations from DB cache at connect time
    ConversationsPrefill(Vec<String>),
    // MEMO: OMEMO trust dialog messages
    OmemoTrust(omemo_trust::Message),
    // Tab focus navigation between input fields
    FocusNext,
    FocusPrevious,
}

pub(crate) enum Screen {
    Login(LoginScreen),
    Benchmark(BenchmarkScreen),
    Chat(Box<ChatScreen>),
    Settings(Box<settings::SettingsScreen>, Box<Screen>),
    About(Box<about::AboutScreen>, Box<Screen>),
    VCardEditor(Box<vcard_editor::VCardEditorScreen>, Box<Screen>),
    Adhoc(Box<adhoc::AdhocScreen>, Box<Screen>),
    AccountSwitcher(Box<account_switcher::AccountSwitcherScreen>, Box<Screen>),
}

impl App {
    pub fn new_with_settings(settings: Settings, db: Arc<SqlitePool>) -> (Self, Task<Message>) {
        let mam_mode = settings.mam_default_mode.clone();
        let password = config::load_password(&settings.last_jid).unwrap_or_default();

        // AUTH-1: auto-connect if remember_me is set and stored credentials exist.
        let auto_connect =
            settings.remember_me && !settings.last_jid.is_empty() && !password.is_empty();

        let login = LoginScreen::with_saved(
            settings.last_jid.clone(),
            password.clone(),
            settings.last_server.clone(),
            settings.remember_me,
        );

        let auto_connect_cfg = if auto_connect {
            Some(login.connect_config())
        } else {
            None
        };

        // DC-21: create the multi-account event channel
        let (multi_event_tx, multi_event_rx) =
            tokio::sync::mpsc::channel::<(AccountId, XmppEvent)>(64);
        let multi_event_rx_shared =
            std::sync::Arc::new(std::sync::Mutex::new(Some(multi_event_rx)));

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
                avatar_cache: crate::config::load_avatar_cache(),
                avatar_fetching: HashSet::new(),
                xmpp_console: XmppConsole::new(200),
                show_console: false,
                show_palette: false,
                palette_query: String::new(),
                pending_upload: None,
                own_presence: PresenceStatus::Available,
                last_activity: std::time::Instant::now(),
                idle_state: IdleState::Active,
                mam_default_mode: mam_mode,
                auto_connect_cfg,
                spam_report_modal: None,
                account_state_mgr: AccountStateManager::new(),
                multi_engine: MultiEngineManager::new(AccountId::new(String::new())),
                multi_event_tx,
                multi_event_rx: multi_event_rx_shared,
                is_adding_account: false,
                omemo_enabled: false,
                omemo_device_id: None,
                omemo_trust_modal: None,
                omemo_peer_devices: HashMap::new(),
            },
            Task::none(),
        )
    }

    pub fn iced_theme(&self) -> iced::Theme {
        // M1: if use_system_theme is set, detect OS dark/light mode
        let effective = if self.settings.use_system_theme {
            config::detect_system_theme()
        } else {
            self.settings.theme.clone()
        };
        match effective {
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
                let results = command_palette::search(&palette_commands(), &self.palette_query);
                if let Some(m) = results.get(i) {
                    match m.command.id.as_str() {
                        "open-settings" => return self.update(Message::GoToSettings),
                        "open-about" => return self.update(Message::GoToAbout),
                        "edit-profile" => return self.update(Message::GoToVCardEditor),
                        "adhoc-commands" => return self.update(Message::GoToAdhoc),
                        "toggle-console" => return self.update(Message::ToggleConsole),
                        "switch-account" => return self.update(Message::GoToAccountSwitcher),
                        "report-spam" => {
                            return self.update(Message::OpenSpamReport(String::new()))
                        }
                        "disconnect" => {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(crate::xmpp::XmppCommand::Disconnect).await;
                                });
                            }
                        }
                        // DC-11: treat the palette query text as the xmpp: URI to open.
                        "open-xmpp-uri" => {
                            let uri = self.palette_query.trim().to_string();
                            self.palette_query.clear();
                            self.show_palette = false;
                            return self.update(Message::HandleXmppUri(uri));
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
                        let action = chat.update(chat::Message::Conversation(
                            jid.clone(),
                            conversation::Message::FilesDropped(paths),
                        ));
                        return self.handle_chat_action(action);
                    }
                }
                Task::none()
            }

            // I1: route clipboard paste to active conversation
            Message::PasteFromClipboard => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    if let Some(jid) = chat.active_jid().map(str::to_owned) {
                        let action = chat.update(chat::Message::Conversation(
                            jid.clone(),
                            conversation::Message::PasteFromClipboard,
                        ));
                        return self.handle_chat_action(action);
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
                    // Set sidebar last-message preview from the most recent history row.
                    if let Some(last_body) = rows
                        .iter()
                        .rev()
                        .filter_map(|r| r.body.as_deref())
                        .find(|b| !b.trim().is_empty())
                    {
                        chat.set_sidebar_last_message(&jid, last_body);
                    }

                    let own_jid = chat.own_jid().to_string();
                    // Strip resource so bare JIDs are compared; stored from_jid may
                    // carry the resource from a previous session with a different suffix.
                    let own_bare = own_jid.split('/').next().unwrap_or(&own_jid).to_string();
                    if let Some(convo) = chat.get_conversation_mut(&jid) {
                        let display: Vec<crate::ui::conversation::DisplayMessage> = rows
                            .into_iter()
                            .map(|r| crate::ui::conversation::DisplayMessage {
                                id: r.id,
                                from: r.from_jid.clone(),
                                body: r.body.unwrap_or_default(),
                                own: r.from_jid.split('/').next().unwrap_or(&r.from_jid)
                                    == own_bare,
                                timestamp: r.timestamp,
                                reply_preview: None,
                                edited: r.edited_body.is_some(),
                                retracted: r.retracted != 0,
                                is_encrypted: false,
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

            Message::ConversationsPrefill(jids) => {
                if let Screen::Chat(ref mut chat) = self.screen {
                    chat.prefill_conversations(jids.clone());
                }
                // Auto-select the most recent conversation so the user sees
                // their last chat history immediately after restart.
                if let Some(first_jid) = jids.first() {
                    return self.update(Message::Chat(chat::Message::Sidebar(
                        crate::ui::sidebar::Message::SelectContact(first_jid.clone()),
                    )));
                }
                Task::none()
            }

            Message::GoToSettings => navigation::go_to_settings(self),

            Message::GoToAbout => navigation::go_to_about(self),

            Message::About(msg) => navigation::handle_about(self, msg),

            // K2: navigate to vCard editor
            Message::GoToVCardEditor => navigation::go_to_vcard_editor(self),

            Message::VCardEditor(msg) => {
                let action = if let Screen::VCardEditor(ref mut ve, _) = self.screen {
                    ve.update(msg)
                } else {
                    return Task::none();
                };
                self.handle_vcard_action(action)
            }

            // L4: navigate to ad-hoc commands screen
            Message::GoToAdhoc => navigation::go_to_adhoc(self),

            Message::Adhoc(msg) => {
                let action = if let Screen::Adhoc(ref mut adhoc, _) = self.screen {
                    adhoc.update(msg)
                } else {
                    return Task::none();
                };
                self.handle_adhoc_action(action)
            }

            // L5: spam report modal
            Message::OpenSpamReport(jid) => {
                self.spam_report_modal = Some(spam_report::SpamReportModal::new(jid));
                Task::none()
            }

            Message::SpamReport(msg) => {
                let action = if let Some(ref mut modal) = self.spam_report_modal {
                    modal.update(msg)
                } else {
                    spam_report::Action::None
                };
                match action {
                    spam_report::Action::None => Task::none(),
                    spam_report::Action::Cancel => {
                        self.spam_report_modal = None;
                        Task::none()
                    }
                    spam_report::Action::Submit { jid, reason } => {
                        self.spam_report_modal = None;
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::ReportSpam { jid, reason }).await;
                            });
                        }
                        self.update(Message::ShowToast(
                            "Spam report sent.".into(),
                            ToastKind::Info,
                        ))
                    }
                }
            }

            // MULTI: account switcher screen — populate with live account data.
            Message::GoToAccountSwitcher => navigation::go_to_account_switcher(self),

            Message::AccountSwitcher(msg) => navigation::handle_account_switcher(self, msg),

            // DC-11: parse an xmpp: URI and dispatch the appropriate action.
            Message::HandleXmppUri(uri) => {
                let Some(parsed) = xmpp_uri::parse(&uri) else {
                    return self.update(Message::ShowToast(
                        format!("Invalid XMPP URI: {uri}"),
                        ToastKind::Error,
                    ));
                };
                match parsed.action {
                    xmpp_uri::XmppUriAction::Join => {
                        // Join a MUC room. Use own JID local part as default nick.
                        let nick = if let Screen::Chat(ref chat) = self.screen {
                            chat.own_jid().split('@').next().unwrap_or("me").to_string()
                        } else {
                            "me".to_string()
                        };
                        // Override with URI-provided nick if present.
                        let nick = parsed.params.get("nick").cloned().unwrap_or(nick);
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            let jid = parsed.jid.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::JoinRoom { jid, nick }).await;
                            });
                        }
                        // Open the room conversation panel.
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let action = chat.update(chat::Message::Sidebar(
                                crate::ui::sidebar::Message::SelectContact(parsed.jid),
                            ));
                            return self.handle_chat_action(action);
                        }
                    }
                    xmpp_uri::XmppUriAction::Subscribe => {
                        // Send a contact subscription / add to roster.
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            let jid = parsed.jid.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::AddContact(jid)).await;
                            });
                        }
                        return self.update(Message::ShowToast(
                            format!("Subscription request sent to {}", parsed.jid),
                            ToastKind::Info,
                        ));
                    }
                    // ?message or bare JID (no action) — open a direct chat.
                    xmpp_uri::XmppUriAction::Message | xmpp_uri::XmppUriAction::Unknown(_) => {
                        if let Screen::Chat(ref mut chat) = self.screen {
                            let action = chat.update(chat::Message::Sidebar(
                                crate::ui::sidebar::Message::SelectContact(parsed.jid),
                            ));
                            return self.handle_chat_action(action);
                        }
                    }
                    xmpp_uri::XmppUriAction::Remove => {
                        // Nothing to act on without a confirmation dialog — show info toast.
                        return self.update(Message::ShowToast(
                            format!("Use the contact list to remove {}", parsed.jid),
                            ToastKind::Info,
                        ));
                    }
                }
                Task::none()
            }

            Message::GoBack => navigation::go_back(self),

            Message::ToggleConsole => {
                self.show_console = !self.show_console;
                Task::none()
            }

            Message::Settings(smsg) => {
                let action = if let Screen::Settings(ref mut ss, _) = self.screen {
                    let action = ss.update(smsg);
                    self.settings = ss.settings().clone();
                    // M3: drain block/unblock commands produced by the settings panel
                    let cmds = ss.drain_commands();
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
                    action
                } else {
                    settings::Action::None
                };
                match action {
                    settings::Action::None => Task::none(),
                    settings::Action::Task(task) => task.map(Message::Settings),
                    settings::Action::GoBack => self.update(Message::GoBack),
                    settings::Action::Logout => self.update(Message::Logout),
                    settings::Action::OpenAbout => self.update(Message::GoToAbout),
                    settings::Action::OpenVCardEditor => self.update(Message::GoToVCardEditor),
                    settings::Action::EnableOmemo => {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::OmemoEnable).await;
                            });
                        }
                        Task::none()
                    }
                    settings::Action::AvatarSelected(data, mime_type) => {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            Task::future(async move {
                                let _ = tx.send(XmppCommand::SetAvatar { data, mime_type }).await;
                                Message::ShowToast("Uploading avatar…".into(), ToastKind::Info)
                            })
                        } else {
                            Task::none()
                        }
                    }
                    settings::Action::ClearHistory => {
                        let pool = self.db.clone();
                        tokio::spawn(async move {
                            let _ = crate::store::message_repo::clear_all(&pool).await;
                            let _ = crate::store::conversation_repo::clear_all(&pool).await;
                        });
                        Task::none()
                    }
                }
            }

            Message::Login(msg) => {
                let action = if let Screen::Login(login) = &mut self.screen {
                    login.update(msg)
                } else {
                    return Task::none();
                };

                match action {
                    login::Action::None => Task::none(),
                    login::Action::AttemptConnect(cfg) => {
                        if let Screen::Login(ref mut login) = self.screen {
                            if !config::is_valid_jid(&cfg.jid) {
                                login.on_error(
                                    "Invalid JID: must be in the form user@domain".into(),
                                );
                                return Task::none();
                            }
                        }
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            self.settings.last_jid = cfg.jid.clone();
                            self.settings.last_server = cfg.server.clone();
                            let _ = config::save(&self.settings);
                            self.last_connect_cfg = Some(cfg.clone());
                            self.reconnect_attempt = 0;
                            Task::future(async move {
                                let _ = tx.send(XmppCommand::Connect(cfg)).await;
                                Message::Login(login::Message::Connecting)
                            })
                        } else {
                            Task::none()
                        }
                    }
                    login::Action::AttemptRegister(cfg) => {
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            Task::future(async move {
                                let _ = tx.send(XmppCommand::Register(cfg)).await;
                                Message::Login(login::Message::Registering)
                            })
                        } else {
                            Task::none()
                        }
                    }
                    login::Action::GoToBenchmark => {
                        self.screen = Screen::Benchmark(BenchmarkScreen::new());
                        Task::none()
                    }
                    login::Action::RememberMeToggled(v) => {
                        self.settings.remember_me = v;
                        let _ = config::save(&self.settings);
                        Task::none()
                    }
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

            Message::Logout => navigation::logout(self),

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
                if let Screen::Chat(ref mut chat) = self.screen {
                    let action = chat.update(msg);
                    // E4: capture pending upload targets before draining commands
                    let upload_targets = chat.drain_upload_targets();
                    if !upload_targets.is_empty() && self.pending_upload.is_none() {
                        self.pending_upload = upload_targets.into_iter().next();
                    }
                    let cmds = chat.drain_commands();
                    if !cmds.is_empty() {
                        // Persist outgoing messages to SQLite before forwarding to engine.
                        let own_jid = chat.own_jid().to_string();
                        let own_bare_jid =
                            own_jid.split('/').next().unwrap_or(&own_jid).to_string();
                        let pool = self.db.clone();
                        let send_cmds: Vec<(String, String)> = cmds
                            .iter()
                            .filter_map(|c| {
                                if let XmppCommand::SendMessage { to, body, .. } = c {
                                    Some((to.clone(), body.clone()))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !send_cmds.is_empty() {
                            tokio::spawn(async move {
                                let ts = chrono::Utc::now().timestamp_millis();
                                for (to, body) in send_cmds {
                                    let _ =
                                        crate::store::conversation_repo::upsert(&pool, &to).await;
                                    let _ = crate::store::conversation_repo::update_last_activity(
                                        &pool, &to, ts,
                                    )
                                    .await;
                                    let _ = crate::store::message_repo::insert(
                                        &pool,
                                        &crate::store::message_repo::Message {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            conversation_jid: to,
                                            from_jid: own_bare_jid.clone(),
                                            body: Some(body),
                                            timestamp: ts,
                                            stanza_id: None,
                                            origin_id: None,
                                            state: "sent".into(),
                                            edited_body: None,
                                            retracted: 0,
                                        },
                                    )
                                    .await;
                                }
                            });
                        }
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                for cmd in cmds {
                                    let _ = tx.send(cmd).await;
                                }
                            });
                        }
                    }
                    self.handle_chat_action(action)
                } else {
                    Task::none()
                }
            }

            // MEMO: OMEMO trust dialog messages
            Message::OmemoTrust(msg) => {
                if let Some(ref mut modal) = self.omemo_trust_modal {
                    match modal.update(msg) {
                        omemo_trust::Action::TrustDevice { jid, device_id } => {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    let _ = tx
                                        .send(XmppCommand::OmemoTrustDevice { jid, device_id })
                                        .await;
                                });
                            }
                        }
                        omemo_trust::Action::Close => {
                            self.omemo_trust_modal = None;
                        }
                        omemo_trust::Action::None => {}
                    }
                }
                Task::none()
            }

            // Tab focus navigation between input fields
            Message::FocusNext => iced::widget::focus_next(),
            Message::FocusPrevious => iced::widget::focus_previous(),

            Message::XmppReady(tx) => {
                tracing::debug!("xmpp command channel ready");
                self.xmpp_tx = Some(tx.clone());
                // AUTH-1: if we have a pending auto-connect config, connect now.
                if let Some(cfg) = self.auto_connect_cfg.take() {
                    self.last_connect_cfg = Some(cfg.clone());
                    self.reconnect_attempt = 0;
                    return Task::future(async move {
                        let _ = tx.send(XmppCommand::Connect(cfg)).await;
                        Message::Login(login::Message::Connecting)
                    });
                }
                Task::none()
            }

            Message::XmppEvent(event) => xmpp_events::handle(self, event),
        }
    }

    /// Handle a `chat::Action` uniformly.  Used by `Message::Chat`,
    /// `Message::FilesDropped`, `Message::PasteFromClipboard`, and any
    /// `XmppEvent` handler that forwards a message into the chat screen.
    pub(crate) fn handle_chat_action(&mut self, action: chat::Action) -> Task<Message> {
        match action {
            chat::Action::None => Task::none(),
            chat::Action::Task(task) => task.map(Message::Chat),
            chat::Action::OpenSettings => self.update(Message::GoToSettings),
            chat::Action::OpenAccountSwitcher => self.update(Message::GoToAccountSwitcher),
            chat::Action::OpenOmemoTrust(jid) => {
                tracing::info!("OMEMO: opening trust dialog for {jid}");
                let device_ids = self
                    .omemo_peer_devices
                    .get(&jid)
                    .cloned()
                    .unwrap_or_default();
                let devices: Vec<omemo_trust::DeviceEntry> = device_ids
                    .into_iter()
                    .map(|id| omemo_trust::DeviceEntry {
                        device_id: id,
                        identity_key: vec![],
                        trust: crate::xmpp::modules::omemo::store::TrustState::Undecided,
                        label: None,
                        active: true,
                    })
                    .collect();
                self.omemo_trust_modal = Some(omemo_trust::OmemoTrustScreen::new(jid, devices));
                Task::none()
            }
            chat::Action::SetPresence(status) => {
                self.own_presence = status;
                Task::none()
            }
            chat::Action::ToggleMute(jid) => {
                if self.settings.muted_jids.contains(&jid) {
                    self.settings.muted_jids.remove(&jid);
                } else {
                    self.settings.muted_jids.insert(jid);
                }
                let _ = config::save(&self.settings);
                Task::none()
            }
            chat::Action::ContactSelected(jid) => {
                let history_task = {
                    let jid = jid.clone();
                    let pool = self.db.clone();
                    Task::future(async move {
                        let rows =
                            crate::store::message_repo::find_by_conversation(&pool, &jid, 50)
                                .await
                                .unwrap_or_default();
                        Message::MessagesLoaded(jid, rows)
                    })
                };
                if let Screen::Chat(ref chat) = self.screen {
                    if let Some(last_id) = chat.last_message_id(&jid) {
                        let pool = self.db.clone();
                        tokio::spawn(async move {
                            let _ =
                                crate::store::conversation_repo::mark_read(&pool, &jid, &last_id)
                                    .await;
                        });
                    }
                }
                history_task
            }
        }
    }

    /// Handle a `vcard_editor::Action` uniformly.
    pub(crate) fn handle_vcard_action(&mut self, action: vcard_editor::Action) -> Task<Message> {
        match action {
            vcard_editor::Action::None => Task::none(),
            vcard_editor::Action::Save(fields) => {
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(XmppCommand::SetOwnVCard(fields)).await;
                    });
                }
                Task::none()
            }
            vcard_editor::Action::Close => {
                if let Screen::VCardEditor(_, prev) =
                    std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                {
                    self.screen = *prev;
                }
                Task::none()
            }
        }
    }

    /// Handle an `adhoc::Action` uniformly.
    pub(crate) fn handle_adhoc_action(&mut self, action: adhoc::Action) -> Task<Message> {
        match action {
            adhoc::Action::None => Task::none(),
            adhoc::Action::Discover { target_jid } => {
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx
                            .send(XmppCommand::DiscoverAdhocCommands { target_jid })
                            .await;
                    });
                }
                Task::none()
            }
            adhoc::Action::Execute { target_jid, node } => {
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx
                            .send(XmppCommand::ExecuteAdhocCommand {
                                to_jid: target_jid,
                                node,
                            })
                            .await;
                    });
                }
                Task::none()
            }
            adhoc::Action::Submit {
                target_jid,
                node,
                session_id,
                fields,
            } => {
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx
                            .send(XmppCommand::ContinueAdhocCommand {
                                to_jid: target_jid,
                                node,
                                session_id,
                                fields,
                            })
                            .await;
                    });
                }
                Task::none()
            }
            adhoc::Action::Cancel {
                target_jid,
                node,
                session_id,
            } => {
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx
                            .send(XmppCommand::CancelAdhocCommand {
                                to_jid: target_jid,
                                node,
                                session_id,
                            })
                            .await;
                    });
                }
                Task::none()
            }
            adhoc::Action::Close => {
                if let Screen::Adhoc(_, prev) =
                    std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                {
                    self.screen = *prev;
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
            Screen::Chat(chat) => {
                let vctx = ViewContext {
                    avatars: &self.avatar_cache,
                    time_format: self.settings.time_format,
                    own_jid: chat.own_jid(),
                    omemo_enabled: self.omemo_enabled,
                };
                chat.view(&vctx).map(Message::Chat)
            }
            Screen::Settings(ss, _) => ss.view().map(Message::Settings),
            Screen::About(about, _) => about.view().map(Message::About),
            Screen::VCardEditor(ve, _) => ve.view().map(Message::VCardEditor),
            Screen::Adhoc(adhoc, _) => adhoc.view().map(Message::Adhoc),
            Screen::AccountSwitcher(sw, _) => sw.view().map(Message::AccountSwitcher),
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

        // MEMO: OMEMO trust modal overlay
        if let Some(ref trust_screen) = self.omemo_trust_modal {
            let backdrop = container(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.5,
                    ))),
                    ..Default::default()
                });
            let trust_view = trust_screen.view().map(Message::OmemoTrust);
            let modal_box = container(trust_view)
                .width(Length::Fixed(480.0))
                .height(Length::Fixed(480.0))
                .style(styles::modal_container_style);
            let modal_overlay = container(
                column![
                    Space::new(Length::Fill, Length::Fixed(80.0)),
                    row![
                        Space::new(Length::Fill, Length::Shrink),
                        modal_box,
                        Space::new(Length::Fill, Length::Shrink),
                    ]
                    .width(Length::Fill),
                ]
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill);
            layers.push(backdrop.into());
            layers.push(modal_overlay.into());
        }

        // L5: spam report modal overlay
        if let Some(ref modal) = self.spam_report_modal {
            let backdrop = container(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.0, 0.0, 0.0, 0.5,
                    ))),
                    ..Default::default()
                });
            let modal_view = modal.view().map(Message::SpamReport);
            let modal_overlay = container(
                column![
                    Space::new(Length::Fill, Length::Fixed(100.0)),
                    row![
                        Space::new(Length::Fill, Length::Shrink),
                        modal_view,
                        Space::new(Length::Fill, Length::Shrink),
                    ],
                ]
                .spacing(0),
            )
            .width(Length::Fill)
            .height(Length::Fill);
            layers.push(backdrop.into());
            layers.push(modal_overlay.into());
        }

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
            let results = command_palette::search(&palette_commands(), &self.palette_query);

            let input = text_input("Search commands...", &self.palette_query)
                .id(iced::widget::text_input::Id::new("palette_input"))
                .on_input(Message::PaletteQuery)
                .on_submit(if results.is_empty() {
                    Message::TogglePalette
                } else {
                    Message::PaletteExecute(0)
                })
                .padding(10)
                .size(16);

            let cmd_buttons: Vec<Element<Message>> = results
                .into_iter()
                .enumerate()
                .map(|(i, m)| {
                    button(text(m.command.label).size(14))
                        .on_press(Message::PaletteExecute(i))
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
                        let mut s = styles::modal_container_style(theme);
                        s.shadow = iced::Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                            offset: iced::Vector::new(0.0, 4.0),
                            blur_radius: 16.0,
                        };
                        s
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
            use crate::xmpp::modules::console::StanzaDirection;
            let entry_rows: Vec<Element<Message>> = self
                .xmpp_console
                .entries()
                .map(|e| {
                    let prefix = if e.direction == StanzaDirection::Sent {
                        "[sent]"
                    } else {
                        "[recv]"
                    };
                    let snippet: String = e.xml.chars().take(120).collect();
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

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            xmpp::subscription::xmpp_subscription((*self.db).clone()),
            subscriptions::keyboard_shortcuts(),
            subscriptions::file_drop(),
            subscriptions::idle_tick(),
            subscriptions::voice_tick(),
            subscriptions::multi_engine_events(self.multi_event_rx.clone()),
        ])
    }
}
