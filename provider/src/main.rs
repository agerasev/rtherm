use reqwest::Client;
use rtherm_common::{Measurement, ProvideRequest};
use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, SystemTime},
};
use tokio::{fs, time::sleep};

async fn read_all_w1_therm() -> HashMap<String, Measurement> {
    let w1_dir = Path::new("/sys/bus/w1/devices/");
    let mut entries = fs::read_dir(w1_dir).await.unwrap();
    let mut sensors = HashMap::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        match fs::read(entry.path().join("temperature")).await {
            Ok(bytes) => {
                sensors.insert(
                    entry.file_name().to_str().unwrap().to_owned(),
                    Measurement {
                        value: {
                            let raw = String::from_utf8(bytes).unwrap();
                            dbg!(&raw);
                            raw.trim().parse::<i32>().unwrap() as f64 * 1e-3
                        },
                        time: SystemTime::now(),
                    },
                );
            }
            Err(err) => {
                println!("Cannot read {:?} temperature: {:?}", entry.file_name(), err);
                continue;
            }
        }
    }
    sensors
}

const PERIOD: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> ! {
    let client = Client::new();
    println!("Provider started");

    loop {
        let measurements = read_all_w1_therm().await;

        client
            .post("http://192.168.0.2:8080/provide")
            .json(&ProvideRequest {
                source: "berezki-rpi".into(),
                measurements,
            })
            .send()
            .await
            .unwrap();

        sleep(PERIOD).await;
    }
}
