use chrono::{DateTime, Local};
use rtherm_common::{Measurement as Meas, Temperature as Temp};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display},
    sync::Arc,
    time::{Duration, SystemTime},
};
use teloxide::types::ChatId;
use tokio::sync::RwLock;

pub type DbHandle = Arc<RwLock<Db>>;

#[derive(Default, Debug)]
pub struct Db {
    pub sensors: HashMap<Id, Sensor>,
    pub subscribers: HashSet<ChatId>,
}

impl Db {
    pub fn handle(self) -> DbHandle {
        Arc::new(RwLock::new(self))
    }
}

pub type Id = String;

#[derive(Default, Debug)]
pub struct Sensor {
    pub settings: Settings,
    pub values: Values,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub timeout: Duration,
    pub low_temp: Temp,
    pub safe_temp: Temp,
}

#[derive(Clone, Debug)]
pub struct Values {
    last: Option<Meas>,
    sum: Temp,
    min: Temp,
    max: Temp,
    count: u64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub last: Option<Meas>,
    pub mean: Temp,
    pub min: Temp,
    pub max: Temp,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            low_temp: 30.0,
            safe_temp: 35.0,
        }
    }
}

impl Default for Values {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: Temp::INFINITY,
            max: Temp::NEG_INFINITY,
            last: None,
        }
    }
}
impl Values {
    pub fn last(&self) -> Option<&Meas> {
        self.last.as_ref()
    }

    pub fn update(&mut self, meas: Meas) {
        self.count += 1;
        self.sum += meas.value;
        self.min = self.min.min(meas.value);
        self.max = self.max.max(meas.value);
        self.last = Some(meas);
    }

    pub fn stats(&self) -> Stats {
        Stats {
            last: self.last.clone(),
            mean: self.sum / self.count as f64,
            min: self.min,
            max: self.max,
        }
    }
}

impl Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "last seen: ")?;
        let value = match &self.last {
            Some(m) => {
                let date = DateTime::UNIX_EPOCH.with_timezone(&Local)
                    + m.time
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or(Duration::ZERO);
                writeln!(f, "{}", date.format("%d.%m.%Y %H:%M:%S"))?;
                m.value
            }
            None => {
                writeln!(f, "never")?;
                return Ok(());
            }
        };
        writeln!(f, "last: {:.1} °C", value)?;
        writeln!(f, "min: {:.1} °C", self.min)?;
        writeln!(f, "max: {:.1} °C", self.max)?;
        writeln!(f, "average: {:.1} °C", self.mean)?;
        Ok(())
    }
}
