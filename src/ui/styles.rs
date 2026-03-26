#![allow(dead_code)]
// Link-style button: looks like a hyperlink (blue text, no background/border).
//
// Usage: `button(text("Click")).style(link_style)`

use iced::widget::button;
use iced::{Background, Border, Color};

use super::palette;

/// Canonical link/URL color used across the UI.
pub const LINK_COLOR: Color = Color::from_rgb(0.3, 0.5, 1.0);

/// Link button color (slightly different from LINK_COLOR for interactive use).
const LINK_BTN: Color = Color::from_rgb(0.29, 0.62, 1.0); // #4A9EFF
const LINK_BTN_PRESSED: Color = Color::from_rgb(0.22, 0.50, 0.88); // pressed tint

pub fn link_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: None,
            text_color: LINK_BTN,
            border: Border::default(),
            shadow: Default::default(),
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color {
                a: 0.1,
                ..LINK_BTN
            })),
            text_color: LINK_BTN,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color {
                a: 0.15,
                ..LINK_BTN
            })),
            text_color: LINK_BTN_PRESSED,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Disabled => button::Style {
            background: None,
            text_color: Color {
                a: 0.4,
                ..LINK_BTN
            },
            border: Border::default(),
            shadow: Default::default(),
        },
    }
}

/// Small cancel/remove button: subtle background with red hover tint.
///
/// Usage: `button(text("x")).style(cancel_btn_style)`
pub fn cancel_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(Color {
                a: 0.15,
                ..palette::MUTED_TEXT
            })),
            text_color: palette::QUOTE_TEXT,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.9, 0.3, 0.3, 0.25))),
            text_color: Color::from_rgb(0.95, 0.3, 0.3),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.9, 0.3, 0.3, 0.35))),
            text_color: palette::DANGER_RED,
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Disabled => button::Style {
            background: None,
            text_color: Color {
                a: 0.3,
                ..palette::MUTED_TEXT
            },
            border: Border::default(),
            shadow: Default::default(),
        },
    }
}
