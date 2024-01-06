use serde::Deserialize;
use std::path::Path;
use tokio::fs;

#[derive(Clone, Debug, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub http: HttpConfig,
    pub telegram: TelegramConfig,
}

impl Config {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let bytes = fs::read(path).await.map_err(|e| format!("{e}"))?;
        let text = String::from_utf8(bytes).map_err(|e| format!("{e}"))?;
        toml::from_str(&text).map_err(|e| format!("{e}"))
    }
}
