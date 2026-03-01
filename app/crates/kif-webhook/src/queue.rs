use std::{collections::HashSet, sync::Arc};

use tokio::sync::{Mutex, mpsc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub namespace: String,
    pub service_account_name: String,
}

#[derive(Clone)]
pub struct Queue {
    tx: mpsc::Sender<Key>,
    in_flight: Arc<Mutex<HashSet<Key>>>,
}

impl Queue {
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<Key>) {
        let (tx, rx) = mpsc::channel(buffer);
        let q = Self {
            tx,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        };
        (q, rx)
    }

    pub async fn enqueue(&self, key: Key) {
        {
            let mut set = self.in_flight.lock().await;
            if set.contains(&key) {
                return;
            }
            set.insert(key.clone());
        }

        if self.tx.send(key.clone()).await.is_err() {
            self.in_flight.lock().await.remove(&key);
        }
    }

    pub async fn done(&self, key: &Key) {
        let mut set = self.in_flight.lock().await;
        set.remove(key);
    }
}
