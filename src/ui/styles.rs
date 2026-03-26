#![allow(dead_code)]
// Centralised style constants and reusable style functions.
//
// All border-radius, padding, and container/button helpers live here so that
// the settings screen and main/sidebar screens share a single design language.

use iced::widget::{button, container};
use iced::{Background, Border, Color};

// ---------------------------------------------------------------------------
// Layout constants — single source of truth for spacing & radii
// ---------------------------------------------------------------------------

/// Standard border-radius for cards, panels, and section containers.
pub const RADIUS_CARD: f32 = 8.0;
/// Border-radius for small interactive elements (buttons, segmented controls).
pub const RADIUS_BUTTON: f32 = 6.0;
/// Border-radius for tiny inline controls (cancel badges, copy buttons).
pub const RADIUS_SMALL: f32 = 4.0;

/// Padding inside section/card containers.
pub const PADDING_CARD: u16 = 12;

// ---------------------------------------------------------------------------
// Reusable container style: themed card with weak background + subtle border
// ---------------------------------------------------------------------------

/// Theme-aware section card style (used by settings sections, sidebar panels, etc.)
pub fn card_container_style(theme: &iced::Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: RADIUS_CARD.into(),
        },
        ..Default::default()
    }
}

/// Theme-aware modal/overlay card with primary-tinted border.
pub fn modal_container_style(theme: &iced::Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            color: palette.primary.base.color,
            width: 1.0,
            radius: RADIUS_CARD.into(),
        },
        ..Default::default()
    }
}

/// Canonical link/URL color used across the UI.
pub const LINK_COLOR: Color = Color::from_rgb(0.3, 0.5, 1.0);

pub fn link_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let color = Color::from_rgb(0.29, 0.62, 1.0); // #4A9EFF
    let dark = Color::from_rgb(0.22, 0.50, 0.88); // pressed tint

    match status {
        button::Status::Active => button::Style {
            background: None,
            text_color: color,
            border: Border::default(),
            shadow: Default::default(),
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.29, 0.62, 1.0, 0.1))),
            text_color: color,
            border: Border {
                radius: RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.29, 0.62, 1.0, 0.15))),
            text_color: dark,
            border: Border {
                radius: RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Disabled => button::Style {
            background: None,
            text_color: Color::from_rgba(0.29, 0.62, 1.0, 0.4),
            border: Border::default(),
            shadow: Default::default(),
        },
    }
}

/// Small cancel/remove button: subtle background with red hover tint.
///
/// Usage: `button(text("x")).style(cancel_btn_style)`
pub fn cancel_btn_style(_theme: &iced::Theme, status: button::Status) -> button::Style {
    let text_color = Color::from_rgb(0.6, 0.6, 0.6);
    match status {
        button::Status::Active => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.5, 0.5, 0.5, 0.15))),
            text_color,
            border: Border {
                radius: RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.9, 0.3, 0.3, 0.25))),
            text_color: Color::from_rgb(0.95, 0.3, 0.3),
            border: Border {
                radius: RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color::from_rgba(0.9, 0.3, 0.3, 0.35))),
            text_color: Color::from_rgb(0.85, 0.2, 0.2),
            border: Border {
                radius: RADIUS_SMALL.into(),
                ..Border::default()
            },
            shadow: Default::default(),
        },
        button::Status::Disabled => button::Style {
            background: None,
            text_color: Color::from_rgba(0.5, 0.5, 0.5, 0.3),
            border: Border::default(),
            shadow: Default::default(),
        },
    }
}
