use crate::{config::TelegramConfig, recepient::Recepient, statistics::ChannelHistory};
use frankenstein::{
    AllowedUpdate, AsyncApi, AsyncTelegramApi, GetUpdatesParams, Message, ParseMode,
    SendMessageParams, UpdateContent,
};
use rtherm_common::{ChannelId, Measurements};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    ops::RangeInclusive,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::RwLock, task::spawn, time::sleep};

type ChatId = i64;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CommonSettings {
    /// Time to assume that channel is offline.
    offline_timeout: Duration,
    /// Offset from normal range bound when value becomes normal again.
    hysteresis: f64,
}

pub trait RangeExt {
    type Item: Copy;
    fn widen(&self, offset: Self::Item) -> Self;
    fn contains_range(&self, other: &Self) -> bool;
    fn display(&self) -> String;
}

impl RangeExt for RangeInclusive<f64> {
    type Item = f64;
    fn widen(&self, offset: f64) -> Self {
        if self.start() - 2.0 * offset > *self.end() {
            let center = 0.5 * (self.start() + self.end());
            center..=center
        } else {
            (self.start() - offset)..=(self.end() + offset)
        }
    }
    fn contains_range(&self, other: &Self) -> bool {
        self.start() <= other.start() && other.end() <= self.end()
    }
    fn display(&self) -> String {
        if self.start() == self.end() {
            format!("{}", self.start())
        } else {
            format!("[{}, {}]", self.start(), self.end())
        }
    }
}

impl Default for CommonSettings {
    fn default() -> Self {
        Self {
            offline_timeout: Duration::from_secs(2 * 60),
            hysteresis: 5.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChannelSettings {
    /// Range of good values for a channel.
    ///
    /// Values outside of this range considered to be bad.
    normal_range: RangeInclusive<f64>,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            normal_range: 30.0..=80.0,
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

impl ChannelState {
    fn digest(&self) -> String {
        self.values.statistics().to_string()
    }
    fn last_value_text(&self) -> String {
        if let Some(point) = self.values.statistics().last {
            return format!("{:.1} °C", point.value);
        }
        return "offline".into();
    }
}

impl State {
    fn digest(&self) -> String {
        if !self.channels.is_empty() {
            self.channels
                .iter()
                .map(|(id, channel)| format!("/digest_{id}: {}\n", channel.last_value_text()))
                .collect()
        } else {
            "No active channels".to_string()
        }
    }
}

#[derive(Clone, Debug)]
enum Command {
    Help,
    Digest { channel: Option<ChannelId> },
    Subscribe { channel: Option<ChannelId> },
    Unsubscribe { channel: Option<ChannelId> },
}

impl Command {
    const HELP: &'static str = r#"Available commands:
/help - display this text.
/digest - show info about all channels or a selected one.
/subscribe - subscribe to a specified channel.
/unsubscribe - unsubscribe from a previously subscribed channel.
"#;
}

impl FromStr for Command {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = if s.contains('_') {
            Cow::Owned(s.replace('_', " "))
        } else {
            Cow::Borrowed(s)
        };
        let mut args = s.split_whitespace();
        let cmd = args.next().ok_or("Empty command")?;
        if !cmd.starts_with('/') {
            return Err("Command must start with '/'".into());
        }
        let make_opt_chid = |s: Option<&str>| -> Result<Option<ChannelId>, String> {
            if let Some(s) = s {
                ChannelId::try_from(s)
                    .map_err(|e| e.to_string())
                    .map(|s| Some(s))
            } else {
                Ok(None)
            }
        };
        let ret = match &cmd[1..] {
            "start" | "help" => Self::Help,
            "digest" => Self::Digest {
                channel: make_opt_chid(args.next())?,
            },
            "subscribe" => Self::Subscribe {
                channel: make_opt_chid(args.next())?,
            },
            "unsubscribe" => Self::Unsubscribe {
                channel: make_opt_chid(args.next())?,
            },
            other => return Err(format!("Unknown command: {other}")),
        };
        if let Some(extra) = args.next() {
            return Err(format!("Unexpected argument: {extra}"));
        }
        Ok(ret)
    }
}

#[derive(Clone)]
pub struct Telegram {
    api: AsyncApi,
    state: SharedState,
}

type Error = <AsyncApi as AsyncTelegramApi>::Error;

async fn send_message(api: &AsyncApi, chat: ChatId, text: impl Into<String>) -> Result<(), Error> {
    api.send_message(
        &SendMessageParams::builder()
            .parse_mode(ParseMode::Html)
            .chat_id(chat)
            .text(text)
            .build(),
    )
    .await?;
    Ok(())
}

impl Telegram {
    pub async fn new(config: TelegramConfig) -> Self {
        let this = Self {
            api: AsyncApi::new(&config.token),
            state: SharedState::default(),
        };
        spawn(this.clone().poll());
        spawn(this.clone().monitor());
        this
    }

    async fn process_message(&self, msg: Message) -> Result<(), Error> {
        let chat = msg.chat.id;
        if let Some(text) = msg.text {
            match Command::from_str(&text) {
                Ok(cmd) => self.process_command(chat, cmd).await?,
                Err(reason) => send_message(&self.api, chat, format!("Error: {reason}")).await?,
            }
        } else {
            send_message(&self.api, chat, "Error: Only text commands are supported").await?;
        };
        Ok(())
    }

    async fn poll(self) -> ! {
        let mut params = GetUpdatesParams::builder()
            .allowed_updates(vec![AllowedUpdate::Message])
            .build();
        loop {
            let update = match self.api.get_updates(&params).await {
                Ok(u) => u,
                Err(err) => {
                    log::error!("Cannot get updates: {err}");
                    continue;
                }
            };
            for update in update.result {
                params.offset = Some(update.update_id as i64 + 1);
                match update.content {
                    UpdateContent::Message(msg) => {
                        if let Err(err) = self.process_message(msg).await {
                            log::error!("Error processing message: {err}");
                        }
                    }
                    _ => {
                        log::error!("Unexpected content type");
                        continue;
                    }
                }
            }
        }
    }

    async fn process_command(&self, chat_id: ChatId, cmd: Command) -> Result<(), Error> {
        match cmd {
            Command::Help => send_message(&self.api, chat_id, Command::HELP).await?,
            Command::Digest { channel } => {
                let state = self.state.read().await;
                send_message(
                    &self.api,
                    chat_id,
                    if let Some(id) = channel {
                        match state.channels.get(&id) {
                            Some(chan) => chan.digest(),
                            None => format!("Error: No such channel <code>{id}</code>"),
                        }
                    } else {
                        state.digest()
                    },
                )
                .await?;
            }
            Command::Subscribe { channel } => {
                if let Some(channel) = channel {
                    let mut state = self.state.write().await;
                    let chat = state.chats.entry(chat_id).or_default();
                    let entry = chat.subscriptions.entry(channel.clone());
                    let done = matches!(&entry, Entry::Vacant(..));
                    entry.or_default();
                    send_message(
                        &self.api,
                        chat_id,
                        format!(
                            "You {} to channel <code>{}</code>.",
                            if done {
                                "have successfully subscribed"
                            } else {
                                "are already subscribed"
                            },
                            channel
                        ),
                    )
                    .await?
                } else {
                    let state = self.state.read().await;
                    send_message(
                        &self.api,
                        chat_id,
                        format!(
                            "Please provide the channel name. For example:\n{}",
                            state
                                .channels
                                .keys()
                                .map(|id| format!("/subscribe_{id}\n"))
                                .collect::<String>()
                        ),
                    )
                    .await?
                }
            }
            Command::Unsubscribe { channel } => {
                if let Some(channel) = channel {
                    let mut state = self.state.write().await;
                    let chat = state.chats.entry(chat_id).or_default();
                    let done = chat.subscriptions.remove(&channel).is_some();
                    send_message(
                        &self.api,
                        chat_id,
                        format!(
                            "You {} channel <code>{}</code>.",
                            if done {
                                "have successfully unsubscribed from"
                            } else {
                                "are not subscribed to"
                            },
                            channel
                        ),
                    )
                    .await?
                } else {
                    let state = self.state.read().await;
                    let channels = match state.chats.get(&chat_id) {
                        Some(chat) => chat.subscriptions.keys().collect::<Vec<_>>(),
                        None => Vec::new(),
                    };
                    send_message(
                        &self.api,
                        chat_id,
                        if channels.is_empty() {
                            "You have not subscribed to any channel yet.".to_string()
                        } else {
                            format!(
                                "Please provide the channel name. For example:\n{}",
                                channels
                                    .into_iter()
                                    .map(|id| format!("/unsubscribe_{id}\n"))
                                    .collect::<String>()
                            )
                        },
                    )
                    .await?
                }
            }
        }
        Ok(())
    }

    async fn monitor(self) -> ! {
        let settings = self.state.read().await.settings.clone();

        loop {
            sleep(settings.offline_timeout / 2).await;

            let mut messages = Vec::<(ChatId, String)>::new();

            {
                let state = self.state.read().await;
                let now = Instant::now();
                for (channel_id, channel) in state.channels.iter() {
                    if let Some(last_update) = channel.last_update {
                        if last_update + settings.offline_timeout > now {
                            continue;
                        }
                    }
                    for (&chat_id, chat) in state.chats.iter() {
                        if chat.subscriptions.contains_key(channel_id) {
                            messages.push((
                                chat_id,
                                format!("<b>Alert!</b>\n<code>{}</code> is offline", channel_id),
                            ));
                        }
                    }
                }
            }

            for (chat_id, message) in messages {
                if let Err(err) = send_message(&self.api, chat_id, message).await {
                    log::error!("Cannot send message: {err}");
                }
            }
        }
    }
}

impl Recepient for Telegram {
    type Error = Error;

    async fn update(&mut self, measurements: Measurements) -> Vec<Error> {
        let Self { api, state, .. } = self;
        let mut messages = Vec::<(ChatId, String)>::new();

        {
            let mut state = state.write().await;
            let settings = state.settings.clone();
            let now = Instant::now();

            for (channel_id, points) in measurements {
                if points.is_empty() {
                    continue;
                }
                let value_range = points
                    .iter()
                    .map(|p| p.value)
                    .fold(f64::INFINITY..=f64::NEG_INFINITY, |range, value| {
                        range.start().min(value)..=range.end().max(value)
                    });
                let channel = state.channels.entry(channel_id.clone()).or_default();
                channel.values.update(points);
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
                                format!(
                                    "<code>{}</code> is online (value: {}).",
                                    channel_id,
                                    value_range.display(),
                                ),
                            ));
                        }
                        if !sub.is_bad {
                            if !&sub.settings.normal_range.contains_range(&value_range) {
                                sub.is_bad = true;
                                messages.push((
                                    chat_id,
                                    format!(
                                        "<b>Alert!</b>\n<code>{}</code> value {} is out of normal range {:?}.",
                                        channel_id,
                                        value_range.display(),
                                        sub.settings.normal_range,
                                    ),
                                ));
                            }
                        } else if sub
                            .settings
                            .normal_range
                            .widen(-settings.hysteresis)
                            .contains_range(&value_range)
                        {
                            sub.is_bad = false;
                            messages.push((
                                chat_id,
                                format!(
                                    "<code>{}</code> value {} returned to normal range {:?}.",
                                    channel_id,
                                    value_range.display(),
                                    sub.settings.normal_range,
                                ),
                            ));
                        }
                    }
                }
            }
        }

        let mut errors = Vec::new();
        for (chat_id, message) in messages {
            if let Err(err) = send_message(&api, chat_id, message).await {
                errors.push(err);
            }
        }

        errors
    }
}
