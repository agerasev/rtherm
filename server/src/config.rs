#![allow(dead_code)]

use rtherm_common::ChannelId;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::{collections::HashSet, path::Path};
use tokio::fs;

#[derive(Clone, Default, Debug, Deserialize)]
pub struct DashboardConfig {
    #[serde(default)]
    pub devices: Vec<DeviceConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeviceConfig {
    pub name: String,
    pub sensors: BTreeMap<ChannelId, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub user: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SqliteConfig {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DbConfig {
    pub postgres: Option<PostgresConfig>,
    pub sqlite: Option<SqliteConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
    /// Empty or omitted means that nobody can use the bot.
    #[serde(default)]
    pub allowed_user_ids: HashSet<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    #[default]
    Mem,
    Fs,
    Db,
}

#[derive(Clone, Default, Debug, Deserialize)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub type_: StorageType,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub http: HttpConfig,
    pub db: Option<DbConfig>,
    pub telegram: Option<TelegramConfig>,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub dashboard: DashboardConfig,
}

impl Config {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let bytes = fs::read(path).await.map_err(|e| format!("{e}"))?;
        let text = String::from_utf8(bytes).map_err(|e| format!("{e}"))?;
        let config: Self = toml::from_str(&text).map_err(|e| format!("{e}"))?;
        let mut channels = HashSet::new();
        for device in &config.dashboard.devices {
            if device.name.trim().is_empty() {
                return Err("Dashboard device name cannot be empty".into());
            }
            for (channel, name) in &device.sensors {
                if name.trim().is_empty() || !channels.insert(channel) {
                    return Err(format!(
                        "Empty label or repeated dashboard sensor: {channel}"
                    ));
                }
            }
        }
        Ok(config)
    }
}
