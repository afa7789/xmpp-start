//! Navigation helpers extracted from `App::update()`.
//!
//! Each function receives `&mut App` (plus any message-specific data) and
//! returns `Task<Message>`.  `App::update()` delegates to these for every
//! screen-transition message so that the giant match stays thin.

use iced::Task;

use super::{
    about, account_details, account_switcher, login::LoginScreen, settings, vcard_editor, App,
    Message, Screen,
};
use crate::config;
use crate::xmpp::XmppCommand;

// ---------------------------------------------------------------------------
// GoToSettings
// ---------------------------------------------------------------------------

pub(crate) fn go_to_settings(app: &mut App) -> Task<Message> {
    let mut settings_screen = settings::SettingsScreen::new(app.settings.clone());
    // Populate Account Details section with the live connection state.
    if let Screen::Chat(ref chat) = app.screen {
        settings_screen.set_account_info(account_details::AccountInfo {
            bound_jid: chat.own_jid().to_string(),
            connected: true,
            server_features: String::new(),
            auth_method: String::new(),
        });
    }
    // MEMO: propagate current OMEMO activation state into the settings screen.
    if let Some(device_id) = app.omemo_device_id {
        settings_screen.set_omemo_active(device_id);
    }
    // Full-screen settings: swap the screen (keep previous for GoBack)
    let prev = std::mem::replace(&mut app.screen, Screen::Login(super::login::LoginScreen::new()));
    app.screen = Screen::Settings(Box::new(settings_screen), Box::new(prev));
    Task::none()
}

// ---------------------------------------------------------------------------
// GoToAbout
// ---------------------------------------------------------------------------

pub(crate) fn go_to_about(app: &mut App) -> Task<Message> {
    let prev = std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()));
    app.screen = Screen::About(Box::default(), Box::new(prev));
    Task::none()
}

// ---------------------------------------------------------------------------
// About(msg) — delegates to the about screen, navigates back on GoBack
// ---------------------------------------------------------------------------

pub(crate) fn handle_about(app: &mut App, msg: about::Message) -> Task<Message> {
    if let Screen::About(ref mut about, _) = app.screen {
        if let about::Action::GoBack = about.update(msg) {
            if let Screen::About(_, prev) =
                std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()))
            {
                app.screen = *prev;
            }
        }
    }
    Task::none()
}

// ---------------------------------------------------------------------------
// GoToVCardEditor
// ---------------------------------------------------------------------------

pub(crate) fn go_to_vcard_editor(app: &mut App) -> Task<Message> {
    let prev = std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()));
    let mut ve = vcard_editor::VCardEditorScreen::new();
    // Request own vCard from engine.
    if let Some(ref tx) = app.xmpp_tx {
        let tx = tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(XmppCommand::FetchOwnVCard).await;
        });
    }
    ve.loading = true;
    app.screen = Screen::VCardEditor(Box::new(ve), Box::new(prev));
    Task::none()
}

// ---------------------------------------------------------------------------
// GoToAdhoc
// ---------------------------------------------------------------------------

pub(crate) fn go_to_adhoc(app: &mut App) -> Task<Message> {
    let prev = std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()));
    app.screen = Screen::Adhoc(Box::default(), Box::new(prev));
    Task::none()
}

// ---------------------------------------------------------------------------
// GoToAccountSwitcher
// ---------------------------------------------------------------------------

pub(crate) fn go_to_account_switcher(app: &mut App) -> Task<Message> {
    let prev = std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()));
    // Build the account entry list from the state manager.
    let active_id = app.account_state_mgr.active_id().cloned();
    let entries: Vec<account_switcher::AccountEntry> = app
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
    app.screen = Screen::AccountSwitcher(Box::new(sw), Box::new(prev));
    Task::none()
}

// ---------------------------------------------------------------------------
// AccountSwitcher(msg)
// ---------------------------------------------------------------------------

pub(crate) fn handle_account_switcher(
    app: &mut App,
    msg: account_switcher::Message,
) -> Task<Message> {
    if let Screen::AccountSwitcher(ref mut sw, _) = app.screen {
        match sw.update(msg) {
            account_switcher::Action::SwitchTo(id) => {
                // MULTI: update per-account state manager and multi-engine focus.
                app.account_state_mgr.switch_to(&id);
                app.multi_engine.switch_active(id.clone());
                // Sync the new active account into the chat screen's indicator bar.
                let unread = app
                    .account_state_mgr
                    .get_active()
                    .map_or(0, |s| s.unread_total);
                if let Screen::Chat(ref mut chat) = app.screen {
                    chat.set_active_account(Some(id.clone()), unread);
                }
                // Also notify the engine so it can route commands correctly.
                if let Some(ref tx) = app.xmpp_tx {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let _ = tx.send(XmppCommand::SwitchAccount(id)).await;
                    });
                }
            }
            account_switcher::Action::AddAccount => {
                // MULTI: AddAccount navigates to the login screen so the user
                // can enter credentials for a second account.  When that
                // connection succeeds the Connected handler will register it
                // via the MultiEngineManager.
                app.is_adding_account = true;
                if let Screen::AccountSwitcher(_, prev) =
                    std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()))
                {
                    drop(prev); // discard — login is the new entry point
                }
                return Task::none();
            }
            account_switcher::Action::Close => {
                if let Screen::AccountSwitcher(_, prev) =
                    std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()))
                {
                    app.screen = *prev;
                }
            }
            account_switcher::Action::None => {}
        }
    }
    Task::none()
}

// ---------------------------------------------------------------------------
// GoBack
// ---------------------------------------------------------------------------

pub(crate) fn go_back(app: &mut App) -> Task<Message> {
    if let Screen::Settings(ref ss, _) = app.screen {
        app.settings = ss.settings().clone();
    }
    if let Screen::Settings(_, prev) =
        std::mem::replace(&mut app.screen, Screen::Login(LoginScreen::new()))
    {
        app.screen = *prev;
    }
    Task::none()
}

// ---------------------------------------------------------------------------
// Logout
// ---------------------------------------------------------------------------

pub(crate) fn logout(app: &mut App) -> Task<Message> {
    // AUTH-2: disconnect, clear keychain if !remember_me, return to login screen
    if !app.settings.remember_me && !app.settings.last_jid.is_empty() {
        config::delete_password(&app.settings.last_jid);
    }
    // Remove the active account from the state manager so it
    // doesn't linger as a stale entry on re-login.
    if let Some(id) = app.account_state_mgr.active_id().cloned() {
        app.account_state_mgr.remove_account(&id);
    }
    // Reset own presence
    app.own_presence = crate::xmpp::modules::presence_machine::PresenceStatus::Available;
    let login = LoginScreen::with_saved(
        app.settings.last_jid.clone(),
        String::new(),
        app.settings.last_server.clone(),
        app.settings.remember_me,
    );
    app.screen = Screen::Login(login);
    if let Some(ref tx) = app.xmpp_tx {
        let tx = tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(XmppCommand::Disconnect).await;
        });
    }
    Task::none()
}
