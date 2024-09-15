use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub value: f64,
    #[serde(with = "unix_secs")]
    pub time: SystemTime,
}

pub type ProviderId = String;
pub type SensorId = String;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProvideRequest {
    pub source: ProviderId,
    pub measurements: HashMap<SensorId, Measurement>,
}

mod unix_secs {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::{Duration, SystemTime};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(
            time.duration_since(SystemTime::UNIX_EPOCH)
                .map(|dur| dur.as_secs())
                .unwrap_or(0),
        )
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
    }
}
