pub mod error;

use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry, HashMap},
    time::SystemTime,
};

/// Single measured point
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Point {
    pub value: f64,
    #[serde(with = "unix_secs")]
    pub time: SystemTime,
}

pub type ChannelId = String;
pub type Measurements = HashMap<ChannelId, Vec<Point>>;

pub fn merge_groups(groups: impl IntoIterator<Item = Measurements>) -> Measurements {
    let mut accum = HashMap::new();
    for group in groups {
        for (channel, measurements) in group {
            match accum.entry(channel) {
                Entry::Vacant(e) => {
                    e.insert(measurements);
                }
                Entry::Occupied(mut e) => {
                    e.get_mut().extend(measurements);
                }
            }
        }
    }
    accum
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProvideRequest {
    pub measurements: Measurements,
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
