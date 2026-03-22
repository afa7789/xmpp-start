// Task P6.5 — OS native desktop notifications
//
// Replaces the WebPush / ServiceWorker approach used in the original
// Tauri+React app.  `notify-rust` wraps the platform notification APIs
// (libnotify on Linux, UserNotifications on macOS, Windows.UI.Notifications
// on Windows) behind a single ergonomic interface.

/// All the data needed to display a single desktop notification.
#[derive(Debug, Clone)]
pub struct NotificationPayload {
    pub title: String,
    pub body: String,
    /// App name shown in the notification banner (platform-dependent).
    pub app_name: String,
}

/// Send a desktop notification.
///
/// Returns `Ok(())` on success.  Returns `Err(String)` if the underlying
/// platform API rejects the notification (e.g. the user has denied permission,
/// or the notification daemon is not running).  The caller may log the error
/// and continue — notifications are best-effort.
pub fn send(payload: &NotificationPayload) -> Result<(), String> {
    notify_rust::Notification::new()
        .summary(&payload.title)
        .body(&payload.body)
        .appname(&payload.app_name)
        .show()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Convenience wrapper: show a "new message" notification.
///
/// `from_jid` is displayed as the notification title; `body` is the message
/// text.
pub fn notify_message(from_jid: &str, body: &str) -> Result<(), String> {
    let payload = NotificationPayload {
        title: from_jid.to_string(),
        body: body.to_string(),
        app_name: "xmpp-start".to_string(),
    };
    send(&payload)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_title_and_body() {
        let p = NotificationPayload {
            title: "New message".into(),
            body: "Hello!".into(),
            app_name: "xmpp-start".into(),
        };
        assert_eq!(p.title, "New message");
        assert_eq!(p.body, "Hello!");
    }

    #[test]
    fn payload_app_name() {
        let p = NotificationPayload {
            title: "t".into(),
            body: "b".into(),
            app_name: "xmpp-start".into(),
        };
        assert_eq!(p.app_name, "xmpp-start");
    }

    #[test]
    fn notify_message_payload_construction() {
        // Verify the string formatting that notify_message uses without
        // triggering an actual system notification.
        let from = "alice@example.com";
        let body = "Hey there";

        let payload = NotificationPayload {
            title: from.to_string(),
            body: body.to_string(),
            app_name: "xmpp-start".to_string(),
        };

        assert_eq!(payload.title, "alice@example.com");
        assert_eq!(payload.body, "Hey there");
        assert_eq!(payload.app_name, "xmpp-start");
    }
}
