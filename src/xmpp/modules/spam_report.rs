// Task L5 — XEP-0377 Spam Reporting
// XEP reference: https://xmpp.org/extensions/xep-0377.html
//
// Pure stanza builder — no I/O, no async.
// Builds an IQ set containing an abuse report as per XEP-0377.

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

use super::NS_CLIENT;

const NS_ABUSE: &str = "urn:xmpp:reporting:1";

// ---------------------------------------------------------------------------
// Stanza builder
// ---------------------------------------------------------------------------

/// Build a spam/abuse report IQ (XEP-0377).
///
/// ```xml
/// <iq type="set" id="{uuid}">
///   <report xmlns="urn:xmpp:reporting:1" reason="spam">
///     <jid>spammer@example.org</jid>
///     <text>Unsolicited messages</text>   <!-- optional -->
///   </report>
/// </iq>
/// ```
pub fn build_spam_report(jid: &str, reason: Option<&str>) -> Element {
    let id = Uuid::new_v4().to_string();

    let jid_el = Element::builder("jid", NS_ABUSE)
        .append(jid)
        .build();

    let mut report_builder = Element::builder("report", NS_ABUSE)
        .attr("reason", "spam")
        .append(jid_el);

    if let Some(text) = reason {
        let text_el = Element::builder("text", NS_ABUSE)
            .append(text)
            .build();
        report_builder = report_builder.append(text_el);
    }

    Element::builder("iq", NS_CLIENT)
        .attr("type", "set")
        .attr("id", &id)
        .append(report_builder.build())
        .build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spam_report_has_correct_type_and_ns() {
        let el = build_spam_report("spammer@example.org", None);

        assert_eq!(el.name(), "iq");
        assert_eq!(el.attr("type"), Some("set"));
        assert!(el.attr("id").is_some());

        let report = el
            .children()
            .find(|c| c.name() == "report")
            .expect("no report child");
        assert_eq!(report.ns(), NS_ABUSE);
        assert_eq!(report.attr("reason"), Some("spam"));
    }

    #[test]
    fn spam_report_contains_jid() {
        let el = build_spam_report("spammer@example.org", None);
        let report = el.children().find(|c| c.name() == "report").unwrap();

        let jid_el = report
            .children()
            .find(|c| c.name() == "jid")
            .expect("no jid child");
        assert_eq!(jid_el.text(), "spammer@example.org");
    }

    #[test]
    fn spam_report_with_reason_includes_text_element() {
        let el = build_spam_report("spammer@example.org", Some("Sending unsolicited ads"));
        let report = el.children().find(|c| c.name() == "report").unwrap();

        let text_el = report
            .children()
            .find(|c| c.name() == "text")
            .expect("no text child");
        assert_eq!(text_el.text(), "Sending unsolicited ads");
    }

    #[test]
    fn spam_report_without_reason_has_no_text_element() {
        let el = build_spam_report("spammer@example.org", None);
        let report = el.children().find(|c| c.name() == "report").unwrap();

        assert!(report.children().find(|c| c.name() == "text").is_none());
    }

    #[test]
    fn spam_report_unique_ids() {
        let el1 = build_spam_report("a@example.org", None);
        let el2 = build_spam_report("a@example.org", None);
        assert_ne!(el1.attr("id"), el2.attr("id"));
    }
}
