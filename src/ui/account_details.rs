// M4: Account details data
//
// Snapshot of the current account's connection state, consumed by the
// settings panel to render account details inline.

/// Snapshot of the current connection state shown in the settings panel.
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
