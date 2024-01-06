use rtherm_common::{Measurement as Meas, Temperature as Temp};
use serde::Serialize;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, RwLock},
};

#[derive(Clone, Default)]
pub struct DbHandle {
    db: Arc<RwLock<Db>>,
}

impl DbHandle {
    pub fn update(&self, id: String, meas: Meas) {
        match self.db.write().unwrap().sensors.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(Sensor::new(meas));
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().update(meas);
            }
        }
    }

    pub fn stats(&self) -> HashMap<String, Stats> {
        self.db
            .read()
            .unwrap()
            .sensors
            .iter()
            .map(|(id, sensor)| (id.clone(), sensor.stats()))
            .collect()
    }
}

#[derive(Default, Debug)]
struct Db {
    sensors: HashMap<String, Sensor>,
}

#[derive(Debug)]
pub struct Sensor {
    last: Meas,
    sum: Temp,
    min: Temp,
    max: Temp,
    count: u64,
}

impl Sensor {
    pub fn new(meas: Meas) -> Self {
        Self {
            count: 1,
            sum: meas.value,
            min: meas.value,
            max: meas.value,
            last: meas,
        }
    }

    pub fn update(&mut self, meas: Meas) {
        self.count += 1;
        self.sum += meas.value;
        self.min = self.min.min(meas.value);
        self.max = self.max.max(meas.value);
        self.last = meas;
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

#[derive(Debug, Serialize)]
pub struct Stats {
    pub last: Meas,
    pub mean: Temp,
    pub min: Temp,
    pub max: Temp,
}
