// Task P1.9 — iced subscription: bridges XmppEngine ↔ UI via two channels.
//
// Pattern:
//   1. Subscription starts, creates a (cmd_tx, cmd_rx) pair.
//   2. Sends cmd_tx back to the UI as Message::XmppReady so the UI can
//      drive the engine (Connect, SendMessage, Disconnect).
//   3. Spawns run_engine() which reads from cmd_rx and emits XmppEvent
//      through event_tx → forwarded to UI as Message::XmppEvent.

use iced::futures::SinkExt;
use iced::Subscription;
use sqlx::SqlitePool;
use tokio::sync::mpsc;

use super::{engine::run_engine, XmppCommand, XmppEvent};
use crate::ui::Message;

/// Returns an iced Subscription that owns the XMPP engine for the app lifetime.
///
/// `db` is the SQLite pool passed to the engine for OMEMO key/session persistence.
pub fn xmpp_subscription(db: SqlitePool) -> Subscription<Message> {
    let stream = iced::stream::channel(
        64,
        |mut iced_tx: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Channel for events engine → UI.
            let (event_tx, mut event_rx) = mpsc::channel::<XmppEvent>(64);
            // Channel for commands UI → engine.
            let (cmd_tx, cmd_rx) = mpsc::channel::<XmppCommand>(32);

            // Give the command sender to the UI so it can drive the engine.
            let _ = iced_tx.send(Message::XmppReady(cmd_tx)).await;

            // Spawn the engine with the DB pool for OMEMO support.
            tokio::spawn(run_engine(event_tx, cmd_rx, Some(db)));

            // Forward engine events into iced's message stream.
            while let Some(event) = event_rx.recv().await {
                let _ = iced_tx.send(Message::XmppEvent(event)).await;
            }
        },
    );

    Subscription::run_with_id(std::any::TypeId::of::<XmppCommand>(), stream)
}
