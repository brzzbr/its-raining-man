use std::collections::HashMap;
use std::path;
use std::sync::Arc;

use teloxide::prelude::ChatId;
use tokio::fs;
use tokio::sync::RwLock;

use crate::domain::{AsyncUnit, Location};

type ConsistentState = Arc<RwLock<HashMap<ChatId, (Location, Option<u64>)>>>;

#[derive(Clone)]
pub struct KindaDb {
    path: String,
    state: ConsistentState,
}

impl KindaDb {
    pub async fn all(&self) -> HashMap<ChatId, (Location, Option<u64>)> {
        self.state.read().await.clone()
    }

    pub async fn add(&self, chat_id: ChatId, location: Location) {
        let mut state = self.state.write().await;
        let _ = state.insert(chat_id, (location, None));
        Self::save_state(&self.path, &state).await
    }

    pub async fn update(&self, chat_id: ChatId, time: u64) {
        let mut state = self.state.write().await;
        let _ = state.entry(chat_id).and_modify(|(_, t)| *t = Some(time));
        Self::save_state(&self.path, &state).await
    }

    pub async fn delete(&self, chat_id: ChatId) {
        let mut state = self.state.write().await;
        let _ = state.remove(&chat_id);
        Self::save_state(&self.path, &state).await
    }

    pub async fn new(path: String) -> KindaDb {
        let state = match path::Path::new(&path).exists() {
            false => HashMap::default(),
            true => fs::read_to_string(&path)
                .await
                .unwrap()
                .split('\n')
                .filter(|&s| !s.is_empty())
                .map(|record| {
                    log::info!("record is {:?}", record);
                    let mut parts = record.split_whitespace();
                    let chat_id = ChatId(parts.next().unwrap().parse::<i64>().unwrap());
                    let lat = parts.next().unwrap().parse::<f64>().unwrap();
                    let lon = parts.next().unwrap().parse::<f64>().unwrap();
                    let optional_u64 = parts.next().and_then(|field| field.parse::<u64>().ok());

                    let location = Location::new(lat, lon);
                    (chat_id, (location, optional_u64))
                })
                .collect(),
        };

        KindaDb {
            path,
            state: Arc::new(RwLock::new(state)),
        }
    }

    async fn save_state(path: &String, state: &HashMap<ChatId, (Location, Option<u64>)>) {
        let state_str = state.iter().fold(
            String::new(),
            |mut acc, (chat_id, (location, last_alert))| {
                match last_alert {
                    None => acc.push_str(&format!(
                        "{} {:.7} {:.7}\n",
                        chat_id,
                        location.lat(),
                        location.lon()
                    )),
                    Some(last_alert) => acc.push_str(&format!(
                        "{} {:.7} {:.7} {}\n",
                        chat_id,
                        location.lat(),
                        location.lon(),
                        last_alert
                    )),
                }

                acc
            },
        );

        fs::write(path, state_str).await.unwrap()
    }
}

pub fn kinda_create(db: KindaDb) -> impl Fn(ChatId, Location) -> AsyncUnit {
    move |chat_id: ChatId, location: Location| {
        let db = db.clone();
        let result = async move { db.add(chat_id, location).await };
        Box::pin(result)
    }
}

pub fn kinda_update(db: KindaDb) -> impl Fn(ChatId, u64) -> AsyncUnit {
    move |chat_id: ChatId, alert_last: u64| {
        let db = db.clone();
        let result = async move { db.update(chat_id, alert_last).await };
        Box::pin(result)
    }
}

pub fn kinda_delete(db: KindaDb) -> impl Fn(ChatId) -> AsyncUnit {
    move |chat_id: ChatId| {
        let db = db.clone();
        let result = async move { db.delete(chat_id).await };
        Box::pin(result)
    }
}
