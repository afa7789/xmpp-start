// Task P2.5 — Settings, keychain, notifications, theme
//
// Source reference:
//   apps/fluux/src/stores/settingsStore.ts
//   apps/fluux/src/utils/keychain.ts

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme: Theme,
    pub font_size: u8,
    pub show_timestamps: bool,
    pub notifications_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        }
    }
}
