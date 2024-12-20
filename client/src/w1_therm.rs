use crate::provider::Provider;
use rtherm_common::{Measurements, Point};
use std::{collections::HashMap, io, time::SystemTime};
use tokio::fs;

const W1_DIR: &str = "/sys/bus/w1/devices/";

pub struct W1Therm;

impl Provider for W1Therm {
    type Error = io::Error;
    async fn measure(&mut self) -> (Measurements<String>, Vec<Self::Error>) {
        let mut entries = match fs::read_dir(W1_DIR).await {
            Ok(xs) => xs,
            Err(err) => return (Measurements::default(), vec![err]),
        };
        let mut sensors = HashMap::new();
        let mut errors = Vec::new();
        'sensors: loop {
            let entry = match entries.next_entry().await {
                Ok(Some(x)) => x,
                Ok(None) => break,
                Err(err) => {
                    errors.push(err);
                    continue;
                }
            };

            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("w1_bus_master") {
                continue;
            }
            let value = {
                let mut values = [0.0; 3];
                for value in &mut values {
                    let bytes = match fs::read(entry.path().join("temperature")).await {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            errors.push(err);
                            continue;
                        }
                    };
                    *value = match String::from_utf8(bytes)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
                        .and_then(|s| {
                            s.trim()
                                .parse::<i32>()
                                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
                        }) {
                        Ok(raw) => raw as f64 * 1e-3,
                        Err(err) => {
                            errors.push(err);
                            continue 'sensors;
                        }
                    };
                }
                // Median filter
                values.sort_by(f64::total_cmp);
                values[values.len() / 2]
            };
            sensors.insert(
                name,
                vec![Point {
                    value,
                    time: SystemTime::now(),
                }],
            );
        }
        (sensors, errors)
    }
}
