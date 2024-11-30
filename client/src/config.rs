use rtherm_common::ChannelId;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
use tokio::fs;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    W1Therm,
    Dummy,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub prefix: String,
    pub server: String,
    pub period: f64,
    pub providers: HashSet<ProviderKind>,
    pub name_map: HashMap<String, ChannelId>,
}

impl Config {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let bytes = fs::read(path).await.map_err(|e| format!("{e}"))?;
        let text = String::from_utf8(bytes).map_err(|e| format!("{e}"))?;
        toml::from_str(&text).map_err(|e| format!("{e}"))
    }
}
