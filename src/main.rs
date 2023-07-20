use std::collections::HashMap;
use std::sync::Arc;

use config::Config;
use dotenv::dotenv;
use serde::Deserialize;
use teloxide::prelude::*;
use teloxide::Bot;
use tokio::sync::RwLock;

use crate::kinda_db::KindaDb;
use crate::state_manager::StateManager;

mod bot_flow;
mod domain;
mod kinda_db;
mod state_manager;

#[derive(Deserialize)]
struct AppConfig {
    db_path: String,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();

    log::info!("reading cfg, loading state, doing initialization mumbo-jumbo...");

    let config: AppConfig = Config::builder()
        .add_source(
            config::Environment::with_prefix("APP")
                .try_parsing(true)
                .separator("__"),
        )
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();

    let db = KindaDb::new(config.db_path).await;

    let bot = Bot::from_env();

    let state_manager = StateManager {
        check_every_seconds: 300,
        checkers: Arc::new(RwLock::new(HashMap::default())),
        check_forecast: Arc::new(bot_flow::check_forecast_and_notify_if_rain(bot.clone())),

        create_record: Arc::new(kinda_db::kinda_create(db.clone())),
        update_record: Arc::new(kinda_db::kinda_update(db.clone())),
        remove_record: Arc::new(kinda_db::kinda_delete(db.clone())),
    };

    for (chat_id, (location, last_alert)) in db.all().await {
        state_manager.add(chat_id, location, last_alert).await;
    }

    log::info!("its-raining-man bot started...");

    Dispatcher::builder(bot, bot_flow::schema())
        .dependencies(dptree::deps![state_manager])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    log::info!("its-raining-man bot stopped...");
}
