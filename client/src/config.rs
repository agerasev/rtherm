use rtherm_common::ChannelId;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
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
    #[serde(
        default = "default_connect_timeout",
        deserialize_with = "timeout_seconds"
    )]
    pub connect_timeout: Duration,
    #[serde(
        default = "default_request_timeout",
        deserialize_with = "timeout_seconds"
    )]
    pub request_timeout: Duration,
    #[serde(default = "default_buffer_path", deserialize_with = "buffer_path")]
    pub buffer_path: PathBuf,
    #[serde(default = "default_batch_size", deserialize_with = "batch_size")]
    pub batch_size: u16,
    #[serde(
        default = "default_retry_interval",
        deserialize_with = "timeout_seconds"
    )]
    pub retry_interval: Duration,
    pub period: f64,
    pub providers: HashSet<ProviderKind>,
    pub name_map: HashMap<String, ChannelId>,
}

fn default_buffer_path() -> PathBuf {
    PathBuf::from("rtherm-buffer.sqlite3")
}

fn buffer_path<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    let path = PathBuf::deserialize(deserializer)?;
    if path.as_os_str().is_empty() || path == Path::new(":memory:") {
        return Err(serde::de::Error::custom(
            "buffer_path must name a persistent file",
        ));
    }
    Ok(path)
}

fn default_batch_size() -> u16 {
    500
}

fn batch_size<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let size = u16::deserialize(deserializer)?;
    if !(1..=1000).contains(&size) {
        return Err(serde::de::Error::custom(
            "batch_size must be between 1 and 1000",
        ));
    }
    Ok(size)
}

fn default_retry_interval() -> Duration {
    Duration::from_secs(5)
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_request_timeout() -> Duration {
    Duration::from_secs(30)
}

fn timeout_seconds<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Duration, D::Error> {
    let seconds = f64::deserialize(deserializer)?;
    let timeout = Duration::try_from_secs_f64(seconds).map_err(serde::de::Error::custom)?;
    if timeout.is_zero() {
        return Err(serde::de::Error::custom(
            "timeout must be greater than zero",
        ));
    }
    Ok(timeout)
}

impl Config {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Config, String> {
        let bytes = fs::read(path).await.map_err(|e| format!("{e}"))?;
        let text = String::from_utf8(bytes).map_err(|e| format!("{e}"))?;
        toml::from_str(&text).map_err(|e| format!("{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
        prefix = "Test"
        server = "http://127.0.0.1:4100"
        period = 60
        providers = ["dummy"]
        name_map = {}
    "#;

    #[test]
    fn existing_configs_get_finite_timeouts() {
        let config: Config = toml::from_str(CONFIG).unwrap();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.buffer_path, PathBuf::from("rtherm-buffer.sqlite3"));
        assert_eq!(config.batch_size, 500);
        assert_eq!(config.retry_interval, Duration::from_secs(5));
    }

    #[test]
    fn buffer_settings_reject_volatile_paths_and_unbounded_batches() {
        for value in ["0", "1001", "-1"] {
            assert!(toml::from_str::<Config>(&format!("{CONFIG}\nbatch_size = {value}")).is_err());
        }
        for value in ["", ":memory:"] {
            assert!(
                toml::from_str::<Config>(&format!("{CONFIG}\nbuffer_path = {value:?}")).is_err()
            );
        }
    }

    #[test]
    fn timeouts_accept_seconds() {
        let config: Config = toml::from_str(&format!(
            "{CONFIG}\nconnect_timeout = 2\nrequest_timeout = 4.5"
        ))
        .unwrap();
        assert_eq!(config.connect_timeout, Duration::from_secs(2));
        assert_eq!(config.request_timeout, Duration::from_millis(4500));
    }

    #[test]
    fn timeouts_must_be_positive_and_finite() {
        for field in ["connect_timeout", "request_timeout", "retry_interval"] {
            for value in ["0", "-1", "inf", "nan", "1e30"] {
                assert!(
                    toml::from_str::<Config>(&format!("{CONFIG}\n{field} = {value}")).is_err(),
                    "accepted {field} = {value}"
                );
            }
        }
    }
}
