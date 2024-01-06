use rtherm_common::{Measurement as Meas, Temperature as Temp};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};
use teloxide::types::ChatId;
use tokio::sync::RwLock;

// FIXME: Don't use global DB.
pub static DB: OnceLock<DbHandle> = OnceLock::new();

pub type DbHandle = Arc<RwLock<Db>>;

#[derive(Default, Debug)]
pub struct Db {
    pub sensors: HashMap<String, Sensor>,
    pub subscribers: HashSet<ChatId>,
}

impl Db {
    pub fn handle(self) -> DbHandle {
        Arc::new(RwLock::new(self))
    }
}

#[derive(Debug)]
pub struct Sensor {
    last: Meas,
    sum: Temp,
    min: Temp,
    max: Temp,
    count: u64,
    pub flags: Flags,
}

impl Sensor {
    pub fn new(meas: Meas) -> Self {
        Self {
            count: 1,
            sum: meas.value,
            min: meas.value,
            max: meas.value,
            last: meas,
            flags: Flags {
                online: true,
                low_temp: false,
            },
        }
    }

    pub fn last(&self) -> &Meas {
        &self.last
    }

    pub fn update(&mut self, meas: Meas) {
        self.count += 1;
        self.sum += meas.value;
        self.min = self.min.min(meas.value);
        self.max = self.max.max(meas.value);
        self.last = meas;
        self.flags.online = true;
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

#[derive(Clone, Default, Debug)]
pub struct Flags {
    pub online: bool,
    pub low_temp: bool,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub last: Meas,
    pub mean: Temp,
    pub min: Temp,
    pub max: Temp,
}
