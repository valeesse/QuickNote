mod store;
pub mod websocket;

use dashmap::DashMap;
use tokio::sync::broadcast;

#[derive(Clone)]
pub enum CollabEvent {
    Update(Vec<u8>),
    Awareness(String),
}

#[derive(Default)]
pub struct CollabHub {
    rooms: DashMap<String, broadcast::Sender<CollabEvent>>,
}

impl CollabHub {
    pub fn subscribe(&self, room: &str) -> broadcast::Receiver<CollabEvent> {
        self.sender(room).subscribe()
    }

    pub fn broadcast(&self, room: &str, event: CollabEvent) {
        let _ = self.sender(room).send(event);
    }

    fn sender(&self, room: &str) -> broadcast::Sender<CollabEvent> {
        self.rooms
            .entry(room.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .clone()
    }
}
