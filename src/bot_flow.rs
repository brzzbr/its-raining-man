use std::time::Duration;

use fantoccini::ClientBuilder;
use teloxide::dispatching::UpdateHandler;
use teloxide::macros::BotCommands;
use teloxide::prelude::*;
use teloxide::types::{ButtonRequest, ChatMemberKind, InputFile, KeyboardButton, KeyboardMarkup};
use teloxide::Bot;

use crate::domain::{Async, CheckError, Location, WeatherResponse};
use crate::state_manager::StateManager;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    Start,
}

type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub fn schema() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    use dptree::case;

    dptree::entry()
        .branch(Update::filter_my_chat_member().endpoint(chat_member))
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .branch(case![Command::Start].endpoint(start)),
        )
        .branch(Update::filter_message().endpoint(update_location))
}

pub async fn chat_member(mmbr: ChatMemberUpdated, state: StateManager) -> HandlerResult {
    let new_member = mmbr.new_chat_member.clone();

    if new_member.kind != ChatMemberKind::Member {
        log::info!(
            "user {} {} left",
            new_member.user.full_name(),
            new_member.user.id
        );
        state.remove(new_member.user.id.into()).await;
    }

    Ok(())
}

pub async fn update_location(bot: Bot, msg: Message, state: StateManager) -> HandlerResult {
    if let Some(loc) = msg.location() {
        let location = Location::new(loc.latitude, loc.longitude);
        log::info!(
            "{} {} registered location {:?}",
            msg.from()
                .map(|u| u.full_name())
                .unwrap_or("unknown".to_string()),
            msg.chat.id,
            location
        );
        state.add(msg.chat.id, location, None).await;
        bot.send_message(msg.chat.id, "Awesome! Location updated")
            .await
            .unwrap();
    } else {
        bot.send_message(
            msg.chat.id,
            "That was not a location, sorry... Send me your location so \
            I can monitor a weather for you",
        )
        .await
        .unwrap();
    }

    Ok(())
}

pub async fn start(bot: Bot, msg: Message) -> HandlerResult {
    log::info!(
        "{} {} joined",
        msg.from()
            .map(|u| u.full_name())
            .unwrap_or("unknown".to_string()),
        msg.chat.id
    );

    let button =
        KeyboardButton::new("I'M HERE (WORKS ONLY ON MOBILE)").request(ButtonRequest::Location);

    bot.send_message(
        msg.chat.id,
        "Hey! Send me your location so I can monitor a weather for you",
    )
    .reply_markup(KeyboardMarkup::new(vec![vec![button]]))
    .await
    .unwrap();

    Ok(())
}

pub fn check_forecast_and_notify_if_rain(
    bot: Bot,
) -> impl Fn(ChatId, Location, u64) -> Async<Result<bool, CheckError>> {
    move |chat_id: ChatId, loc: Location, time: u64| {
        let bot = bot.clone();

        let result = async move {
            let params = [
                ("lat", loc.lat().to_string()),
                ("lon", loc.lon().to_string()),
                ("time", time.to_string()),
                ("allow_absent_alert", true.to_string()),
                ("lang", "en".to_owned()),
            ];
            let url = "https://yandex.ee/weather/front/maps/prec-alert";
            let url = reqwest::Url::parse_with_params(url, &params)?;

            log::info!("{} request {}", chat_id, url);

            let response = reqwest::get(url).await?.json::<WeatherResponse>().await?;

            log::info!("{} response {:?}", chat_id, response);

            if response.alert.typ != "noprec" {
                let url = format!(
                    "https://yandex.ee/weather/maps/nowcast?lat={lat}&lon={lon}&z=9&le_Lightning=1",
                    lat = loc.lat(),
                    lon = loc.lon(),
                );

                let client = ClientBuilder::native()
                    .connect("http://127.0.0.1:4444")
                    .await?;

                client.goto(url.as_str()).await?;
                client.set_window_size(1920, 1080).await?;
                tokio::time::sleep(Duration::from_secs(30)).await;
                let png_data = client.screenshot().await?;

                bot.send_photo(chat_id, InputFile::memory(png_data))
                    .caption(format!("Oops! {}\n{}", response.alert.title, url))
                    .await?;

                client.close().await?;

                Ok(true)
            } else {
                Ok(false)
            }
        };

        Box::pin(result)
    }
}
