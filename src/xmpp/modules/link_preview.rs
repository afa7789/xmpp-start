#![allow(dead_code)]
/// Parsed Open Graph / HTML meta-tag preview for a URL.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinkPreview {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub site_name: Option<String>,
}

/// Parse Open Graph and standard `<meta>` / `<title>` tags from raw HTML.
///
/// Priority order for each field:
///   title:       og:title > twitter:title > `<title>`
///   description: og:description > twitter:description > meta[name=description]
///   image:       og:image > twitter:image
///   site_name:   og:site_name
///
/// This is a minimal line-by-line parser — no external HTML parser dependency.
/// It scans for `<meta` and `<title>` tags using string matching.
pub fn parse_preview(url: &str, html: &str) -> LinkPreview {
    let mut og_title: Option<String> = None;
    let mut tw_title: Option<String> = None;
    let mut html_title: Option<String> = None;

    let mut og_description: Option<String> = None;
    let mut tw_description: Option<String> = None;
    let mut meta_description: Option<String> = None;

    let mut og_image: Option<String> = None;
    let mut tw_image: Option<String> = None;

    let mut og_site_name: Option<String> = None;

    // Normalise to lowercase for attribute matching but keep original for value extraction.
    for line in html.lines() {
        let lower = line.to_lowercase();

        // ---- <meta …> tags ------------------------------------------------
        if lower.contains("<meta") {
            // og:title
            if lower.contains(r#"property="og:title""#) || lower.contains("property='og:title'") {
                if let Some(v) = extract_content(line) {
                    og_title.get_or_insert(v);
                }
            }
            // twitter:title
            if lower.contains(r#"name="twitter:title""#)
                || lower.contains("name='twitter:title'")
                || lower.contains(r#"property="twitter:title""#)
                || lower.contains("property='twitter:title'")
            {
                if let Some(v) = extract_content(line) {
                    tw_title.get_or_insert(v);
                }
            }
            // og:description
            if lower.contains(r#"property="og:description""#)
                || lower.contains("property='og:description'")
            {
                if let Some(v) = extract_content(line) {
                    og_description.get_or_insert(v);
                }
            }
            // twitter:description
            if lower.contains(r#"name="twitter:description""#)
                || lower.contains("name='twitter:description'")
                || lower.contains(r#"property="twitter:description""#)
                || lower.contains("property='twitter:description'")
            {
                if let Some(v) = extract_content(line) {
                    tw_description.get_or_insert(v);
                }
            }
            // meta name=description
            if lower.contains(r#"name="description""#) || lower.contains("name='description'") {
                if let Some(v) = extract_content(line) {
                    meta_description.get_or_insert(v);
                }
            }
            // og:image
            if lower.contains(r#"property="og:image""#) || lower.contains("property='og:image'") {
                if let Some(v) = extract_content(line) {
                    og_image.get_or_insert(v);
                }
            }
            // twitter:image
            if lower.contains(r#"name="twitter:image""#)
                || lower.contains("name='twitter:image'")
                || lower.contains(r#"property="twitter:image""#)
                || lower.contains("property='twitter:image'")
            {
                if let Some(v) = extract_content(line) {
                    tw_image.get_or_insert(v);
                }
            }
            // og:site_name
            if lower.contains(r#"property="og:site_name""#)
                || lower.contains("property='og:site_name'")
            {
                if let Some(v) = extract_content(line) {
                    og_site_name.get_or_insert(v);
                }
            }
        }

        // ---- <title> tag --------------------------------------------------
        if lower.contains("<title>") {
            if let Some(v) = extract_title_tag(line) {
                html_title.get_or_insert(v);
            }
        }
    }

    LinkPreview {
        url: url.to_string(),
        title: og_title.or(tw_title).or(html_title),
        description: og_description.or(tw_description).or(meta_description),
        image_url: og_image.or(tw_image),
        site_name: og_site_name,
    }
}

/// Extract the value of the `content` attribute from a tag string.
/// Handles both double-quote and single-quote delimiters.
fn extract_content(tag: &str) -> Option<String> {
    // Try double-quote first.
    if let Some(v) = extract_attr_value(tag, "content=\"") {
        return Some(v);
    }
    // Fall back to single-quote.
    extract_attr_value(tag, "content='")
}

/// Find `needle` (case-insensitive) in `src`, then extract the value up to the
/// matching closing quote character (either `"` or `'`).
fn extract_attr_value(src: &str, needle: &str) -> Option<String> {
    let lower = src.to_lowercase();
    let pos = lower.find(needle)?;
    let closing_quote = needle.chars().last()?; // " or '
    let value_start = pos + needle.len();
    let rest = src.get(value_start..)?;
    let end = rest.find(closing_quote).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Extract text content between `<title>` and `</title>` within a single line.
fn extract_title_tag(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let start_tag = "<title>";
    let end_tag = "</title>";
    let start = lower.find(start_tag)? + start_tag.len();
    let rest = line.get(start..)?;
    let end = rest.to_lowercase().find(end_tag).unwrap_or(rest.len());
    let content = rest[..end].trim().to_string();
    if content.is_empty() {
        None
    } else {
        Some(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_URL: &str = "https://example.com/article";

    fn html(snippets: &[&str]) -> String {
        format!(
            "<!DOCTYPE html><html><head>{}</head><body></body></html>",
            snippets.join("\n")
        )
    }

    #[test]
    fn parse_og_title() {
        let h = html(&[r#"<meta property="og:title" content="OG Title Here" />"#]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(preview.title, Some("OG Title Here".to_string()));
    }

    #[test]
    fn parse_og_description() {
        let h =
            html(&[r#"<meta property="og:description" content="Great article about Rust." />"#]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(
            preview.description,
            Some("Great article about Rust.".to_string())
        );
    }

    #[test]
    fn parse_og_image() {
        let h = html(&[r#"<meta property="og:image" content="https://example.com/img.png" />"#]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(
            preview.image_url,
            Some("https://example.com/img.png".to_string())
        );
    }

    #[test]
    fn parse_fallback_title_tag() {
        // No og:title — should fall back to <title>
        let h = html(&["<title>Fallback HTML Title</title>"]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(preview.title, Some("Fallback HTML Title".to_string()));
    }

    #[test]
    fn parse_site_name() {
        let h = html(&[r#"<meta property="og:site_name" content="Example Site" />"#]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(preview.site_name, Some("Example Site".to_string()));
    }

    #[test]
    fn empty_html_returns_defaults() {
        let preview = parse_preview(TEST_URL, "");
        assert_eq!(preview.url, TEST_URL);
        assert_eq!(preview.title, None);
        assert_eq!(preview.description, None);
        assert_eq!(preview.image_url, None);
        assert_eq!(preview.site_name, None);
    }

    #[test]
    fn single_quote_content_attribute() {
        let h = html(&["<meta property='og:title' content='Single Quote Title' />"]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(preview.title, Some("Single Quote Title".to_string()));
    }

    #[test]
    fn og_title_takes_priority_over_html_title() {
        let h = html(&[
            r#"<meta property="og:title" content="OG Wins" />"#,
            "<title>HTML Title</title>",
        ]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(preview.title, Some("OG Wins".to_string()));
    }

    #[test]
    fn twitter_image_fallback() {
        let h = html(&[r#"<meta name="twitter:image" content="https://example.com/tw.jpg" />"#]);
        let preview = parse_preview(TEST_URL, &h);
        assert_eq!(
            preview.image_url,
            Some("https://example.com/tw.jpg".to_string())
        );
    }
}
