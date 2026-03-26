// OMEMO trust/verification UI (XEP-0384)
//
// Provides:
//   - OmemoTrustScreen: contact device fingerprint list with trust toggle buttons
//   - OwnDeviceInfo: own device ID, fingerprint, and session count panel
//   - encryption_badge: lock/unlock badge helper
//   - trust_color: color for each TrustState
//   - format_fingerprint: hex fingerprint with spaces every 8 chars
//
// This module is wired into App as a modal overlay triggered by OpenOmemoTrust.

use iced::{
    widget::{button, column, container, row, scrollable, text, Space},
    Alignment, Color, Element, Length,
};

use crate::xmpp::modules::omemo::store::TrustState;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single device entry shown in the trust screen.
#[derive(Debug, Clone)]
pub struct DeviceEntry {
    /// OMEMO device ID.
    pub device_id: u32,
    /// Raw identity key bytes (used to compute the displayed fingerprint).
    pub identity_key: Vec<u8>,
    /// Current trust classification.
    pub trust: TrustState,
    /// Optional human-readable label (e.g. "Desktop", "Phone").
    pub label: Option<String>,
    /// Whether the device is currently active on the server device list.
    pub active: bool,
}

/// Data needed to display the own device info panel.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct OwnDeviceData {
    pub device_id: u32,
    pub identity_key: Vec<u8>,
    /// Number of active Olm sessions (outbound).
    pub active_session_count: usize,
}

// ---------------------------------------------------------------------------
// OmemoTrustScreen
// ---------------------------------------------------------------------------

/// Screen showing OMEMO device trust info for a contact.
#[derive(Debug, Clone)]
pub struct OmemoTrustScreen {
    pub contact_jid: String,
    pub devices: Vec<DeviceEntry>,
    /// When false, only active devices are shown.
    pub show_inactive: bool,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Message {
    TrustDevice(u32),
    UntrustDevice(u32),
    ToggleShowInactive,
    Close,
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

pub enum Action {
    None,
    TrustDevice { jid: String, device_id: u32 },
    UntrustDevice { jid: String, device_id: u32 },
    Close,
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

impl OmemoTrustScreen {
    pub fn new(contact_jid: impl Into<String>, devices: Vec<DeviceEntry>) -> Self {
        Self {
            contact_jid: contact_jid.into(),
            devices,
            show_inactive: false,
        }
    }

    /// Update trust state optimistically in local state.
    pub fn update(&mut self, msg: Message) -> Action {
        match msg {
            Message::TrustDevice(id) => {
                if let Some(dev) = self.devices.iter_mut().find(|d| d.device_id == id) {
                    dev.trust = TrustState::Trusted;
                }
                Action::TrustDevice {
                    jid: self.contact_jid.clone(),
                    device_id: id,
                }
            }
            Message::UntrustDevice(id) => {
                if let Some(dev) = self.devices.iter_mut().find(|d| d.device_id == id) {
                    dev.trust = TrustState::Untrusted;
                }
                Action::UntrustDevice {
                    jid: self.contact_jid.clone(),
                    device_id: id,
                }
            }
            Message::ToggleShowInactive => {
                self.show_inactive = !self.show_inactive;
                Action::None
            }
            Message::Close => Action::Close,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let title = text(format!("Fingerprint Management — {}", self.contact_jid)).size(18);

        // Count inactive devices for the toggle label.
        let inactive_count = self.devices.iter().filter(|d| !d.active).count();

        // Filter devices based on the show_inactive toggle.
        let visible_devices: Vec<&DeviceEntry> = self
            .devices
            .iter()
            .filter(|d| d.active || self.show_inactive)
            .collect();

        let device_rows: Vec<Element<Message>> =
            visible_devices.iter().map(|dev| device_row(dev)).collect();

        let list = device_rows
            .into_iter()
            .fold(column![].spacing(12), iced::widget::Column::push);

        let list_scroll = scrollable(list).height(Length::Fill);

        // "Show Inactive Devices" toggle row.
        let toggle_label = if self.show_inactive {
            format!("Hide Inactive Devices ({inactive_count})")
        } else {
            format!("Show Inactive Devices ({inactive_count})")
        };
        let toggle_btn = button(text(toggle_label).size(12))
            .on_press(Message::ToggleShowInactive)
            .padding([4, 12]);

        let close_btn = button("Close").on_press(Message::Close).padding([8, 24]);

        let bottom_row = row![toggle_btn, Space::with_width(Length::Fill), close_btn]
            .spacing(8)
            .align_y(Alignment::Center);

        let content = column![
            title,
            Space::with_height(Length::Fixed(12.0)),
            list_scroll,
            Space::with_height(Length::Fixed(12.0)),
            bottom_row,
        ]
        .spacing(8)
        .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

// ---------------------------------------------------------------------------
// Own device info panel
// ---------------------------------------------------------------------------

/// Panel shown in settings with own identity key and session count.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OwnDeviceInfo {
    pub data: OwnDeviceData,
}

#[allow(dead_code)]
impl OwnDeviceInfo {
    pub fn new(data: OwnDeviceData) -> Self {
        Self { data }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let header = text("Your Device").size(16);

        let id_row = row![
            text("Device ID:").size(13).width(Length::Fixed(140.0)),
            text(self.data.device_id.to_string()).size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let fp_row = row![
            text("Fingerprint:").size(13).width(Length::Fixed(140.0)),
            text(format_fingerprint(&self.data.identity_key)).size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Start);

        let sessions_row = row![
            text("Active sessions:")
                .size(13)
                .width(Length::Fixed(140.0)),
            text(self.data.active_session_count.to_string()).size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // QR code placeholder button (disabled for now)
        let qr_btn = button("Show QR Code").padding([6, 16]);

        let content = column![header, id_row, fp_row, sessions_row, qr_btn].spacing(10);

        container(content).padding(12).into()
    }
}

// ---------------------------------------------------------------------------
// Helper: single device row
// ---------------------------------------------------------------------------

fn device_row(dev: &DeviceEntry) -> Element<'_, Message> {
    let label_text = dev.label.as_deref().unwrap_or("Unknown device");

    let label = text(label_text).size(13);
    let device_id_text = text(format!("Device ID: {}", dev.device_id)).size(11);
    let fp_text = text(format!("Fingerprint: {}", format_fingerprint(&dev.identity_key))).size(11);

    // Active/inactive indicator.
    let activity_badge: Element<Message> = if dev.active {
        text("Active")
            .size(10)
            .color(Color::from_rgb(0.20, 0.75, 0.35))
            .into()
    } else {
        text("Inactive")
            .size(10)
            .color(Color::from_rgb(0.55, 0.55, 0.55))
            .into()
    };

    let trust = trust_badge(&dev.trust);

    // Trust toggle button depends on current state.
    let toggle_btn: Element<Message> = match dev.trust {
        TrustState::Trusted | TrustState::Tofu => button(text("Distrust").size(12))
            .on_press(Message::UntrustDevice(dev.device_id))
            .padding([4, 10])
            .into(),
        TrustState::Untrusted | TrustState::Undecided => button(text("Trust").size(12))
            .on_press(Message::TrustDevice(dev.device_id))
            .padding([4, 10])
            .into(),
    };

    let status_row = row![trust, Space::with_width(Length::Fixed(8.0)), activity_badge]
        .align_y(Alignment::Center);

    let left = column![label, device_id_text, fp_text].spacing(2);

    let right = column![status_row, toggle_btn]
        .spacing(4)
        .align_x(Alignment::End);

    row![left.width(Length::Fill), right]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Format raw key bytes as uppercase hex grouped in 8-character blocks.
///
/// Example: `ABCD1234 EF012345 …`
pub fn format_fingerprint(key_bytes: &[u8]) -> String {
    if key_bytes.is_empty() {
        return "(no key)".to_string();
    }
    let hex: String = key_bytes.iter().map(|b| format!("{:02X}", b)).collect();
    hex.as_bytes()
        .chunks(8)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Return the color associated with a trust state for UI badges/indicators.
pub fn trust_color(state: &TrustState) -> Color {
    match state {
        TrustState::Trusted => Color::from_rgb(0.20, 0.75, 0.35), // green
        TrustState::Tofu => Color::from_rgb(0.90, 0.70, 0.10),    // yellow/amber
        TrustState::Untrusted => Color::from_rgb(0.85, 0.25, 0.25), // red
        TrustState::Undecided => Color::from_rgb(0.55, 0.55, 0.55), // gray
    }
}

/// Returns a small lock or unlock text badge indicating encryption state.
pub fn encryption_badge<'a, M: 'a + Clone>(is_encrypted: bool) -> Element<'a, M> {
    let (label, _color) = if is_encrypted {
        ("Encrypted", Color::from_rgb(0.20, 0.75, 0.35))
    } else {
        ("Unencrypted", Color::from_rgb(0.85, 0.25, 0.25))
    };
    text(label).size(11).into()
}

// ---------------------------------------------------------------------------
// Private: trust badge text element
// ---------------------------------------------------------------------------

fn trust_badge(state: &TrustState) -> Element<'_, Message> {
    let (label, color) = match state {
        TrustState::Trusted => ("Trusted", trust_color(state)),
        TrustState::Tofu => ("TOFU", trust_color(state)),
        TrustState::Untrusted => ("Untrusted", trust_color(state)),
        TrustState::Undecided => ("Undecided", trust_color(state)),
    };
    text(label).size(11).color(color).into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_fingerprint_empty() {
        assert_eq!(format_fingerprint(&[]), "(no key)");
    }

    #[test]
    fn format_fingerprint_exact_block() {
        // 4 bytes = 8 hex chars = 1 block, no space
        let fp = format_fingerprint(&[0xAB, 0xCD, 0x12, 0x34]);
        assert_eq!(fp, "ABCD1234");
    }

    #[test]
    fn format_fingerprint_two_blocks() {
        // 8 bytes = 16 hex chars = 2 blocks separated by space
        let fp = format_fingerprint(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]);
        assert_eq!(fp, "00112233 44556677");
    }

    #[test]
    fn format_fingerprint_partial_last_block() {
        // 5 bytes = 10 hex chars = "AABBCCDD" + " EE"
        let fp = format_fingerprint(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
        assert_eq!(fp, "AABBCCDD EE");
    }

    #[test]
    fn trust_color_trusted_is_greenish() {
        let c = trust_color(&TrustState::Trusted);
        // Green channel dominant
        assert!(c.g > c.r);
        assert!(c.g > c.b);
    }

    #[test]
    fn trust_color_untrusted_is_reddish() {
        let c = trust_color(&TrustState::Untrusted);
        assert!(c.r > c.g);
        assert!(c.r > c.b);
    }

    #[test]
    fn trust_color_tofu_is_yellowish() {
        let c = trust_color(&TrustState::Tofu);
        // Both red and green channels high, blue low
        assert!(c.r > 0.5);
        assert!(c.g > 0.5);
        assert!(c.b < 0.3);
    }

    #[test]
    fn trust_color_undecided_is_gray() {
        let c = trust_color(&TrustState::Undecided);
        let diff_rg = (c.r - c.g).abs();
        let diff_rb = (c.r - c.b).abs();
        // All channels close to each other
        assert!(diff_rg < 0.05);
        assert!(diff_rb < 0.05);
    }

    #[test]
    fn format_fingerprint_32_bytes() {
        let bytes: Vec<u8> = (0u8..32).collect();
        let fp = format_fingerprint(&bytes);
        // 32 bytes = 64 hex chars = 8 blocks of 8, 7 spaces between them
        let blocks: Vec<&str> = fp.split(' ').collect();
        assert_eq!(blocks.len(), 8);
        for block in &blocks {
            assert_eq!(block.len(), 8);
        }
    }
}
