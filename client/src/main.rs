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
use rtherm_common::{merge_groups, ChannelId, ProvideRequest};
use std::{
    collections::{hash_map::Entry, HashMap},
    env, mem,
    ops::Deref,
    time::Duration,
};
use storage::{MemStorage, Storage, StorageGuard};
use tokio::{sync::mpsc::unbounded_channel as channel, time::sleep};

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

    let mut storage = MemStorage::default();

    let (producer, mut consumer) = channel();
    tokio::spawn({
        log::info!("Measurement task started");
        let config = config.clone();
        async move {
            let period = Duration::from_secs_f64(config.period);
            loop {
                let (meas, errors) = providers.measure().await;
                for err in errors {
                    log::error!("Provider error: {err}");
                }
                log::debug!("Measurements obtained: {meas:?}");
                producer.send(meas).expect("Consumer is closed");

                sleep(period).await;
            }
        }
    });

    let client = Client::new();
    let mut meas_buffer = Vec::new();
    loop {
        if consumer.recv_many(&mut meas_buffer, usize::MAX).await == 0 {
            panic!("Producer is closed");
        }
        let raw_meas = merge_groups(mem::take(&mut meas_buffer));
        log::debug!("Measured: {:?}", raw_meas);
        let mut meas = HashMap::new();
        if !config.prefix.is_empty() {
            for (chan_id, values) in raw_meas {
                let id = match ChannelId::try_from(format!(
                    "{}{}",
                    config.prefix,
                    match config.name_map.get(&chan_id) {
                        Some(name) => name.to_string(),
                        None => str_to_id_lossy(&chan_id),
                    }
                )) {
                    Ok(id) => id,
                    Err(err) => {
                        log::error!("Bad channel name: {}", err);
                        continue;
                    }
                };
                match meas.entry(id) {
                    Entry::Vacant(e) => e.insert(values),
                    Entry::Occupied(e) => {
                        log::error!("Key collision: {}", e.key());
                        continue;
                    }
                };
            }
        }

        let stored = match storage.store(meas.clone()).await {
            Ok(()) => true,
            Err(e) => {
                log::error!("Cannot store measurements: {e}");
                false
            }
        };
        let guard = match storage.load().await {
            Ok(guard) => {
                if stored {
                    meas.clear();
                }
                Some(guard)
            }
            Err(e) => {
                log::error!("Cannot load from storage: {e}");
                None
            }
        };

        let request = ProvideRequest {
            measurements: match &guard {
                Some(guard) => merge_groups([guard.deref().clone(), meas]),
                None => meas,
            },
        };
        match client
            .post(format!("{}/provide", config.server))
            .json(&request)
            .send()
            .await
            .and_then(|res| res.error_for_status())
        {
            Ok(_) => {
                log::debug!("Measurements sent to '{}'", config.server);
                if let Some(guard) = guard {
                    if let Err(e) = guard.remove().await {
                        log::error!("Cannot clean up storage: {e}");
                    }
                }
            }
            Err(err) => {
                log::error!("Error sending measurements: {err}");
                continue;
            }
        }
    }
}

fn str_to_id_lossy(s: &str) -> String {
    s.replace('-', "")
}
