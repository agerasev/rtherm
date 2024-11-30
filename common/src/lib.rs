pub mod error;

use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry, HashMap},
    error::Error,
    fmt::{self, Display},
    hash::Hash,
    ops::Deref,
    time::SystemTime,
};

/// Single measured point
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Point {
    pub value: f64,
    #[serde(with = "unix_secs")]
    pub time: SystemTime,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChannelId(String);

impl Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}
impl From<ChannelId> for String {
    fn from(value: ChannelId) -> Self {
        value.0
    }
}
impl AsRef<str> for ChannelId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl Deref for ChannelId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for ChannelId {
    type Error = InvalidFormat;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().all(|c| {
            ('0'..='9').contains(&c)
                || ('A'..='Z').contains(&c)
                || ('a'..='z').contains(&c)
                || '_' == c
        }) {
            Ok(Self(value))
        } else {
            Err(InvalidFormat(value))
        }
    }
}
impl TryFrom<&str> for ChannelId {
    type Error = InvalidFormat;
    fn try_from(value: &str) -> Result<Self, InvalidFormat> {
        Self::try_from(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct InvalidFormat(pub String);
impl Display for InvalidFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ChannelId allowed to contain only these chars: 0-9 | A-Z | a-z | _\nGot string: {:?}",
            &self.0
        )
    }
}
impl Error for InvalidFormat {}

pub type Measurements<K = ChannelId> = HashMap<K, Vec<Point>>;

pub fn merge_groups<K: Eq + Hash>(
    groups: impl IntoIterator<Item = Measurements<K>>,
) -> Measurements<K> {
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
