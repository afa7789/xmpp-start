// M4: Account details panel
//
// Displays the current account's JID, server, resource, connection status,
// and optionally server capabilities (from disco#info).

use iced::{
    widget::{column, container, row, text},
    Element, Length,
};

// ---------------------------------------------------------------------------
// Data the caller must supply
// ---------------------------------------------------------------------------

/// Snapshot of the current connection state shown in the panel.
#[derive(Debug, Clone, Default)]
pub struct AccountInfo {
    /// Fully qualified bound JID (e.g. "alice@example.com/resource").
    pub bound_jid: String,
    /// Whether the session is currently online.
    pub connected: bool,
    /// Server capabilities summary (e.g. "MAM, CSI, Push").
    /// Empty string when unknown.
    pub server_features: String,
    /// Authentication mechanism used (e.g. "SCRAM-SHA-256").
    pub auth_method: String,
}

// ---------------------------------------------------------------------------
// Derived fields helpers
// ---------------------------------------------------------------------------

impl AccountInfo {
    fn bare_jid(&self) -> &str {
        self.bound_jid
            .split('/')
            .next()
            .unwrap_or(&self.bound_jid)
    }

    fn server(&self) -> &str {
        self.bare_jid()
            .split('@')
            .nth(1)
            .unwrap_or("")
    }

    fn resource(&self) -> &str {
        self.bound_jid
            .split('/')
            .nth(1)
            .unwrap_or("")
    }
}

// ---------------------------------------------------------------------------
// Messages — none needed (read-only panel)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

pub struct AccountDetailsPanel {
    pub info: AccountInfo,
}

impl AccountDetailsPanel {
    pub fn new(info: AccountInfo) -> Self {
        Self { info }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = text("Account Details").size(16);

        let status_str = if self.info.connected {
            "Connected"
        } else {
            "Offline"
        };

        let rows: Vec<(&str, String)> = vec![
            ("JID", self.info.bare_jid().to_string()),
            ("Server", self.info.server().to_string()),
            ("Resource", self.info.resource().to_string()),
            ("Status", status_str.to_string()),
            (
                "Auth",
                if self.info.auth_method.is_empty() {
                    "—".to_string()
                } else {
                    self.info.auth_method.clone()
                },
            ),
            (
                "Server features",
                if self.info.server_features.is_empty() {
                    "—".to_string()
                } else {
                    self.info.server_features.clone()
                },
            ),
        ];

        let detail_rows: Vec<Element<Message>> = rows
            .into_iter()
            .map(|(label, value)| {
                row![
                    text(label).size(13).width(Length::Fixed(130.0)),
                    text(value).size(13).width(Length::Fill),
                ]
                .spacing(8)
                .into()
            })
            .collect();

        let content = detail_rows
            .into_iter()
            .fold(column![header].spacing(8), iced::widget::Column::push);

        container(content).padding(0).into()
    }
}
