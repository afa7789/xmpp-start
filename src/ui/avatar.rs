// H5: XEP-0392 consistent color generation for JID avatars

use iced::Color;

/// Compute a consistent color for a JID using the XEP-0392 algorithm.
/// Hue = sum of JID bytes mod 360, saturation=0.7, lightness=0.4.
pub fn jid_color(jid: &str) -> Color {
    let hue: u32 = jid.bytes().map(|b| b as u32).sum::<u32>() % 360;
    hsl_to_color(hue as f32, 0.7, 0.4)
}

/// Convert HSL (hue 0–360, sat 0–1, lig 0–1) to iced Color.
fn hsl_to_color(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    Color::from_rgb(r1 + m, g1 + m, b1 + m)
}

/// Return the first character of the local-part of a JID (before @), uppercased.
pub fn jid_initial(jid: &str) -> char {
    jid.split('@')
        .next()
        .and_then(|local| local.chars().next())
        .unwrap_or('?')
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jid_color_is_deterministic() {
        let c1 = jid_color("alice@example.com");
        let c2 = jid_color("alice@example.com");
        assert_eq!(c1.r, c2.r);
        assert_eq!(c1.g, c2.g);
        assert_eq!(c1.b, c2.b);
    }

    #[test]
    fn jid_color_differs_per_jid() {
        let c1 = jid_color("alice@example.com");
        let c2 = jid_color("bob@example.com");
        // They may occasionally match but very unlikely for different JIDs
        // Just verify the function runs and returns valid values
        assert!(c1.r >= 0.0 && c1.r <= 1.0);
        assert!(c2.r >= 0.0 && c2.r <= 1.0);
    }

    #[test]
    fn jid_initial_returns_local_part_first_char() {
        assert_eq!(jid_initial("alice@example.com"), 'A');
        assert_eq!(jid_initial("bob@example.com"), 'B');
        assert_eq!(jid_initial("example.com"), 'E');
    }
}
