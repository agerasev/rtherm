use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};

pub type Temperature = f64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub value: Temperature,
    pub time: SystemTime,
}

pub type ProviderId = String;
pub type SensorId = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProvideRequest {
    pub source: ProviderId,
    pub measurements: HashMap<SensorId, Measurement>,
}
