use crate::{
    config::TelegramConfig,
    db::{Db, DbHandle, Id, Values},
};
use std::{
    collections::{hash_map::Entry, HashMap},
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
    #[command(description = "show all sensors data.")]
    Digest,
    #[command(description = "subscribe to alerts and daily digest.")]
    Subscribe,
}

fn make_digest(db: &Db) -> String {
    let mut text = String::new();
    for (id, sensor) in db.sensors.iter() {
        let stats = sensor.values.stats();
        text = format!("{}\n`{}`:\n{}", text, id, stats);
    }
    text
}

async fn answer(bot: Bot, msg: Message, cmd: Command, db: DbHandle) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::Digest => {
            bot.send_message(msg.chat.id, make_digest(&*db.read().await))
                .await?
        }
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
    println!("Starting telegram bot ...");
    let bot = Bot::new(config.token);
    spawn(monitor(bot.clone(), db.clone()));
    Command::repl(bot, move |bot, msg, cmd| answer(bot, msg, cmd, db.clone())).await;
}

#[derive(Default, Debug)]
pub struct SensorState {
    pub is_offline: bool,
    pub is_low: bool,
}

const MONITOR_PERIOD: Duration = Duration::from_secs(10);
const DIGEST_PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

async fn monitor(bot: Bot, db: DbHandle) -> ResponseResult<()> {
    let mut last_digest = SystemTime::now();
    let mut sensors_state = HashMap::<Id, SensorState>::new();

    loop {
        sleep(MONITOR_PERIOD).await;

        let mut messages = Vec::<String>::new();
        let now = SystemTime::now();

        for (id, sensor) in db.read().await.sensors.iter() {
            let last = match sensor.values.last() {
                Some(m) => m,
                None => continue,
            };
            let settings = &sensor.settings;

            let state = match sensors_state.entry(id.clone()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(SensorState::default()),
            };

            if !state.is_low {
                if last.value < settings.low_temp {
                    state.is_low = true;
                    messages.push(format!(
                        "**Alert!**\n`{}` temperature is lower than **{}** °C.",
                        id, settings.low_temp,
                    ));
                }
            } else if last.value >= settings.safe_temp {
                state.is_low = false;
            }

            if !state.is_offline && last.time + settings.timeout <= now {
                state.is_offline = true;
                messages.push(format!("**Alert!**\n`{}` is offline.", id));
            }
            if state.is_offline && last.time + settings.timeout > now {
                state.is_offline = false;
                messages.push(format!("`{}` is online again.", id));
            }
        }

        if last_digest + DIGEST_PERIOD <= now {
            let mut db = db.write().await;
            last_digest = now;
            messages.push(make_digest(&db));
            for sensor in db.sensors.values_mut() {
                sensor.values = Values::default();
            }
        }

        {
            let db = db.read().await;
            for msg in messages {
                if let Err(err) = send_to_all(&bot, db.subscribers.iter().copied(), msg).await {
                    println!("Error sending notification: {}", err);
                }
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
