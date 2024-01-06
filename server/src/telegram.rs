use crate::{
    config::TelegramConfig,
    db::{DbHandle, DB},
};
use rtherm_common::Temperature as Temp;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::{task::spawn, time::sleep};

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "read all sensors.")]
    Read,
    #[command(description = "subscribe to alerts and daily digest.")]
    Subscribe,
}

async fn digest(db: &DbHandle) -> String {
    let sensors = db
        .read()
        .await
        .sensors
        .iter()
        .map(|(id, sensor)| (id.clone(), sensor.stats()))
        .collect::<HashMap<_, _>>();
    format!("{:?}", sensors)
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let db = DB.get().unwrap().clone();
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::Read => bot.send_message(msg.chat.id, digest(&db).await).await?,
        Command::Subscribe => {
            bot.send_message(
                msg.chat.id,
                if db.write().await.subscribers.insert(msg.chat.id) {
                    "You have successfully subscribed."
                } else {
                    "You are already subscribed."
                },
            )
            .await?
        }
    };

    Ok(())
}

pub async fn run(config: TelegramConfig, db: DbHandle) {
    assert!(Arc::ptr_eq(&db, DB.get().unwrap()));
    println!("Starting telegram bot...");
    let bot = Bot::new(config.token);
    spawn(monitor(bot.clone(), db));
    Command::repl(bot, answer).await;
}

const TEMP_THRESHOLD: Temp = 30.0;
const TEMP_HYSTERESYS: Temp = 5.0;
const OFFLINE_TIMEOUT: Duration = Duration::from_secs(60);
const DIGEST_PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

async fn monitor(bot: Bot, db: DbHandle) -> ResponseResult<()> {
    let mut last_digest = SystemTime::now();
    loop {
        sleep(Duration::from_secs(30)).await;
        let mut db = db.write().await;
        let mut messages = Vec::<String>::new();

        for (id, sensor) in db.sensors.iter_mut() {
            if !sensor.flags.low_temp {
                if sensor.last().value < TEMP_THRESHOLD {
                    sensor.flags.low_temp = true;
                    messages.push(format!(
                        "Alert! Sensor `{}` temperature is lower than {} C.",
                        id, TEMP_THRESHOLD,
                    ));
                }
            } else if sensor.last().value > TEMP_THRESHOLD + TEMP_HYSTERESYS {
                sensor.flags.low_temp = false;
            }

            if sensor.flags.online && sensor.last().time + OFFLINE_TIMEOUT < SystemTime::now() {
                sensor.flags.online = false;
                messages.push(format!("Alert! Sensor `{}` is offline.", id));
            }

            if last_digest + DIGEST_PERIOD < SystemTime::now() {
                last_digest = SystemTime::now();
                messages.push("/read".into());
            }
        }

        for msg in messages {
            if let Err(err) = send_to_all(&bot, db.subscribers.iter().copied(), msg).await {
                println!("Error sending notification: {}", err);
            }
        }
    }
}

async fn send_to_all(
    bot: &Bot,
    chats: impl IntoIterator<Item = ChatId>,
    message: impl Into<String>,
) -> ResponseResult<()> {
    let mut res = Ok(());
    let message = message.into();
    for chat in chats.into_iter() {
        if let Err(err) = bot.send_message(chat, &message).await {
            res = Err(err);
        }
    }
    res
}
