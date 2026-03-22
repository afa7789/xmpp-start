// Task P0.2 — iced subscription that bridges the XmppEngine into the UI.
//
// iced 0.13 pattern:
//   iced::stream::channel(size, async_fn) -> Stream<Item = T>
//   Subscription::run_with_id(id, stream)

use iced::futures::SinkExt;
use iced::Subscription;
use tokio::sync::mpsc;

use super::{engine::XmppEngine, XmppEvent};
use crate::ui::Message;

/// Returns an iced Subscription that spawns the XMPP engine and pipes its
/// events into `Message::XmppEvent` variants.
pub fn xmpp_subscription() -> Subscription<Message> {
    // Build a stream that spawns the engine and forwards its events.
    let stream = iced::stream::channel(
        100,
        |mut iced_sender: iced::futures::channel::mpsc::Sender<Message>| async move {
            let (tx, mut rx) = mpsc::channel::<XmppEvent>(32);
            let engine = XmppEngine::new(tx);

            // Spawn the connect stub; errors are silently ignored for now.
            tokio::spawn(async move {
                let _ = engine.connect().await;
            });

            // Forward every engine event into iced's message stream.
            while let Some(event) = rx.recv().await {
                let _ = iced_sender.send(Message::XmppEvent(event)).await;
            }
        },
    );

    Subscription::run_with_id(std::any::TypeId::of::<XmppEngine>(), stream)
}
