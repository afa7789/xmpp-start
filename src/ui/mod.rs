// Task P2: UI Foundation
// Reference: https://github.com/squidowl/halloy (iced IRC client)

use std::collections::HashMap;
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
pub mod omemo_trust;
pub mod settings;
pub mod sidebar;
pub mod spam_report;
pub mod styling;
pub mod toast;
pub mod vcard_editor;

pub use benchmark::BenchmarkScreen;
pub use chat::ChatScreen;
pub use login::LoginScreen;

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
type MultiEventRx = std::sync::Arc<std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<(AccountId, XmppEvent)>>>>;

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
    // F1: debug console — circular buffer of raw XML stanzas and visibility flag
    xmpp_console: XmppConsole,
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
    // AUTH-1: pending auto-connect config — consumed when XmppReady fires
    auto_connect_cfg: Option<crate::xmpp::ConnectConfig>,
    // L5: spam report modal — Some when open
    spam_report_modal: Option<spam_report::SpamReportModal>,
    // MULTI: per-account UI state manager
    account_state_mgr: AccountStateManager,
    // DC-21: multi-engine manager for additional accounts
    multi_engine: MultiEngineManager,
    // DC-21: tx end of the multi-account event channel
    multi_event_tx: tokio::sync::mpsc::Sender<(AccountId, XmppEvent)>,
    // DC-21: rx end, shared so the iced subscription can take it once
    multi_event_rx: MultiEventRx,
    // DC-21: true while navigating to Login to add a second account
    is_adding_account: bool,
}

/// S1: tracks which auto-away stage has been sent to the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleState {
    Active,
    AutoAway,
    AutoXa,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    Login(login::Message),
    Benchmark(benchmark::Message),
    Chat(chat::Message),
    GoToBenchmark,
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
}

enum Screen {
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
                avatar_cache: HashMap::new(),
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
                    chat.prefill_conversations(jids);
                }
                Task::none()
            }

            Message::GoToSettings => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                let mut settings_screen = settings::SettingsScreen::new(self.settings.clone());
                // Populate Account Details section with the live connection state.
                if let Screen::Chat(ref chat) = prev {
                    settings_screen.set_account_info(account_details::AccountInfo {
                        bound_jid: chat.own_jid().to_string(),
                        connected: true,
                        server_features: String::new(),
                        auth_method: String::new(),
                    });
                }
                self.screen = Screen::Settings(Box::new(settings_screen), Box::new(prev));
                Task::none()
            }

            Message::GoToAbout => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                self.screen = Screen::About(Box::default(), Box::new(prev));
                Task::none()
            }

            Message::About(msg) => {
                let is_back = matches!(msg, about::Message::Back);
                if let Screen::About(ref mut about, _) = self.screen {
                    about.update(msg);
                }
                if is_back {
                    // Restore the previous screen.
                    if let Screen::About(_, prev) =
                        std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                    {
                        self.screen = *prev;
                    }
                }
                Task::none()
            }

            // K2: navigate to vCard editor
            Message::GoToVCardEditor => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                let mut ve = vcard_editor::VCardEditorScreen::new();
                // Request own vCard from engine.
                if let Some(ref tx) = self.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(XmppCommand::FetchOwnVCard).await;
                    });
                }
                ve.loading = true;
                self.screen = Screen::VCardEditor(Box::new(ve), Box::new(prev));
                Task::none()
            }

            Message::VCardEditor(msg) => {
                // Intercept Close (back navigation)
                let is_close = matches!(msg, vcard_editor::Message::Close);
                // Intercept SaveRequested to send command to engine
                let is_save = matches!(msg, vcard_editor::Message::SaveRequested);
                if let Screen::VCardEditor(ref mut ve, _) = self.screen {
                    let _ = ve.update(msg);
                    if is_save {
                        let fields = ve.current_fields();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx.send(XmppCommand::SetOwnVCard(fields)).await;
                            });
                        }
                    }
                }
                if is_close {
                    if let Screen::VCardEditor(_, prev) =
                        std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                    {
                        self.screen = *prev;
                    }
                }
                Task::none()
            }

            // L4: navigate to ad-hoc commands screen
            Message::GoToAdhoc => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                self.screen = Screen::Adhoc(Box::default(), Box::new(prev));
                Task::none()
            }

            Message::Adhoc(msg) => {
                let is_close = matches!(msg, adhoc::Message::Close);
                let is_discover = matches!(msg, adhoc::Message::DiscoverRequested);
                let is_submit = matches!(msg, adhoc::Message::SubmitForm);
                let is_cancel = matches!(msg, adhoc::Message::CancelCommand);
                if let Screen::Adhoc(ref mut adhoc, _) = self.screen {
                    if is_discover {
                        let target = adhoc.target_jid.clone();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx
                                    .send(XmppCommand::DiscoverAdhocCommands { target_jid: target })
                                    .await;
                            });
                        }
                    }
                    if let adhoc::Message::CommandSelected(ref node) = msg {
                        let target = adhoc.target_jid.clone();
                        let node = node.clone();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let _ = tx
                                    .send(XmppCommand::ExecuteAdhocCommand {
                                        to_jid: target,
                                        node,
                                    })
                                    .await;
                            });
                        }
                    }
                    if is_submit {
                        if let Some(node) = adhoc.active_node().map(str::to_owned) {
                            if let Some(session_id) = adhoc.active_session_id().map(str::to_owned) {
                                let fields = adhoc.collect_fields();
                                let target = adhoc.target_jid.clone();
                                if let Some(ref tx) = self.xmpp_tx {
                                    let tx = tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx
                                            .send(XmppCommand::ContinueAdhocCommand {
                                                to_jid: target,
                                                node,
                                                session_id,
                                                fields,
                                            })
                                            .await;
                                    });
                                }
                            }
                        }
                    }
                    if is_cancel {
                        if let Some(node) = adhoc.active_node().map(str::to_owned) {
                            if let Some(session_id) = adhoc.active_session_id().map(str::to_owned) {
                                let target = adhoc.target_jid.clone();
                                if let Some(ref tx) = self.xmpp_tx {
                                    let tx = tx.clone();
                                    tokio::spawn(async move {
                                        let _ = tx
                                            .send(XmppCommand::CancelAdhocCommand {
                                                to_jid: target,
                                                node,
                                                session_id,
                                            })
                                            .await;
                                    });
                                }
                            }
                        }
                    }
                    let _ = adhoc.update(msg);
                }
                if is_close {
                    if let Screen::Adhoc(_, prev) =
                        std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                    {
                        self.screen = *prev;
                    }
                }
                Task::none()
            }

            // L5: spam report modal
            Message::OpenSpamReport(jid) => {
                self.spam_report_modal = Some(spam_report::SpamReportModal::new(jid));
                Task::none()
            }

            Message::SpamReport(msg) => {
                let is_cancel = matches!(msg, spam_report::Message::Cancel);
                let mut cmd_to_send: Option<(String, Option<String>)> = None;
                if let Some(ref mut modal) = self.spam_report_modal {
                    if let Some(spam_cmd) = modal.update(msg) {
                        cmd_to_send = Some((spam_cmd.jid, spam_cmd.reason));
                    }
                }
                if is_cancel {
                    self.spam_report_modal = None;
                }
                if let Some((jid, reason)) = cmd_to_send {
                    self.spam_report_modal = None;
                    if let Some(ref tx) = self.xmpp_tx {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(XmppCommand::ReportSpam { jid, reason }).await;
                        });
                    }
                    return self.update(Message::ShowToast(
                        "Spam report sent.".into(),
                        ToastKind::Info,
                    ));
                }
                Task::none()
            }

            // MULTI: account switcher screen — populate with live account data.
            Message::GoToAccountSwitcher => {
                let prev = std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()));
                // Build the account entry list from the state manager.
                let active_id = self.account_state_mgr.active_id().cloned();
                let entries: Vec<account_switcher::AccountEntry> = self
                    .account_state_mgr
                    .account_ids()
                    .map(|id| account_switcher::AccountEntry {
                        label: id.as_str().to_owned(),
                        connected: true, // single-engine mode: if registered, it's connected
                        color: None,
                        id: id.clone(),
                    })
                    .collect();
                let mut sw = account_switcher::AccountSwitcherScreen::new();
                sw.accounts = entries;
                sw.active = active_id;
                self.screen = Screen::AccountSwitcher(Box::new(sw), Box::new(prev));
                Task::none()
            }

            Message::AccountSwitcher(msg) => {
                let is_close = matches!(msg, account_switcher::Message::Close);
                let is_add = matches!(msg, account_switcher::Message::AddAccount);
                let switch_to = if let account_switcher::Message::SwitchTo(ref id) = msg {
                    Some(id.clone())
                } else {
                    None
                };
                if let Screen::AccountSwitcher(ref mut sw, _) = self.screen {
                    sw.update(msg);
                }
                if let Some(ref id) = switch_to {
                    // MULTI: update per-account state manager and multi-engine focus.
                    self.account_state_mgr.switch_to(id);
                    self.multi_engine.switch_active(id.clone());
                    // Sync the new active account into the chat screen's indicator bar.
                    let unread = self
                        .account_state_mgr
                        .get_active()
                        .map_or(0, |s| s.unread_total);
                    if let Screen::Chat(ref mut chat) = self.screen {
                        chat.set_active_account(Some(id.clone()), unread);
                    }
                    // Also notify the engine so it can route commands correctly.
                    if let Some(ref tx) = self.xmpp_tx {
                        let tx = tx.clone();
                        let id_clone = id.clone();
                        tokio::spawn(async move {
                            let _ = tx.send(XmppCommand::SwitchAccount(id_clone)).await;
                        });
                    }
                }
                if is_add {
                    // MULTI: AddAccount navigates to the login screen so the user
                    // can enter credentials for a second account.  When that
                    // connection succeeds the Connected handler will register it
                    // via the MultiEngineManager.
                    self.is_adding_account = true;
                    if let Screen::AccountSwitcher(_, prev) =
                        std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                    {
                        drop(prev); // discard — login is the new entry point
                    }
                    return Task::none();
                }
                if is_close {
                    if let Screen::AccountSwitcher(_, prev) =
                        std::mem::replace(&mut self.screen, Screen::Login(LoginScreen::new()))
                    {
                        self.screen = *prev;
                    }
                }
                Task::none()
            }

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
                            let _ = chat.update(chat::Message::Sidebar(
                                crate::ui::sidebar::Message::SelectContact(parsed.jid),
                            ));
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
                            let _ = chat.update(chat::Message::Sidebar(
                                crate::ui::sidebar::Message::SelectContact(parsed.jid),
                            ));
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
                // AUTH-2: intercept Logout from settings panel before delegating.
                if matches!(smsg, settings::Message::Logout) {
                    return self.update(Message::Logout);
                }
                // M7: intercept OpenAbout from settings panel before delegating.
                if matches!(smsg, settings::Message::OpenAbout) {
                    return self.update(Message::GoToAbout);
                }
                // K2: intercept OpenVCardEditor from settings panel before delegating.
                if matches!(smsg, settings::Message::OpenVCardEditor) {
                    return self.update(Message::GoToVCardEditor);
                }
                let go_back = matches!(smsg, settings::Message::Back);
                // M6: detect clear-history confirmation before delegating
                let is_clear_history = matches!(smsg, settings::Message::ClearHistoryConfirm);
                let mut avatar_task = Task::none();
                if let settings::Message::AvatarSelected(ref data, ref mime_type) = smsg {
                    if let Some(ref tx) = self.xmpp_tx {
                        let tx = tx.clone();
                        let d = data.clone();
                        let m = mime_type.clone();
                        avatar_task = Task::future(async move {
                            let _ = tx
                                .send(XmppCommand::SetAvatar {
                                    data: d,
                                    mime_type: m,
                                })
                                .await;
                            Message::ShowToast("Uploading avatar…".into(), ToastKind::Info)
                        });
                    }
                }
                let mut update_task = Task::none();
                if let Screen::Settings(ref mut ss, _) = self.screen {
                    update_task = ss.update(smsg).map(Message::Settings);
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
                }
                // M6: clear all chat history from the DB
                if is_clear_history {
                    let pool = self.db.clone();
                    tokio::spawn(async move {
                        let _ = crate::store::message_repo::clear_all(&pool).await;
                        let _ = crate::store::conversation_repo::clear_all(&pool).await;
                    });
                }
                if go_back {
                    return Task::batch([avatar_task, update_task, self.update(Message::GoBack)]);
                }
                Task::batch([avatar_task, update_task])
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

                if matches!(msg, login::Message::Register) {
                    if let Screen::Login(ref mut login) = self.screen {
                        let cfg = login.connect_config();
                        if let Some(ref tx) = self.xmpp_tx {
                            let tx = tx.clone();
                            return Task::future(async move {
                                let _ = tx.send(XmppCommand::Register(cfg)).await;
                                Message::Login(login::Message::Registering)
                            });
                        }
                    }
                }

                if matches!(msg, login::Message::GoToBenchmark) {
                    self.screen = Screen::Benchmark(BenchmarkScreen::new());
                    return Task::none();
                }

                // AUTH-1: persist remember_me preference when toggled on login screen.
                if let login::Message::RememberMeToggled(v) = msg {
                    self.settings.remember_me = v;
                    let _ = config::save(&self.settings);
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
                    self.settings.remember_me,
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
                // MULTI: intercept OpenAccountSwitcher from sidebar
                if let chat::Message::Sidebar(crate::ui::sidebar::Message::OpenAccountSwitcher) =
                    msg
                {
                    return self.update(Message::GoToAccountSwitcher);
                }
                // OMEMO: open trust dialog when user clicks a lock badge
                if let chat::Message::OpenOmemoTrust(ref peer_jid) = msg {
                    let _peer = peer_jid.clone();
                    tracing::info!("OMEMO: opening trust dialog for {_peer}");
                    return Task::none();
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
                // AUTH-1: if we have a pending auto-connect config, connect now.
                if let Some(cfg) = self.auto_connect_cfg.take() {
                    let tx = self.xmpp_tx.as_ref().unwrap().clone();
                    self.last_connect_cfg = Some(cfg.clone());
                    self.reconnect_attempt = 0;
                    return Task::future(async move {
                        let _ = tx.send(XmppCommand::Connect(cfg)).await;
                        Message::Login(login::Message::Connecting)
                    });
                }
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
                        // MULTI: register this account in the state manager so the
                        // sidebar indicator bar is populated on first connect.
                        let account_id = AccountId::new(bound_jid.clone());
                        self.account_state_mgr.add_account(account_id.clone());

                        // DC-21: if this connection was triggered by AddAccount,
                        // register the new account in the multi-engine manager
                        // so future commands can be routed to it.
                        if self.is_adding_account {
                            self.is_adding_account = false;
                            let cfg =
                                config::AccountConfig::new(bound_jid.clone());
                            self.multi_engine
                                .start_account(cfg, self.multi_event_tx.clone());
                            self.multi_engine.switch_active(account_id.clone());
                        }
                        let mut chat_screen = ChatScreen::new(bound_jid.clone());
                        // Pass the active account info into the chat screen so
                        // view_with_drafts() can render the indicator bar.
                        let unread = self
                            .account_state_mgr
                            .get_active()
                            .map_or(0, |s| s.unread_total);
                        chat_screen.set_active_account(Some(account_id), unread);
                        self.screen = Screen::Chat(Box::new(chat_screen));
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
                        // Also pre-populate known conversations from DB cache.
                        let pool2 = self.db.clone();
                        let conv_prefill = Task::future(async move {
                            let jids: Vec<String> =
                                crate::store::conversation_repo::get_all(&pool2)
                                    .await
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|c| c.jid)
                                    .collect();
                            Message::ConversationsPrefill(jids)
                        });
                        let toast = self.update(Message::ShowToast(
                            format!("Connected as {}", bound_jid),
                            ToastKind::Success,
                        ));
                        return Task::batch([roster_prefill, conv_prefill, toast]);
                    }
                    XmppEvent::RegistrationFormReceived { .. } => {
                        // For now, just show a toast. In a full impl, we'd show the Data Form.
                        return self.update(Message::ShowToast(
                            "Registration form received (XEP-0077)".into(),
                            ToastKind::Info,
                        ));
                    }
                    XmppEvent::RegistrationSuccess => {
                        return self.update(Message::ShowToast(
                            "Account registered successfully!".into(),
                            ToastKind::Success,
                        ));
                    }
                    XmppEvent::RegistrationFailure(reason) => {
                        if let Screen::Login(ref mut login) = self.screen {
                            login.on_error(reason.clone());
                        }
                        return self.update(Message::ShowToast(
                            format!("Registration failed: {}", reason),
                            ToastKind::Error,
                        ));
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
                            let _ = crate::store::conversation_repo::update_last_activity(
                                &pool, &bare_jid, ts,
                            )
                            .await;
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
                            // MULTI: increment unread count for background conversations.
                            if !is_active && !msg.is_historical {
                                if let Some(state) = self.account_state_mgr.get_active_mut() {
                                    state.unread_total += 1;
                                }
                                // Sync updated unread total back into the chat screen.
                                let unread = self
                                    .account_state_mgr
                                    .get_active()
                                    .map_or(0, |s| s.unread_total);
                                let active_id = self.account_state_mgr.active_id().cloned();
                                chat.set_active_account(active_id, unread);
                            }
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
                        // F5: fetch avatar for newly-available contacts not yet cached
                        if available && !self.avatar_cache.contains_key(jid.as_str()) {
                            if let Some(ref tx) = self.xmpp_tx {
                                let tx = tx.clone();
                                let jid_owned = jid.clone();
                                tokio::spawn(async move {
                                    let _ = tx.send(XmppCommand::FetchAvatar(jid_owned)).await;
                                });
                            }
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
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        if direction == "sent" {
                            self.xmpp_console.push_sent(&xml, ts);
                        } else {
                            self.xmpp_console.push_received(&xml, ts);
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
                        // Persist the retraction so the message stays tombstoned after restart.
                        let pool = self.db.clone();
                        let mid = message_id.clone();
                        tokio::spawn(async move {
                            let _ = crate::store::message_repo::mark_retracted(&pool, &mid).await;
                        });
                    }
                    // K2: own vCard received — populate the vCard editor if it's open
                    XmppEvent::OwnVCardReceived(fields) => {
                        if let Screen::VCardEditor(ref mut ve, _) = self.screen {
                            let _ = ve.update(vcard_editor::Message::VCardLoaded(fields));
                        }
                    }
                    // K2: own vCard saved — confirm to the editor
                    XmppEvent::OwnVCardSaved => {
                        if let Screen::VCardEditor(ref mut ve, _) = self.screen {
                            let _ = ve.update(vcard_editor::Message::VCardSaved);
                        }
                    }
                    // L4: ad-hoc commands discovered — forward to adhoc screen
                    XmppEvent::AdhocCommandsDiscovered { from_jid, commands } => {
                        if let Screen::Adhoc(ref mut adhoc, _) = self.screen {
                            let _ = adhoc.update(adhoc::Message::CommandsDiscovered {
                                _from_jid: from_jid,
                                commands,
                            });
                        }
                    }
                    // L4: ad-hoc command response — forward to adhoc screen
                    XmppEvent::AdhocCommandResult(resp) => {
                        if let Screen::Adhoc(ref mut adhoc, _) = self.screen {
                            let _ = adhoc.update(adhoc::Message::CommandResponseReceived(resp));
                        }
                    }
                    // MULTI: account switched — sync indicator bar in the chat screen.
                    XmppEvent::AccountSwitched(ref id) => {
                        self.account_state_mgr.switch_to(id);
                        let unread = self
                            .account_state_mgr
                            .get_active()
                            .map_or(0, |s| s.unread_total);
                        if let Screen::Chat(ref mut chat) = self.screen {
                            chat.set_active_account(Some(id.clone()), unread);
                        }
                    }
                    // E1: XEP-0308 last message correction — persist the edited body.
                    XmppEvent::CorrectionReceived {
                        ref original_id,
                        new_body: ref body,
                        ..
                    } => {
                        let pool = self.db.clone();
                        let oid = original_id.clone();
                        let nb = body.clone();
                        tokio::spawn(async move {
                            let _ =
                                crate::store::message_repo::update_body(&pool, &oid, &nb).await;
                        });
                    }
                    // MEMO / other agents: unhandled events from additional modules.
                    XmppEvent::LocationReceived { .. }
                    | XmppEvent::BobReceived(_)
                    | XmppEvent::OmemoDeviceListReceived { .. }
                    | XmppEvent::OmemoMessageDecrypted { .. }
                    | XmppEvent::OmemoKeyExchangeNeeded { .. }
                    | XmppEvent::StickerPackReceived(_)
                    | XmppEvent::IgnoreListReceived { .. }
                    | XmppEvent::ConversationsReceived(_)
                    | XmppEvent::RetractionReceived { .. }
                    | XmppEvent::PasswordChanged { .. }
                    | XmppEvent::AccountDeleted { .. } => {}
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
        let xmpp_sub = xmpp::subscription::xmpp_subscription((*self.db).clone());
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
                if key == Key::Character("b".into()) {
                    return Some(Message::Chat(chat::Message::ComposerBold));
                }
                if key == Key::Character("i".into()) {
                    return Some(Message::Chat(chat::Message::ComposerItalic));
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
        // DC-21: multi-engine event subscription — polls the shared receiver
        // for events from additional account engines and routes them as XmppEvent.
        let multi_rx = self.multi_event_rx.clone();
        let multi_sub = Subscription::run_with_id("multi-engine-events", {
            iced::stream::channel(32, move |mut output| async move {
                // Take the receiver out of the shared slot (once).
                let mut rx = {
                    let mut guard = multi_rx.lock().unwrap();
                    guard.take()
                };
                if let Some(ref mut rx) = rx {
                    while let Some((_account_id, event)) = rx.recv().await {
                        let _ = output
                            .try_send(Message::XmppEvent(event));
                    }
                }
                // Keep the future alive so the subscription isn't dropped.
                std::future::pending::<()>().await;
            })
        });
        Subscription::batch([xmpp_sub, kb_sub, drop_sub, idle_sub, voice_tick_sub, multi_sub])
    }
}
