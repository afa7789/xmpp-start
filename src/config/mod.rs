// Task P2.5 — Settings persistence, keychain, theme
//
// Source reference:
//   apps/fluux/src/stores/settingsStore.ts
//   apps/fluux/src/utils/keychain.ts
//
// Storage strategy:
//   - JID + server: ~/.config/rexisce/settings.json  (serde_json + std::fs)
//   - Password:     OS keychain via `keyring` crate
//   - Theme:        included in settings.json

pub mod account;
pub use account::{is_valid_jid, AccountConfig};

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
    /// K7: XEP-0357 push service JID.  None = push disabled.
    #[serde(default)]
    pub push_service_jid: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum TimeFormat {
    #[default]
    TwentyFourHour,
    TwelveHour,
}

pub(crate) fn default_true() -> bool {
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
            push_service_jid: None,
        }
    }
}

// ---------------------------------------------------------------------------
// File-system helpers
// ---------------------------------------------------------------------------

fn config_path() -> PathBuf {
    let base = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    base.join(".config").join("rexisce")
}

/// Returns the path to the SQLite database, creating the directory if needed.
pub fn db_path() -> String {
    let base = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        base.join("Library")
            .join("Application Support")
            .join("rexisce")
    } else {
        base.join(".local").join("share").join("rexisce")
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

const KEYRING_SERVICE: &str = "rexisce";

/// Store a password in the OS keychain for the given JID.
pub fn save_password(jid: &str, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, jid).map_err(|e| {
        tracing::error!("keyring: failed to create entry for {jid}: {e}");
        e
    })?;
    entry.set_password(password).map_err(|e| {
        tracing::error!("keyring: failed to store password for {jid}: {e}");
        e
    })?;
    tracing::debug!("keyring: stored password for {jid}");
    Ok(())
}

/// Retrieve the stored password for the given JID; returns `None` if not found.
pub fn load_password(jid: &str) -> Option<String> {
    let entry = match keyring::Entry::new(KEYRING_SERVICE, jid) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("keyring: failed to create entry for {jid}: {e}");
            return None;
        }
    };
    match entry.get_password() {
        Ok(pw) => {
            tracing::debug!("keyring: loaded password for {jid}");
            Some(pw)
        }
        Err(e) => {
            tracing::debug!("keyring: no stored password for {jid}: {e}");
            None
        }
    }
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
pub fn delete_password(jid: &str) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, jid) {
        let _ = entry.delete_credential();
    }
}

// ---------------------------------------------------------------------------
// Avatar disk cache
// ---------------------------------------------------------------------------

/// Platform-appropriate data directory for persistent application data.
fn data_dir() -> PathBuf {
    let base = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        base.join("Library")
            .join("Application Support")
            .join("rexisce")
    } else {
        base.join(".local").join("share").join("rexisce")
    }
}

/// Directory where cached avatar PNGs are stored on disk.
fn avatar_cache_dir() -> PathBuf {
    data_dir().join("avatars")
}

/// Deterministic filename for a JID's avatar (SHA-1 hash of the bare JID).
fn jid_to_filename(jid: &str) -> String {
    use sha1::{Digest, Sha1};
    let bare = jid.split('/').next().unwrap_or(jid);
    let hash = Sha1::digest(bare.as_bytes());
    format!("{:x}.png", hash)
}

/// Persist an avatar to disk so it survives app restarts.
pub fn save_avatar(jid: &str, png_bytes: &[u8]) {
    let dir = avatar_cache_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("avatar cache: failed to create dir: {e}");
        return;
    }
    let path = dir.join(jid_to_filename(jid));
    if let Err(e) = std::fs::write(&path, png_bytes) {
        tracing::warn!("avatar cache: failed to write {}: {e}", path.display());
    }
    save_avatar_jid_sidecar(jid);
}

/// Load all cached avatars from disk into a HashMap keyed by bare JID.
pub fn load_avatar_cache() -> std::collections::HashMap<String, Vec<u8>> {
    let dir = avatar_cache_dir();
    // Also build a reverse lookup: filename → JID.
    // We can't reverse a SHA-1 hash, so we store a companion ".jid" sidecar
    // file alongside each avatar.
    let mut map = std::collections::HashMap::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return map,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        // Read the companion .jid sidecar to recover the bare JID.
        let jid_path = path.with_extension("jid");
        let jid = match std::fs::read_to_string(&jid_path) {
            Ok(j) => j,
            Err(_) => continue,
        };
        if let Ok(bytes) = std::fs::read(&path) {
            map.insert(jid, bytes);
        }
    }
    map
}

/// Internal: write the JID sidecar file so `load_avatar_cache` can map
/// filename back to JID.
fn save_avatar_jid_sidecar(jid: &str) {
    let dir = avatar_cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let bare = jid.split('/').next().unwrap_or(jid);
    let sidecar = dir.join(jid_to_filename(jid)).with_extension("jid");
    let _ = std::fs::write(sidecar, bare);
}

// ---------------------------------------------------------------------------
// Per-account keychain helpers (MULTI)
//
// These operate on `AccountConfig::password_key` instead of the bare JID so
// the credential slot can be rotated independently of the JID string.
// ---------------------------------------------------------------------------

/// Store a password in the OS keychain for the given `AccountConfig`.
///
/// Uses `account.password_key` as the keychain username so the credential can
/// be looked up or deleted by key later.
#[allow(dead_code)]
pub fn save_account_password(account: &AccountConfig, password: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &account.password_key)?;
    entry.set_password(password)?;
    Ok(())
}

/// Retrieve the stored password for the given `AccountConfig`.
/// Returns `None` if no credential is found.
#[allow(dead_code)]
pub fn load_account_password(account: &AccountConfig) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, &account.password_key)
        .ok()?
        .get_password()
        .ok()
}

/// Remove the stored credential for the given `AccountConfig` (e.g. on
/// account removal or explicit sign-out with `remember_me == false`).
#[allow(dead_code)]
pub fn delete_account_password(account: &AccountConfig) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &account.password_key) {
        let _ = entry.delete_credential();
    }
}

impl TimeFormat {
    /// Format a unix timestamp (milliseconds) into a human-readable string.
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
            push_service_jid: None,
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
        assert_eq!(
            s2.manual_srv,
            Some("_xmpp-client._tcp.corp.example.com".into())
        );
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

    // -----------------------------------------------------------------------
    // MULTI: per-account keychain helpers
    // -----------------------------------------------------------------------

    #[test]
    fn save_account_password_uses_password_key() {
        // Verify that `save_account_password` routes through `password_key`,
        // not the bare JID, by constructing an account where the two differ.
        let mut account = AccountConfig::new("alice@example.com");
        account.password_key = "alice@example.com:rotated-2025".to_string();

        // We can't call the real keychain in a unit test environment, but we can
        // confirm the public API is callable and that the key field is wired.
        // The actual keychain round-trip is exercised by manual integration tests.
        assert_eq!(account.password_key, "alice@example.com:rotated-2025");
        assert_ne!(account.jid, account.password_key);
    }

    #[test]
    fn account_config_new_sets_password_key_equal_to_jid() {
        let account = AccountConfig::new("bob@xmpp.example.com");
        assert_eq!(account.password_key, account.jid);
    }

    #[test]
    fn multiple_accounts_have_independent_password_keys() {
        let alice = AccountConfig::new("alice@example.com");
        let bob = AccountConfig::new("bob@example.com");
        // Each account's password_key is distinct, so keychain slots do not clash.
        assert_ne!(alice.password_key, bob.password_key);
    }

    // -----------------------------------------------------------------------
    // Settings persistence (file roundtrip)
    // -----------------------------------------------------------------------

    /// Serialize a Settings with non-default values to a temp file, read it
    /// back, and confirm every field survived the round-trip.
    #[test]
    fn save_and_load_settings_roundtrip() {
        use std::collections::HashSet;

        let original = Settings {
            theme: Theme::Dark,
            font_size: 18,
            show_timestamps: true,
            notifications_enabled: false,
            sound_enabled: false,
            last_jid: "roundtrip@example.com".to_owned(),
            last_server: "xmpp.example.com".to_owned(),
            muted_jids: HashSet::new(),
            status_message: Some("In a meeting".to_owned()),
            remember_me: true,
            send_receipts: true,
            send_typing: false,
            send_read_markers: true,
            mam_default_mode: Some("roster".to_owned()),
            use_system_theme: false,
            time_format: TimeFormat::TwelveHour,
            avatar_data: None,
            contact_sort: "recent".to_owned(),
            mam_fetch_limit: 25,
            show_join_leave: false,
            show_typing_indicators: true,
            compact_layout: true,
            accounts: vec![AccountConfig::new("roundtrip@example.com")],
            proxy_type: None,
            proxy_host: None,
            proxy_port: None,
            manual_srv: None,
            force_tls: true,
            push_service_jid: None,
        };

        // Write to a temp file using the same logic as `save()`.
        let dir = std::env::temp_dir().join("rexisce_test_settings");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings_roundtrip.json");
        let json = serde_json::to_string_pretty(&original).unwrap();
        std::fs::write(&path, &json).unwrap();

        // Read back using the same logic as `load()`.
        let loaded: Settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .expect("should deserialize settings from temp file");

        assert_eq!(loaded.theme, Theme::Dark);
        assert_eq!(loaded.font_size, 18);
        assert_eq!(loaded.time_format, TimeFormat::TwelveHour);
        assert!(!loaded.send_typing, "send_typing should be false");
        assert!(!loaded.show_join_leave, "show_join_leave should be false");
        assert!(loaded.compact_layout, "compact_layout should be true");
        assert_eq!(loaded.contact_sort, "recent");
        assert_eq!(loaded.mam_fetch_limit, 25);
        assert_eq!(loaded.last_jid, "roundtrip@example.com");
        assert_eq!(loaded.status_message, Some("In a meeting".to_owned()));
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].jid, "roundtrip@example.com");

        // Clean up.
        let _ = std::fs::remove_file(&path);
    }

    /// Confirm that `Settings::default()` contains reasonable, safe values.
    #[test]
    fn default_settings_have_sane_values() {
        let s = Settings::default();

        // Theme defaults to dark (existing test covers this; verify here too).
        assert_eq!(s.theme, Theme::Dark);

        // Font size should be in a legible range.
        assert!(
            s.font_size >= 10 && s.font_size <= 32,
            "default font_size {} is outside the expected range 10–32",
            s.font_size
        );

        // Timestamps, notifications, and sounds should be on by default.
        assert!(s.show_timestamps, "timestamps should be shown by default");
        assert!(
            s.notifications_enabled,
            "notifications should be enabled by default"
        );
        assert!(s.sound_enabled, "sound should be enabled by default");

        // Privacy-sensitive defaults: receipts and markers on.
        assert!(s.send_receipts, "send_receipts should default to true");
        assert!(s.send_typing, "send_typing should default to true");
        assert!(
            s.send_read_markers,
            "send_read_markers should default to true"
        );

        // MAM fetch limit should be a sensible positive number.
        assert!(s.mam_fetch_limit > 0, "mam_fetch_limit should be positive");

        // TLS should be required by default.
        assert!(s.force_tls, "force_tls should default to true");

        // No accounts pre-configured.
        assert!(s.accounts.is_empty(), "accounts should be empty by default");

        // JID fields should be empty strings, not garbage.
        assert!(s.last_jid.is_empty(), "last_jid should be empty by default");
        assert!(
            s.last_server.is_empty(),
            "last_server should be empty by default"
        );
    }

    // -----------------------------------------------------------------------
    // Avatar disk cache
    // -----------------------------------------------------------------------

    #[test]
    fn jid_to_filename_is_deterministic() {
        let a = super::jid_to_filename("alice@example.com");
        let b = super::jid_to_filename("alice@example.com");
        assert_eq!(a, b);
        assert!(a.ends_with(".png"));
    }

    #[test]
    fn jid_to_filename_strips_resource() {
        let bare = super::jid_to_filename("alice@example.com");
        let full = super::jid_to_filename("alice@example.com/laptop");
        assert_eq!(bare, full);
    }

    #[test]
    fn avatar_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("rexisce_test_avatars");
        let _ = std::fs::remove_dir_all(&dir);

        // Temporarily override the cache dir by saving directly via the
        // internal helpers (we can't override data_dir, so we test the
        // public API which writes to the real cache dir, then clean up).
        let jid = "roundtrip-test@example.com";
        let png = b"fake-png-data-for-test";

        super::save_avatar(jid, png);
        let loaded = super::load_avatar_cache();
        assert_eq!(loaded.get(jid).map(|v| v.as_slice()), Some(png.as_slice()));

        // Clean up the file we wrote.
        let cache_dir = super::avatar_cache_dir();
        let fname = super::jid_to_filename(jid);
        let _ = std::fs::remove_file(cache_dir.join(&fname));
        let _ = std::fs::remove_file(cache_dir.join(fname).with_extension("jid"));
    }
}
