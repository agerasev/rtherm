use serde::{Deserialize, Serialize};
use std::{fmt::Display, path::Path};
use tokio::fs;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub server: String,
}

fn display<T: Display>(v: T) -> String {
    format!("{}", v)
}

impl Config {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let bytes = fs::read(path).await.map_err(display)?;
        let text = String::from_utf8(bytes).map_err(display)?;
        toml::from_str(&text).map_err(display)
    }
}
