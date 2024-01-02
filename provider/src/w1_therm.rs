use rtherm_common::Measurement;
use std::{collections::HashMap, io, time::SystemTime};
use tokio::fs;

const W1_DIR: &str = "/sys/bus/w1/devices/";

pub async fn read_all() -> io::Result<HashMap<String, Measurement>> {
    let mut entries = fs::read_dir(W1_DIR).await?;
    let mut sensors = HashMap::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("w1_bus_master") {
            continue;
        }
        let bytes = match fs::read(entry.path().join("temperature")).await {
            Ok(bytes) => bytes,
            Err(err) => {
                println!("{}: read error: {}", name, err);
                continue;
            }
        };
        let value = match String::from_utf8(bytes)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .map(|raw| raw as f64 * 1e-3)
        {
            Some(value) => value,
            None => {
                println!("{}: parse error", name);
                continue;
            }
        };
        sensors.insert(
            name,
            Measurement {
                value,
                time: SystemTime::now(),
            },
        );
    }
    Ok(sensors)
}
