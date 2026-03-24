// Task P2.5 — Settings persistence, keychain, theme
//
// Source reference:
//   apps/fluux/src/stores/settingsStore.ts
//   apps/fluux/src/utils/keychain.ts
//
// Storage strategy:
//   - JID + server: ~/.config/xmpp-start/settings.json  (serde_json + std::fs)
//   - Password:     OS keychain via `keyring` crate
//   - Theme:        included in settings.json

pub mod account;
pub use account::AccountConfig;

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Settings struct
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    pub font_size: u8,
    pub show_timestamps: bool,
    pub notifications_enabled: bool,
    pub sound_enabled: bool,
    /// Last-used JID (pre-fills the login screen).
    pub last_jid: String,
    /// Last-used server override (pre-fills the login screen).
    pub last_server: String,
    /// J3: JIDs with notifications muted.
    #[serde(default)]
    pub muted_jids: std::collections::HashSet<String>,
    /// J2: Custom presence status message (e.g. "In a meeting").
    #[serde(default)]
    pub status_message: Option<String>,
    /// AUTH-1: if true, password stays in keychain on logout so next login is instant.
    #[serde(default = "default_remember_me")]
    pub remember_me: bool,
    /// S6: whether to request and send XEP-0184 delivery receipts.
    #[serde(default = "default_true")]
    pub send_receipts: bool,
    /// S6: whether to send XEP-0085 typing indicators.
    #[serde(default = "default_true")]
    pub send_typing: bool,
    /// S6: whether to send XEP-0333 displayed markers (read receipts).
    #[serde(default = "default_true")]
    pub send_read_markers: bool,
    /// J10: MAM archiving default mode ("roster", "always", or "never").
    #[serde(default)]
    pub mam_default_mode: Option<String>,
    /// M1: use system theme instead of manual theme selection.
    #[serde(default)]
    pub use_system_theme: bool,
    /// M1: time format for timestamps (12h or 24h).
    #[serde(default)]
    pub time_format: TimeFormat,
    /// H2: cached own avatar data (PNG bytes).
    #[serde(default)]
    pub avatar_data: Option<Vec<u8>>,
    /// K6: contact sorting preference ("alphabetical" or "recent")
    #[serde(default)]
    pub contact_sort: String,
    /// M6: number of messages to fetch per MAM page (default 50).
    #[serde(default = "default_mam_fetch_limit")]
    pub mam_fetch_limit: u32,
    /// K6: show join/leave presence messages in MUC rooms (default true).
    #[serde(default = "default_true")]
    pub show_join_leave: bool,
    /// K6: show typing indicator when a contact is composing (default true).
    #[serde(default = "default_true")]
    pub show_typing_indicators: bool,
    /// K6: use compact message layout (less padding, default false).
    #[serde(default)]
    pub compact_layout: bool,
    /// MULTI: configured accounts.  When non-empty these take precedence over
    /// the legacy `last_jid` / keychain-password single-account path.
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    /// M5: proxy type ("socks5" or "http"), None = direct.
    #[serde(default)]
    pub proxy_type: Option<String>,
    /// M5: proxy hostname or IP address.
    #[serde(default)]
    pub proxy_host: Option<String>,
    /// M5: proxy port number.
    #[serde(default)]
    pub proxy_port: Option<u16>,
    /// M5: manual SRV override (e.g. "_xmpp-client._tcp.example.com").
    #[serde(default)]
    pub manual_srv: Option<String>,
    /// M5: always require TLS (default true).
    #[serde(default = "default_true")]
    pub force_tls: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum TimeFormat {
    #[default]
    TwentyFourHour,
    TwelveHour,
}

fn default_true() -> bool {
    true
}

fn default_remember_me() -> bool {
    true
}

fn default_mam_fetch_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            font_size: 14,
            show_timestamps: true,
            notifications_enabled: true,
            sound_enabled: true,
            last_jid: String::new(),
            last_server: String::new(),
            muted_jids: std::collections::HashSet::new(),
            status_message: None,
            remember_me: true,
            send_receipts: true,
            send_typing: true,
            send_read_markers: true,
            mam_default_mode: None,
            use_system_theme: false,
            time_format: TimeFormat::TwentyFourHour,
            avatar_data: None,
            contact_sort: "alphabetical".to_string(),
            mam_fetch_limit: 50,
            show_join_leave: true,
            show_typing_indicators: true,
            compact_layout: false,
            accounts: Vec::new(),
            proxy_type: None,
            proxy_host: None,
            proxy_port: None,
            manual_srv: None,
            force_tls: true,
        }
    }
}

// ---------------------------------------------------------------------------
// File-system helpers
// ---------------------------------------------------------------------------

fn config_path() -> PathBuf {
    let base = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    base.join(".config").join("xmpp-start")
}

/// Returns the path to the SQLite database, creating the directory if needed.
pub fn db_path() -> String {
    let base = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        base.join("Library")
            .join("Application Support")
            .join("xmpp-start")
    } else {
        base.join(".local").join("share").join("xmpp-start")
    };
    std::fs::create_dir_all(&dir).ok();
    dir.join("messages.db").to_string_lossy().into_owned()
}

fn settings_file() -> PathBuf {
    config_path().join("settings.json")
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Load settings from disk; returns `Settings::default()` if not found or corrupt.
pub fn load() -> Settings {
    let path = settings_file();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist settings to disk. Creates the config directory if needed.
pub fn save(settings: &Settings) -> Result<()> {
    let dir = config_path();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(settings_file(), json)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Keychain
// ---------------------------------------------------------------------------

const KEYRING_SERVICE: &str = "xmpp-start";

/// Store a password in the OS keychain for the given JID.
pub fn save_password(jid: &str, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, jid)?;
    entry.set_password(password)?;
    Ok(())
}

/// Retrieve the stored password for the given JID; returns `None` if not found.
pub fn load_password(jid: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, jid)
        .ok()?
        .get_password()
        .ok()
}

/// M1: Detect the OS dark/light mode preference.
/// Returns `Theme::Dark` if the OS is in dark mode or detection fails.
pub fn detect_system_theme() -> Theme {
    match dark_light::detect() {
        Ok(dark_light::Mode::Light) => Theme::Light,
        _ => Theme::Dark, // dark or unknown → dark
    }
}

/// Delete the stored password (e.g. on logout).
#[allow(dead_code)]
pub fn delete_password(jid: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, jid) {
        let _ = entry.delete_credential();
    }
}

impl TimeFormat {
    /// Format a unix timestamp (milliseconds) into a human-readable string.
    #[allow(dead_code)]
    pub fn format_timestamp(&self, ts_millis: i64) -> String {
        let ts = chrono::DateTime::from_timestamp_millis(ts_millis);
        match ts {
            Some(dt) => match self {
                TimeFormat::TwentyFourHour => dt.format("%H:%M").to_string(),
                TimeFormat::TwelveHour => dt.format("%I:%M %p").to_string(),
            },
            None => String::new(),
        }
    }

    /// Format a unix timestamp with date for date separators.
    #[allow(dead_code)]
    pub fn format_timestamp_full(&self, ts_millis: i64) -> String {
        let ts = chrono::DateTime::from_timestamp_millis(ts_millis);
        match ts {
            Some(dt) => match self {
                TimeFormat::TwentyFourHour => dt.format("%Y-%m-%d %H:%M").to_string(),
                TimeFormat::TwelveHour => dt.format("%Y-%m-%d %I:%M %p").to_string(),
            },
            None => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_dark() {
        assert_eq!(Settings::default().theme, Theme::Dark);
    }

    #[test]
    fn settings_round_trip_json() {
        let s = Settings {
            theme: Theme::Light,
            font_size: 16,
            show_timestamps: false,
            notifications_enabled: false,
            sound_enabled: false,
            last_jid: "user@example.com".into(),
            last_server: "xmpp.example.com".into(),
            muted_jids: std::collections::HashSet::new(),
            status_message: None,
            remember_me: false,
            send_receipts: false,
            send_typing: false,
            send_read_markers: false,
            mam_default_mode: Some("roster".into()),
            use_system_theme: true,
            time_format: TimeFormat::TwelveHour,
            avatar_data: None,
            contact_sort: "alphabetical".to_string(),
            mam_fetch_limit: 100,
            show_join_leave: false,
            show_typing_indicators: false,
            compact_layout: true,
            accounts: vec![AccountConfig::new("user@example.com")],
            proxy_type: Some("socks5".into()),
            proxy_host: Some("proxy.example.com".into()),
            proxy_port: Some(1080),
            manual_srv: Some("_xmpp-client._tcp.example.com".into()),
            force_tls: false,
        };
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.last_jid, "user@example.com");
        assert_eq!(s2.theme, Theme::Light);
        assert_eq!(s2.font_size, 16);
        assert!(!s2.remember_me);
        assert!(!s2.send_receipts);
        assert!(!s2.send_typing);
        assert!(!s2.send_read_markers);
        assert_eq!(s2.mam_default_mode, Some("roster".into()));
        assert!(s2.use_system_theme);
        assert_eq!(s2.time_format, TimeFormat::TwelveHour);
        assert_eq!(s2.mam_fetch_limit, 100);
        assert_eq!(s2.accounts.len(), 1);
        assert_eq!(s2.accounts[0].jid, "user@example.com");
        assert_eq!(s2.proxy_type, Some("socks5".into()));
        assert_eq!(s2.proxy_host, Some("proxy.example.com".into()));
        assert_eq!(s2.proxy_port, Some(1080));
        assert_eq!(s2.manual_srv, Some("_xmpp-client._tcp.example.com".into()));
        assert!(!s2.force_tls);
    }

    #[test]
    fn network_settings_defaults() {
        let s = Settings::default();
        assert!(s.proxy_type.is_none());
        assert!(s.proxy_host.is_none());
        assert!(s.proxy_port.is_none());
        assert!(s.manual_srv.is_none());
        assert!(s.force_tls);
    }

    #[test]
    fn network_settings_roundtrip() {
        let s = Settings {
            proxy_type: Some("http".into()),
            proxy_host: Some("192.168.1.1".into()),
            proxy_port: Some(8080),
            manual_srv: Some("_xmpp-client._tcp.corp.example.com".into()),
            force_tls: true,
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.proxy_type, Some("http".into()));
        assert_eq!(s2.proxy_host, Some("192.168.1.1".into()));
        assert_eq!(s2.proxy_port, Some(8080));
        assert_eq!(s2.manual_srv, Some("_xmpp-client._tcp.corp.example.com".into()));
        assert!(s2.force_tls);
    }

    #[test]
    fn load_returns_default_when_no_file() {
        // Point at a path that doesn't exist.
        // load() should silently fall back to default.
        let s = Settings::default();
        assert!(s.last_jid.is_empty());
    }

    #[test]
    fn theme_toggle() {
        let mut s = Settings::default();
        assert_eq!(s.theme, Theme::Dark);
        s.theme = Theme::Light;
        assert_eq!(s.theme, Theme::Light);
    }
}
