use crate::{
    config::TelegramConfig,
    recepient::{ChannelId, Recepient},
    statistics::ChannelHistory,
};
use rtherm_common::Measurement;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry, HashMap},
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};
use teloxide::{prelude::*, types::ChatId, utils::command::BotCommands, RequestError};
use tokio::{sync::RwLock, task::spawn, time::sleep};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CommonSettings {
    /// Time to assume that channel is offline.
    offline_timeout: Duration,
    /// Offset from normal range bound when value becomes normal again.
    hysteresis: f64,
}

fn widen_range(range: Range<f64>, offset: f64) -> Range<f64> {
    (range.start - offset)..(range.end + offset)
}

impl Default for CommonSettings {
    fn default() -> Self {
        Self {
            offline_timeout: Duration::from_secs(60),
            hysteresis: 5.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChannelSettings {
    /// Range of good values for a channel.
    ///
    /// Values outside of this range considered to be bad.
    normal_range: Range<f64>,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            normal_range: 30.0..80.0,
        }
    }
}

#[derive(Clone, Default, Debug)]
struct ChannelSubscription {
    settings: ChannelSettings,
    is_bad: bool,
}

#[derive(Clone, Default, Debug)]
struct ChannelState {
    values: ChannelHistory,
    last_update: Option<Instant>,
}

#[derive(Default, Debug)]
struct Chat {
    subscriptions: HashMap<ChannelId, ChannelSubscription>,
}

#[derive(Default, Debug)]
struct State {
    settings: CommonSettings,
    channels: HashMap<ChannelId, ChannelState>,
    chats: HashMap<ChatId, Chat>,
}

type SharedState = Arc<RwLock<State>>;

impl State {
    fn digest(&self) -> String {
        let mut text = String::new();
        for (id, channel) in self.channels.iter() {
            let stats = channel.values.statistics();
            text = format!("{}\n`{}`:\n{}", text, id, stats);
        }
        text
    }
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "show all available channels.")]
    Channels,
    #[command(description = "subscribe to specific channel.")]
    Subscribe { channel: ChannelId },
}

async fn answer(bot: Bot, msg: Message, cmd: Command, state: SharedState) -> ResponseResult<()> {
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?
        }
        Command::Channels => {
            let state = state.read().await;
            bot.send_message(msg.chat.id, state.digest()).await?
        }
        Command::Subscribe { channel } => {
            let mut state = state.write().await;
            let chat = state.chats.entry(msg.chat.id).or_default();
            let entry = chat.subscriptions.entry(channel.clone());
            let subscribed = matches!(&entry, Entry::Vacant(..));
            entry.or_default();
            bot.send_message(
                msg.chat.id,
                format!(
                    "You {} to channel `{}`.",
                    if subscribed {
                        "have successfully subscribed"
                    } else {
                        "are already subscribed"
                    },
                    channel
                ),
            )
            .await?
        }
    };

    Ok(())
}

pub struct Telegram {
    bot: Bot,
    state: SharedState,
}

impl Telegram {
    pub async fn new(config: TelegramConfig) -> Self {
        let state = SharedState::default();
        let bot = Bot::new(config.token);
        spawn(monitor(bot.clone(), state.clone()));
        spawn(Command::repl(bot.clone(), {
            let state = state.clone();
            move |bot, msg, cmd| answer(bot, msg, cmd, state.clone())
        }));
        Self { bot, state }
    }
}

impl Recepient for Telegram {
    type Error = RequestError;

    async fn update(&mut self, channel_id: ChannelId, meas: Measurement) -> ResponseResult<()> {
        let Self { bot, state } = self;
        let mut messages = Vec::<(ChatId, String)>::new();

        {
            let mut state = state.write().await;
            let settings = state.settings.clone();
            let now = Instant::now();

            let channel = state.channels.entry(channel_id.clone()).or_default();
            channel.values.update(meas);
            let become_online = match channel.last_update {
                Some(last_update) => last_update + settings.offline_timeout < now,
                None => true,
            };
            channel.last_update = Some(now);

            for (&chat_id, chat) in state.chats.iter_mut() {
                if let Some(sub) = chat.subscriptions.get_mut(&channel_id) {
                    if become_online {
                        messages.push((
                            chat_id,
                            format!("`{}` is online (value {}).", channel_id, meas.value),
                        ));
                    }
                    if !sub.is_bad {
                        if !sub.settings.normal_range.contains(&meas.value) {
                            sub.is_bad = true;
                            messages.push((
                                chat_id,
                                format!(
                                    "**Alert!**\n`{}` value {} is out of normal range {:?}.",
                                    channel_id, meas.value, sub.settings.normal_range
                                ),
                            ));
                        }
                    } else if widen_range(sub.settings.normal_range.clone(), -settings.hysteresis)
                        .contains(&meas.value)
                    {
                        sub.is_bad = false;
                        messages.push((
                            chat_id,
                            format!(
                                "`{}` value {} returned to normal range {:?}.",
                                channel_id, meas.value, sub.settings.normal_range
                            ),
                        ));
                    }
                }
            }
        }

        for (chat_id, message) in messages {
            bot.send_message(chat_id, message).await?;
        }

        Ok(())
    }
}

async fn monitor(bot: Bot, state: SharedState) -> ResponseResult<()> {
    let settings = state.read().await.settings.clone();

    loop {
        sleep(settings.offline_timeout / 2).await;

        let mut messages = Vec::<(ChatId, String)>::new();

        {
            let state = state.read().await;
            let now = Instant::now();
            for (channel_id, channel) in state.channels.iter() {
                if let Some(last_update) = channel.last_update {
                    if last_update + settings.offline_timeout > now {
                        continue;
                    }
                }
                for (&chat_id, chat) in state.chats.iter() {
                    if chat.subscriptions.contains_key(channel_id) {
                        messages
                            .push((chat_id, format!("**Alert!**\n`{}` is offline", channel_id)));
                    }
                }
            }
        }

        for (chat_id, message) in messages {
            bot.send_message(chat_id, message).await?;
        }
    }
}
