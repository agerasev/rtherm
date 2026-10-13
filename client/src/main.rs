mod config;
#[cfg(feature = "dummy")]
mod dummy;
mod provider;
mod storage;
#[cfg(feature = "w1_therm")]
mod w1_therm;

use crate::config::Config;
use config::ProviderKind;
use provider::{AnyProvider, Provider};
use reqwest::Client;
use rtherm_common::{ChannelId, Measurements, ProvideRequest};
use std::{
    collections::hash_map::Entry,
    env,
    time::{Duration, Instant},
};
use storage::Storage;
use tokio::{sync::Notify, time::sleep};

#[tokio::main]
async fn main() -> ! {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    log::info!("Config: {:?}", config);

    let mut providers = Vec::<AnyProvider>::new();
    #[cfg(feature = "w1_therm")]
    if config.providers.contains(&ProviderKind::W1Therm) {
        providers.push(AnyProvider::new(w1_therm::W1Therm));
        log::info!("W1Therm provider created");
    }
    #[cfg(feature = "dummy")]
    if config.providers.contains(&ProviderKind::Dummy) {
        providers.push(AnyProvider::new(dummy::Dummy::default()));
        log::info!("Dummy provider created");
    }

    // Fail startup rather than silently falling back to volatile storage.
    let storage = Storage::open(&config.buffer_path)
        .await
        .expect("Cannot open persistent buffer");
    log::info!(
        "Persistent buffer opened at '{}': {} readings pending",
        config.buffer_path.display(),
        storage
            .count()
            .await
            .expect("Cannot read persistent buffer")
    );
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(config.connect_timeout)
        .timeout(config.request_timeout)
        .build()
        .expect("Cannot create HTTP client");
    let ready = Notify::new();

    // Poll both tasks together so an unexpected failure cannot silently leave half the client running.
    tokio::select! {
        _ = sample(&config, providers, &storage, &ready) => unreachable!(),
        _ = upload(&config, client, &storage, &ready) => unreachable!(),
    }
}

async fn sample(
    config: &Config,
    mut providers: Vec<AnyProvider>,
    storage: &Storage,
    ready: &Notify,
) -> ! {
    log::info!("Measurement task started");
    let period = Duration::from_secs_f64(config.period);
    loop {
        let started = Instant::now();
        log::info!("Sampling started");
        let (raw, errors) = providers.measure().await;
        log::info!(
            "Sampling completed: {} readings from {} channels in {:.3}s ({} errors)",
            raw.values().map(Vec::len).sum::<usize>(),
            raw.len(),
            started.elapsed().as_secs_f64(),
            errors.len()
        );
        for err in errors {
            log::error!("Provider error: {err}");
        }
        let measurements = map_channels(config, raw);
        let count = measurements.values().map(Vec::len).sum::<usize>();
        if count > 0 {
            // Keep this cycle in memory and pause sampling until it can be committed.
            // Uploading already committed readings can continue and free disk space.
            while let Err(err) = storage.store(&measurements).await {
                log::error!(
                    "Cannot persist {count} readings: {err}; sampling paused, retrying in {:.3}s",
                    config.retry_interval.as_secs_f64()
                );
                sleep(config.retry_interval).await;
            }
            log::info!("Buffered {count} readings in SQLite");
            ready.notify_one();
        }
        sleep(period).await;
    }
}

fn map_channels(config: &Config, raw: Measurements<String>) -> Measurements {
    let mut measurements = Measurements::new();
    for (channel, values) in raw {
        let name = config
            .name_map
            .get(&channel)
            .map(ToString::to_string)
            .unwrap_or_else(|| channel.replace('-', ""));
        let id = match ChannelId::try_from(format!("{}{name}", config.prefix)) {
            Ok(id) => id,
            Err(err) => {
                log::error!("Bad channel name: {err}");
                continue;
            }
        };
        match measurements.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(values);
            }
            Entry::Occupied(entry) => {
                log::error!("Key collision: {}", entry.key());
            }
        }
    }
    measurements
}

async fn upload(config: &Config, client: Client, storage: &Storage, ready: &Notify) -> ! {
    let mut retry_delay = config.retry_interval;
    let max_retry_delay = Duration::from_secs(60).max(config.retry_interval);
    loop {
        let batch = match storage.load(config.batch_size).await {
            Ok(batch) => batch,
            Err(err) => {
                log::error!(
                    "Cannot load persistent buffer: {err}; retrying in {:.3}s",
                    retry_delay.as_secs_f64()
                );
                sleep(retry_delay).await;
                retry_delay = retry_delay.saturating_mul(2).min(max_retry_delay);
                continue;
            }
        };
        if batch.len == 0 {
            // Notify retains a permit if sampling commits between load() and this wait.
            ready.notified().await;
            continue;
        }
        let request = ProvideRequest {
            measurements: batch.measurements.clone(),
        };
        let reading_count = batch.len;
        let started = Instant::now();
        log::info!(
            "Upload started: {reading_count} buffered readings from {} channels to '{}'",
            request.measurements.len(),
            config.server
        );
        let result = client
            .post(format!("{}/provide", config.server.trim_end_matches('/')))
            .json(&request)
            .send()
            .await
            .and_then(|res| res.error_for_status());
        match result {
            // error_for_status() also accepts redirects; only a 2xx is an acknowledgement.
            Ok(response) if response.status().is_success() => {
                // If cleanup fails, retry it without sending this acknowledged batch again.
                while let Err(err) = storage.remove(&batch).await {
                    log::error!("Cannot remove acknowledged batch: {err}; {reading_count} readings still buffered, retrying in {:.3}s", config.retry_interval.as_secs_f64());
                    sleep(config.retry_interval).await;
                }
                log::info!(
                    "Upload completed: {reading_count} readings in {:.3}s; removed from SQLite",
                    started.elapsed().as_secs_f64()
                );
                retry_delay = config.retry_interval;
                // Drain the next bounded batch immediately, including after process startup.
                continue;
            }
            Ok(response) => {
                log::error!(
                    "Upload rejected with HTTP {}; {reading_count} readings retained for retry",
                    response.status()
                );
            }
            Err(err) => {
                log::error!(
                    "Upload failed after {:.3}s (timeout={}): {err}; {reading_count} readings retained for retry",
                    started.elapsed().as_secs_f64(),
                    err.is_timeout()
                );
            }
        }
        log::info!("Upload retry in {:.3}s", retry_delay.as_secs_f64());
        sleep(retry_delay).await;
        retry_delay = retry_delay.saturating_mul(2).min(max_retry_delay);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtherm_common::Point;
    use std::time::SystemTime;

    #[test]
    fn empty_prefix_still_persists_mapped_channels() {
        let config: Config = toml::from_str(
            r#"
            prefix = ""
            server = "http://localhost"
            period = 60
            providers = []
            [name_map]
            "28-test" = "Water"
        "#,
        )
        .unwrap();
        let raw = [(
            "28-test".to_owned(),
            vec![Point {
                value: 12.5,
                time: SystemTime::now(),
            }],
        )]
        .into();
        let mapped = map_channels(&config, raw);
        assert_eq!(
            mapped[&ChannelId::try_from("Water").unwrap()][0].value,
            12.5
        );
    }
}
