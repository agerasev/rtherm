mod config;
#[cfg(feature = "dummy")]
mod dummy;
mod provider;
#[cfg(feature = "w1_therm")]
mod w1_therm;

use crate::config::Config;
use provider::{AnyProvider, Provider};
use reqwest::Client;
use rtherm_common::ProvideRequest;
use std::{env, time::Duration};
use tokio::time::sleep;

const PERIOD: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ! {
    let config = {
        let path = env::args().nth(1).expect("Path to config must be provided");
        Config::read(path).await.expect("Error reading config")
    };

    let mut providers = Vec::<AnyProvider>::new();
    #[cfg(feature = "w1_therm")]
    {
        providers.push(AnyProvider::new(w1_therm::W1Therm));
        println!("W1Therm provider created");
    }
    #[cfg(feature = "dummy")]
    {
        providers.push(AnyProvider::new(dummy::Dummy::default()));
        println!("Dummy provider created");
    }

    let client = Client::new();
    println!("Client started");

    loop {
        sleep(PERIOD).await;

        let measurements = match providers.read_all().await {
            Ok(meas) => meas,
            Err(err) => {
                println!("Provider error: {}", err);
                continue;
            }
        };

        match client
            .post(format!("{}/provide", config.server))
            .json(&ProvideRequest {
                source: config.name.clone(),
                measurements,
            })
            .send()
            .await
            .and_then(|res| res.error_for_status())
        {
            Ok(_) => {
                println!("Measurements successfully sent to '{}'", config.server)
            }
            Err(err) => {
                println!("Error sending measurements: {}", err);
                continue;
            }
        }
    }
}
