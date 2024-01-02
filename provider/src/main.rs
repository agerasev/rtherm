mod config;
mod w1_therm;

use crate::config::Config;
use reqwest::Client;
use rtherm_common::ProvideRequest;
use std::time::Duration;
use tokio::time::sleep;

const PERIOD: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ! {
    let config = Config::read("config.toml")
        .await
        .expect("Error reading config");

    let client = Client::new();
    println!("Provider started");

    loop {
        sleep(PERIOD).await;

        let measurements = match w1_therm::read_all().await {
            Ok(meas) => meas,
            Err(err) => {
                println!("W1 error: {}", err);
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
        {
            Ok(_) => (),
            Err(err) => {
                println!("Error sending measurements: {}", err);
                continue;
            }
        }
    }
}
