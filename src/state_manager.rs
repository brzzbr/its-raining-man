use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rand::Rng;
use teloxide::prelude::ChatId;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::domain::{Async, AsyncUnit, CheckError, Location};

type CheckResult = Result<bool, CheckError>;

#[derive(Clone)]
pub struct StateManager {
    pub check_every_seconds: u64,
    pub checkers: Arc<RwLock<HashMap<ChatId, JoinHandle<()>>>>,
    pub check_forecast: Arc<dyn Fn(ChatId, Location, u64) -> Async<CheckResult> + Sync + Send>,

    pub create_record: Arc<dyn Fn(ChatId, Location) -> AsyncUnit + Sync + Send>,
    pub update_record: Arc<dyn Fn(ChatId, u64) -> AsyncUnit + Sync + Send>,
    pub remove_record: Arc<dyn Fn(ChatId) -> AsyncUnit + Sync + Send>,
}

impl StateManager {
    pub async fn add(
        &self,
        chat_id: ChatId,
        location: Location,
        maybe_last_alerted_sec: Option<u64>,
    ) {
        let mut checkers = self.checkers.write().await;
        if let Some(join) = checkers.remove(&chat_id) {
            join.abort();
        }

        (self.create_record)(chat_id, location).await;

        let check_task = tokio::spawn({
            let check_every_seconds = self.check_every_seconds;
            let check_forecast = self.check_forecast.clone();
            let update_record = self.update_record.clone();
            let mut maybe_last_alerted_sec = maybe_last_alerted_sec;
            async move {
                loop {
                    maybe_last_alerted_sec = check_and_alert(
                        chat_id,
                        location,
                        maybe_last_alerted_sec,
                        check_forecast.deref(),
                        update_record.deref(),
                    )
                    .await;

                    let rnd_jitter_sleep = {
                        let mut rng = rand::thread_rng();
                        rng.gen_range(0..60)
                    };

                    tokio::time::sleep(Duration::from_secs(check_every_seconds + rnd_jitter_sleep))
                        .await;
                }
            }
        });

        checkers.insert(chat_id, check_task);
    }

    pub async fn remove(&self, chat_id: ChatId) {
        let mut checkers = self.checkers.write().await;
        if let Some(join) = checkers.remove(&chat_id) {
            join.abort();
            (self.remove_record)(chat_id).await;
        }
    }
}

async fn check_and_alert(
    chat_id: ChatId,
    location: Location,
    maybe_last_alerted_sec: Option<u64>,
    check_forecast: impl Fn(ChatId, Location, u64) -> Async<CheckResult>,
    update_record: impl Fn(ChatId, u64) -> AsyncUnit,
) -> Option<u64> {
    let now_sec = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let should_check = maybe_last_alerted_sec
        .map(|last_alerted_sec| now_sec - last_alerted_sec > 14400)
        .unwrap_or(false);

    if should_check {
        let alerted = check_forecast(chat_id, location, now_sec).await;

        match alerted {
            Err(err) => log::error!("error during weather request: {:?}", err),
            Ok(alerted) => {
                if alerted {
                    update_record(chat_id, now_sec).await;
                    return Some(now_sec);
                }
            }
        }
    }

    None
}
