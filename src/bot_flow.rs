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
        log::info!("user {} {} left", mmbr.from.full_name(), mmbr.chat.id);
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
        state.add_new(msg.chat.id, location).await;
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
    url_to_pic: String,
) -> impl Fn(ChatId, Location, u64) -> Async<Result<bool, CheckError>> {
    move |chat_id: ChatId, loc: Location, time: u64| {
        let bot = bot.clone();
        let url_to_pic = url_to_pic.clone();

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
            let alert = response.alert.clone();

            log::info!("{} response {:?}", chat_id, response);

            if alert.typ != "noprec" {
                let screenshot_url = format!(
                    "https://yandex.ee/weather/maps/nowcast?lat={lat}&lon={lon}&z=9&le_Lightning=1",
                    lat = loc.lat(),
                    lon = loc.lon(),
                );

                let params = [
                    ("url", screenshot_url.clone()),
                    ("height", "1080".to_string()),
                    ("mobile", "0".to_string()),
                    ("allocated_time", "5".to_string()),
                    ("width", "1920".to_string()),
                    ("base64", "0".to_string()),
                ];

                let url = "https://url-to-screenshot.p.rapidapi.com/get";
                let url = reqwest::Url::parse_with_params(url, &params)?;

                let client = reqwest::Client::new();

                let maybe_response = client
                    .get(url)
                    .header("Accept", "image/png")
                    .header("X-RapidAPI-Key", url_to_pic.as_str())
                    .header("X-RapidAPI-Host", "url-to-screenshot.p.rapidapi.com")
                    .send()
                    .await;

                match maybe_response {
                    Ok(response) => {
                        let maybe_img_bytes = response.bytes().await;

                        match maybe_img_bytes {
                            Ok(bytes) => {
                                let _ = bot
                                    .send_photo(chat_id, InputFile::memory(bytes))
                                    .caption(format!("Oops! {}\n{}", alert.title, screenshot_url))
                                    .await;
                            }

                            Err(_) => {
                                let _ = bot
                                    .send_message(
                                        chat_id,
                                        format!("Oops! {}\n{}", alert.title, screenshot_url),
                                    )
                                    .await;
                            }
                        }
                    }

                    Err(_) => {
                        let _ = bot
                            .send_message(
                                chat_id,
                                format!("Oops! {}\n{}", alert.title, screenshot_url),
                            )
                            .await;
                    }
                }

                Ok(true)
            } else {
                Ok(false)
            }
        };

        Box::pin(result)
    }
}
