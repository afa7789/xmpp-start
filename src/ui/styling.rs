// Task P2.3b — XEP-0393 message styling parser
// XEP reference: https://xmpp.org/extensions/xep-0393.html
//
// Parses inline styling markers into a flat list of Spans.
// iced will render these with appropriate text styles.

#[derive(Debug, Clone, PartialEq)]
pub enum SpanStyle {
    Plain,
    Bold,
    Italic,
    Code,
    Strike,
    Quote,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

impl Span {
    fn plain(text: impl Into<String>) -> Self {
        Span {
            text: text.into(),
            style: SpanStyle::Plain,
        }
    }

    fn styled(text: impl Into<String>, style: SpanStyle) -> Self {
        Span {
            text: text.into(),
            style,
        }
    }
}

/// Returns the `SpanStyle` for an inline delimiter character, or `None` if not a delimiter.
fn style_for_delimiter(ch: char) -> Option<SpanStyle> {
    match ch {
        '*' => Some(SpanStyle::Bold),
        '_' => Some(SpanStyle::Italic),
        '`' => Some(SpanStyle::Code),
        '~' => Some(SpanStyle::Strike),
        _ => None,
    }
}

/// Returns true if the character counts as a word boundary (whitespace or start-of-string
/// sentinel). Used to validate opening delimiter position.
fn is_boundary(ch: char) -> bool {
    ch.is_whitespace()
}

/// Returns true if the character is a valid character after a closing delimiter
/// (whitespace, end-of-string, or ASCII punctuation).
fn is_after_close(ch: char) -> bool {
    ch.is_whitespace() || ch.is_ascii_punctuation()
}

/// Parse a message body into styled spans per XEP-0393.
///
/// Lines beginning with `> ` (greater-than + space) are emitted as a single
/// `SpanStyle::Quote` span. All other lines are scanned for inline delimiters.
/// Nested styling is not supported — only one style is active at a time.
pub fn parse(input: &str) -> Vec<Span> {
    if input.is_empty() {
        return vec![];
    }

    let mut result: Vec<Span> = Vec::new();

    for (line_idx, line) in input.lines().enumerate() {
        // Emit a newline separator between lines (skip before the first line).
        if line_idx > 0 {
            // Attach newline to the previous span or emit its own Plain span.
            if let Some(last) = result.last_mut() {
                last.text.push('\n');
            } else {
                result.push(Span::plain("\n"));
            }
        }

        // Blockquote: line starts with "> "
        if line.starts_with("> ") {
            result.push(Span::styled(
                line.strip_prefix("> ").unwrap_or(line),
                SpanStyle::Quote,
            ));
            continue;
        }

        // Inline parsing: scan character by character.
        parse_inline(line, &mut result);
    }

    result
}

/// Scan one line for inline styling delimiters and push spans into `out`.
fn parse_inline(line: &str, out: &mut Vec<Span>) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    let mut plain_buf = String::new();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if let Some(style) = style_for_delimiter(ch) {
            // Check opening boundary: preceded by whitespace / start-of-string.
            let prev = if i == 0 { None } else { Some(chars[i - 1]) };
            let preceded_by_boundary = prev.map_or(true, is_boundary);

            // Must be followed by a non-whitespace character.
            let next = if i + 1 < len {
                Some(chars[i + 1])
            } else {
                None
            };
            let followed_by_nonws = next.is_some_and(|n| !n.is_whitespace());

            if preceded_by_boundary && followed_by_nonws {
                // Attempt to find a matching closing delimiter.
                if let Some(close_pos) = find_close(&chars, i + 1, ch) {
                    // Flush any buffered plain text first.
                    if !plain_buf.is_empty() {
                        out.push(Span::plain(std::mem::take(&mut plain_buf)));
                    }
                    let inner: String = chars[i + 1..close_pos].iter().collect();
                    out.push(Span::styled(inner, style));
                    i = close_pos + 1;
                    continue;
                }
            }

            // Not a valid opening — treat as plain text.
            plain_buf.push(ch);
            i += 1;
        } else {
            plain_buf.push(ch);
            i += 1;
        }
    }

    if !plain_buf.is_empty() {
        out.push(Span::plain(plain_buf));
    }
}

/// Search for the matching closing delimiter for `delim`, starting at `start`.
///
/// A valid closing position satisfies:
///   - `chars[pos] == delim`
///   - `chars[pos - 1]` is not whitespace (something before the delimiter)
///   - `chars[pos + 1]` is whitespace, punctuation, or end-of-string
///
/// Returns the index of the closing delimiter in `chars`, or `None`.
fn find_close(chars: &[char], start: usize, delim: char) -> Option<usize> {
    for pos in start..chars.len() {
        if chars[pos] != delim {
            continue;
        }
        // Must be preceded by non-whitespace.
        if pos == start {
            // Opening delimiter immediately followed by closing delimiter — empty span, skip.
            continue;
        }
        let before = chars[pos - 1];
        if before.is_whitespace() {
            continue;
        }
        // Must be followed by whitespace, punctuation, or end.
        let after = if pos + 1 < chars.len() {
            Some(chars[pos + 1])
        } else {
            None
        };
        if after.map_or(true, is_after_close) {
            return Some(pos);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str, style: SpanStyle) -> Span {
        Span {
            text: text.to_string(),
            style,
        }
    }

    fn plain(text: &str) -> Span {
        s(text, SpanStyle::Plain)
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(parse("hello world"), vec![plain("hello world")]);
    }

    #[test]
    fn bold_span() {
        assert_eq!(parse("*hello*"), vec![s("hello", SpanStyle::Bold)]);
    }

    #[test]
    fn italic_span() {
        assert_eq!(parse("_hello_"), vec![s("hello", SpanStyle::Italic)]);
    }

    #[test]
    fn code_span() {
        assert_eq!(parse("`hello`"), vec![s("hello", SpanStyle::Code)]);
    }

    #[test]
    fn strike_span() {
        assert_eq!(parse("~hello~"), vec![s("hello", SpanStyle::Strike)]);
    }

    #[test]
    fn unclosed_delimiter_is_plain() {
        assert_eq!(parse("*hello"), vec![plain("*hello")]);
    }

    #[test]
    fn mixed_plain_and_bold() {
        assert_eq!(
            parse("hello *world* foo"),
            vec![plain("hello "), s("world", SpanStyle::Bold), plain(" foo"),]
        );
    }

    #[test]
    fn quote_line() {
        assert_eq!(parse("> hello"), vec![s("hello", SpanStyle::Quote)]);
    }

    #[test]
    fn empty_input() {
        assert_eq!(parse(""), vec![]);
    }

    #[test]
    fn delimiter_mid_word_not_styled() {
        // "he*llo" — the `*` is preceded by 'e' which is not a boundary.
        assert_eq!(parse("he*llo"), vec![plain("he*llo")]);
    }

    // --- additional edge-case tests ---

    #[test]
    fn bold_at_end_of_sentence() {
        // Closing delimiter followed by punctuation is valid.
        assert_eq!(
            parse("say *hello*."),
            vec![plain("say "), s("hello", SpanStyle::Bold), plain("."),]
        );
    }

    #[test]
    fn multiple_styled_spans() {
        assert_eq!(
            parse("*a* _b_ `c`"),
            vec![
                s("a", SpanStyle::Bold),
                plain(" "),
                s("b", SpanStyle::Italic),
                plain(" "),
                s("c", SpanStyle::Code),
            ]
        );
    }

    #[test]
    fn multiline_quote_then_plain() {
        let input = "> quoted line\nnormal line";
        let spans = parse(input);
        // The newline separator is appended to the preceding quote span.
        assert_eq!(spans[0], s("quoted line\n", SpanStyle::Quote));
        // The plain line follows.
        assert!(spans
            .iter()
            .any(|sp| sp.text.contains("normal line") && sp.style == SpanStyle::Plain));
    }

    #[test]
    fn empty_delimiter_pair_is_plain() {
        // "**" has nothing inside — not a valid styled span.
        assert_eq!(parse("**"), vec![plain("**")]);
    }
}
