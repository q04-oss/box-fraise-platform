use tokio::sync::broadcast;

use crate::events::DomainEvent;

// 256 slots: enough for burst traffic; slow consumers that fall behind by
// more than this will receive RecvError::Lagged and must handle the skip.
const CAPACITY: usize = 256;

/// In-process broadcast bus for domain events.
///
/// Clone is cheap — all clones share the same underlying channel sender.
/// Publish is fire-and-forget: if no subscribers are listening, the event
/// is silently dropped. If a subscriber is too slow it receives
/// `RecvError::Lagged` and must decide whether to skip or abort.
#[derive(Clone, Debug)]
pub struct EventBus {
    tx: broadcast::Sender<DomainEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self { tx }
    }

    /// Publish an event to all current subscribers.
    /// Silently drops the event if no subscribers are listening.
    pub fn publish(&self, event: DomainEvent) {
        let _ = self.tx.send(event);
    }

    /// Create a new receiver for this bus.
    /// The receiver will see all events published after this call.
    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
