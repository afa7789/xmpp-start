// R2: Enhanced link preview rendering helpers.
//
// This module provides `render_preview_card` — an improved version of the
// inline preview card shown beneath messages that contain URLs.
//
// ## Changes required in conversation.rs
//
// To integrate this module, make ONE change in conversation.rs:
//
//   1. At the top, add:
//        use crate::ui::link_preview::render_preview_card;
//
//   2. Replace the existing `render_preview_card` function definition at the
//      bottom of conversation.rs with a call to this module's version, OR
//      simply delete the function there and add the import above.
//
// The improved card shows:
//   - Site name (muted, small)
//   - Bold title
//   - Description capped at 150 chars
//   - OGP image rendered at proper aspect-ratio dimensions (max 300 px wide)
//     instead of the raw URL text shown previously.

use iced::{
    font,
    widget::{column, container, image, row, text},
    Alignment, Element, Font, Length,
};

use crate::ui::palette;
use crate::xmpp::modules::link_preview::LinkPreview;

// We need to refer to the conversation Message type. Import it via pub use so
// callers don't need an extra import when using this helper.
pub use crate::ui::conversation::Message;

const MAX_PREVIEW_WIDTH: u32 = 300;

/// Render an OGP link-preview card constrained to `MAX_PREVIEW_WIDTH` pixels.
///
/// If `image_handle` is provided the OGP image is rendered at the correct
/// aspect-ratio dimensions; otherwise the image section is omitted.
pub fn render_preview_card(
    preview: LinkPreview,
    own: bool,
    image_handle: Option<iced::widget::image::Handle>,
) -> Element<'static, Message> {
    let mut card_col: iced::widget::Column<Message> = column![].spacing(4).padding([8, 10]);

    if let Some(ref site_name) = preview.site_name {
        card_col = card_col.push(text(site_name.clone()).size(10).color(palette::MUTED_TEXT));
    }

    if let Some(ref title) = preview.title {
        card_col = card_col.push(text(title.clone()).size(13).font(Font {
            weight: font::Weight::Bold,
            ..Font::DEFAULT
        }));
    }

    if let Some(ref desc) = preview.description {
        let desc_text: String = desc.chars().take(150).collect();
        card_col = card_col.push(text(desc_text).size(12));
    }

    // R2: render the OGP image at proper aspect-ratio dimensions.
    if let Some(handle) = image_handle {
        let (display_w, display_h) = preview.display_dimensions(MAX_PREVIEW_WIDTH);
        let img_widget = if let Some(h) = display_h {
            image(handle).width(display_w as f32).height(h as f32)
        } else {
            image(handle).width(display_w as f32)
        };
        card_col = card_col.push(img_widget);
    } else if let Some(ref image_url) = preview.image_url {
        // Fallback: show URL as link-coloured text if the image hasn't loaded yet.
        card_col = card_col.push(text(image_url.clone()).size(10).color(palette::LINK_BLUE));
    }

    let card = container(card_col)
        .width(MAX_PREVIEW_WIDTH as f32)
        .style(|_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette::SURFACE)),
            border: iced::Border {
                color: palette::BORDER_SUBTLE,
                width: 1.0,
                radius: 2.0.into(),
            },
            ..Default::default()
        });

    let align = if own {
        Alignment::End
    } else {
        Alignment::Start
    };
    container(card).width(Length::Fill).align_x(align).into()
}

/// Build a row displaying the domain name and optional favicon placeholder.
/// This is a small helper that can be composed into other widgets.
pub fn domain_label(url: &str) -> Element<'static, Message> {
    let domain = extract_domain(url).unwrap_or(url);
    row![text(domain.to_string()).size(10).color(palette::MUTED_TEXT)].into()
}

fn extract_domain(url: &str) -> Option<&str> {
    // Strip scheme (http:// or https://)
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // Take up to the first '/' or end of string
    Some(without_scheme.split('/').next().unwrap_or(without_scheme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_domain_https() {
        assert_eq!(
            extract_domain("https://www.example.com/path/to/page"),
            Some("www.example.com")
        );
    }

    #[test]
    fn extract_domain_http() {
        assert_eq!(extract_domain("http://example.com"), Some("example.com"));
    }

    #[test]
    fn extract_domain_no_scheme() {
        assert_eq!(extract_domain("not-a-url"), None);
    }
}
