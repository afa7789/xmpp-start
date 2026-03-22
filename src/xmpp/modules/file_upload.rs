#![allow(dead_code)]
// Task P5.1 — XEP-0363 HTTP File Upload
// XEP reference: https://xmpp.org/extensions/xep-0363.html
//
// This is a pure state machine — no I/O, no async.
// The engine calls it to:
//   - build upload slot request IQ stanzas
//   - parse incoming slot result IQ responses (put/get URLs + headers)
//   - track pending requests and clear them on success or error

use std::collections::HashMap;

use tokio_xmpp::minidom::Element;
use uuid::Uuid;

const NS_UPLOAD: &str = "urn:xmpp:http:upload:0";
const NS_CLIENT: &str = "jabber:client";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An upload slot returned by the server: PUT target, GET URL, and any
/// required HTTP headers to include in the PUT request.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadSlot {
    pub put_url: String,
    pub get_url: String,
    /// Headers to include in the PUT request (name, value pairs).
    pub put_headers: Vec<(String, String)>,
}

/// The parameters that identify a pending slot request.
#[derive(Debug, Clone)]
pub struct SlotRequest {
    /// IQ id used for correlation with the server's result.
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub content_type: String,
    pub upload_service_jid: String,
}

// ---------------------------------------------------------------------------
// FileUploadManager
// ---------------------------------------------------------------------------

/// XEP-0363 state manager.
///
/// Builds slot request IQ stanzas and parses slot result / error IQ responses.
/// All methods are pure: they only mutate in-memory state and return
/// stanzas or parsed values for the caller to act on.
pub struct FileUploadManager {
    /// Pending slot requests keyed by IQ id.
    pending: HashMap<String, SlotRequest>,
}

impl Default for FileUploadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileUploadManager {
    /// Creates an empty manager with no pending requests.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Build the IQ stanza to request an upload slot and register it as
    /// pending.
    ///
    /// Returns `(iq_id, Element)`. The element must be written to the XMPP
    /// stream by the caller.
    ///
    /// ```xml
    /// <iq type="get" id="{uuid}" to="{upload_service_jid}" xmlns="jabber:client">
    ///   <request xmlns="urn:xmpp:http:upload:0"
    ///            filename="{filename}"
    ///            size="{size}"
    ///            content-type="{content_type}"/>
    /// </iq>
    /// ```
    pub fn request_slot(
        &mut self,
        filename: &str,
        size: u64,
        content_type: &str,
        upload_service_jid: &str,
    ) -> (String, Element) {
        let iq_id = Uuid::new_v4().to_string();

        let request_el = Element::builder("request", NS_UPLOAD)
            .attr("filename", filename)
            .attr("size", size.to_string().as_str())
            .attr("content-type", content_type)
            .build();

        let iq = Element::builder("iq", NS_CLIENT)
            .attr("type", "get")
            .attr("id", iq_id.as_str())
            .attr("to", upload_service_jid)
            .append(request_el)
            .build();

        self.pending.insert(
            iq_id.clone(),
            SlotRequest {
                id: iq_id.clone(),
                filename: filename.to_string(),
                size,
                content_type: content_type.to_string(),
                upload_service_jid: upload_service_jid.to_string(),
            },
        );

        (iq_id, iq)
    }

    /// Parse a slot result IQ and return the `UploadSlot` if successful.
    ///
    /// Removes the matching request from pending on success.
    ///
    /// Expected stanza shape:
    /// ```xml
    /// <iq type="result" id="{id}">
    ///   <slot xmlns="urn:xmpp:http:upload:0">
    ///     <put url="https://...">
    ///       <header name="Authorization">Basic ...</header>
    ///     </put>
    ///     <get url="https://..."/>
    ///   </slot>
    /// </iq>
    /// ```
    pub fn on_slot_result(&mut self, el: &Element) -> Option<UploadSlot> {
        if el.name() != "iq" {
            return None;
        }
        if el.attr("type") != Some("result") {
            return None;
        }

        let iq_id = el.attr("id")?;

        // Only handle IQs we're tracking.
        if !self.pending.contains_key(iq_id) {
            return None;
        }

        // Find <slot xmlns='urn:xmpp:http:upload:0'>
        let slot_el = el
            .children()
            .find(|c| c.name() == "slot" && c.ns() == NS_UPLOAD)?;

        // Find <put url="...">
        let put_el = slot_el.children().find(|c| c.name() == "put")?;

        let put_url = put_el.attr("url")?.to_string();

        // Collect <header name="..."> children of <put>
        let put_headers: Vec<(String, String)> = put_el
            .children()
            .filter(|c| c.name() == "header")
            .filter_map(|h| {
                let name = h.attr("name")?.to_string();
                let value = h.text();
                Some((name, value))
            })
            .collect();

        // Find <get url="...">
        let get_el = slot_el.children().find(|c| c.name() == "get")?;

        let get_url = get_el.attr("url")?.to_string();

        // Remove from pending on success.
        self.pending.remove(iq_id);

        Some(UploadSlot {
            put_url,
            get_url,
            put_headers,
        })
    }

    /// Parse an error IQ. Returns the IQ id if it matched a pending request.
    ///
    /// Removes the matching request from pending on error.
    pub fn on_slot_error(&mut self, el: &Element) -> Option<String> {
        if el.name() != "iq" {
            return None;
        }
        if el.attr("type") != Some("error") {
            return None;
        }

        let iq_id = el.attr("id")?;

        if self.pending.remove(iq_id).is_some() {
            Some(iq_id.to_string())
        } else {
            None
        }
    }

    /// Returns `true` if there is a pending request with this IQ id.
    pub fn is_pending(&self, iq_id: &str) -> bool {
        self.pending.contains_key(iq_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build a minimal slot result IQ XML string and parse it.
    fn make_slot_result_iq(
        iq_id: &str,
        put_url: &str,
        get_url: &str,
        headers: &[(&str, &str)],
    ) -> Element {
        let mut put_builder = Element::builder("put", NS_UPLOAD).attr("url", put_url);
        for (name, value) in headers {
            let header_el = Element::builder("header", NS_UPLOAD)
                .attr("name", *name)
                .append(*value)
                .build();
            put_builder = put_builder.append(header_el);
        }

        let get_el = Element::builder("get", NS_UPLOAD)
            .attr("url", get_url)
            .build();

        let slot_el = Element::builder("slot", NS_UPLOAD)
            .append(put_builder.build())
            .append(get_el)
            .build();

        Element::builder("iq", NS_CLIENT)
            .attr("type", "result")
            .attr("id", iq_id)
            .append(slot_el)
            .build()
    }

    // Helper: build a minimal error IQ.
    fn make_error_iq(iq_id: &str) -> Element {
        Element::builder("iq", NS_CLIENT)
            .attr("type", "error")
            .attr("id", iq_id)
            .build()
    }

    // 1. Requesting a slot registers it as pending.
    #[test]
    fn request_slot_registers_pending() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("photo.jpg", 102400, "image/jpeg", "upload.example.com");
        assert!(mgr.is_pending(&id));
    }

    // 2. The request IQ carries the correct namespace on the <request> child.
    #[test]
    fn request_slot_iq_has_correct_namespace() {
        let mut mgr = FileUploadManager::new();
        let (_id, el) = mgr.request_slot("photo.jpg", 102400, "image/jpeg", "upload.example.com");

        let request_child = el
            .children()
            .find(|c| c.name() == "request")
            .expect("<request> child must exist");

        assert_eq!(request_child.ns(), NS_UPLOAD);
    }

    // 3. The <request> element carries filename, size, and content-type attributes.
    #[test]
    fn request_slot_iq_has_filename_size_content_type() {
        let mut mgr = FileUploadManager::new();
        let (_id, el) =
            mgr.request_slot("report.pdf", 20480, "application/pdf", "upload.example.com");

        let req = el
            .children()
            .find(|c| c.name() == "request")
            .expect("<request> must be present");

        assert_eq!(req.attr("filename"), Some("report.pdf"));
        assert_eq!(req.attr("size"), Some("20480"));
        assert_eq!(req.attr("content-type"), Some("application/pdf"));
    }

    // 4. on_slot_result parses put and get URLs correctly.
    #[test]
    fn on_slot_result_parses_put_and_get_urls() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("photo.jpg", 1024, "image/jpeg", "upload.example.com");

        let result_iq = make_slot_result_iq(
            &id,
            "https://upload.example.com/files/abc?sig=xyz",
            "https://cdn.example.com/files/abc",
            &[],
        );

        let slot = mgr.on_slot_result(&result_iq).expect("must parse slot");
        assert_eq!(slot.put_url, "https://upload.example.com/files/abc?sig=xyz");
        assert_eq!(slot.get_url, "https://cdn.example.com/files/abc");
    }

    // 5. on_slot_result parses put headers.
    #[test]
    fn on_slot_result_parses_put_headers() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("photo.jpg", 1024, "image/jpeg", "upload.example.com");

        let result_iq = make_slot_result_iq(
            &id,
            "https://upload.example.com/files/abc",
            "https://cdn.example.com/files/abc",
            &[
                ("Authorization", "Basic dXNlcjpwYXNz"),
                ("Cookie", "session=abc123"),
            ],
        );

        let slot = mgr.on_slot_result(&result_iq).expect("must parse slot");
        assert_eq!(slot.put_headers.len(), 2);
        assert_eq!(
            slot.put_headers[0],
            (
                "Authorization".to_string(),
                "Basic dXNlcjpwYXNz".to_string()
            )
        );
        assert_eq!(
            slot.put_headers[1],
            ("Cookie".to_string(), "session=abc123".to_string())
        );
    }

    // 6. on_slot_result removes the request from pending.
    #[test]
    fn on_slot_result_clears_pending() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("photo.jpg", 1024, "image/jpeg", "upload.example.com");

        let result_iq = make_slot_result_iq(
            &id,
            "https://upload.example.com/files/abc",
            "https://cdn.example.com/files/abc",
            &[],
        );

        mgr.on_slot_result(&result_iq);
        assert!(!mgr.is_pending(&id), "pending must be cleared after result");
    }

    // 7. on_slot_error removes the request from pending and returns the id.
    #[test]
    fn on_slot_error_clears_pending() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("photo.jpg", 1024, "image/jpeg", "upload.example.com");

        let error_iq = make_error_iq(&id);
        let returned_id = mgr
            .on_slot_error(&error_iq)
            .expect("must return id on error");

        assert_eq!(returned_id, id);
        assert!(!mgr.is_pending(&id), "pending must be cleared after error");
    }

    // 8. is_pending returns false after a successful result.
    #[test]
    fn is_pending_false_after_result() {
        let mut mgr = FileUploadManager::new();
        let (id, _el) = mgr.request_slot("video.mp4", 5_000_000, "video/mp4", "upload.example.com");

        assert!(mgr.is_pending(&id));

        let result_iq = make_slot_result_iq(
            &id,
            "https://upload.example.com/files/vid",
            "https://cdn.example.com/files/vid",
            &[],
        );
        mgr.on_slot_result(&result_iq);

        assert!(!mgr.is_pending(&id));
    }

    // 9. Multiple concurrent requests are tracked independently.
    #[test]
    fn multiple_concurrent_requests() {
        let mut mgr = FileUploadManager::new();

        let (id_a, _) = mgr.request_slot("a.jpg", 1024, "image/jpeg", "upload.example.com");
        let (id_b, _) = mgr.request_slot("b.png", 2048, "image/png", "upload.example.com");
        let (id_c, _) = mgr.request_slot("c.pdf", 4096, "application/pdf", "upload.example.com");

        assert!(mgr.is_pending(&id_a));
        assert!(mgr.is_pending(&id_b));
        assert!(mgr.is_pending(&id_c));

        // Resolve b with a result.
        let result_b = make_slot_result_iq(
            &id_b,
            "https://upload.example.com/files/b",
            "https://cdn.example.com/files/b",
            &[],
        );
        let slot_b = mgr.on_slot_result(&result_b).expect("slot_b must parse");
        assert_eq!(slot_b.get_url, "https://cdn.example.com/files/b");

        // a and c still pending; b is gone.
        assert!(mgr.is_pending(&id_a));
        assert!(!mgr.is_pending(&id_b));
        assert!(mgr.is_pending(&id_c));

        // Resolve c with an error.
        let error_c = make_error_iq(&id_c);
        mgr.on_slot_error(&error_c);

        assert!(mgr.is_pending(&id_a));
        assert!(!mgr.is_pending(&id_c));
    }
}
