use tokio::sync::broadcast::Receiver;

use crate::Message;

pub async fn abort(mut rx: Receiver<Message>) {
    while let Ok(Message::NalUnit(_)) = rx.recv().await {
        // Drain
    }

    return;
}
