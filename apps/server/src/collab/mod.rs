pub mod websocket;

use dashmap::DashMap;
use tokio::sync::broadcast;

#[derive(Default)]
pub struct CollabHub {
    rooms: DashMap<String, broadcast::Sender<Vec<u8>>>,
}

impl CollabHub {
    pub fn subscribe(&self, room: &str) -> broadcast::Receiver<Vec<u8>> {
        self.sender(room).subscribe()
    }

    pub fn broadcast(&self, room: &str, update: Vec<u8>) {
        let _ = self.sender(room).send(update);
    }

    fn sender(&self, room: &str) -> broadcast::Sender<Vec<u8>> {
        self.rooms
            .entry(room.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .clone()
    }
}
