use crate::provider::Provider;
use rtherm_common::{Measurements, Point};
use std::{collections::HashMap, convert::Infallible, f64::consts::PI, time::SystemTime};

pub struct Dummy {
    pub name: String,
    pub offset: f64,
    pub mag: f64,
    pub period: f64,
    pub start: SystemTime,
}

impl Default for Dummy {
    fn default() -> Self {
        Self {
            name: "dummy".to_string(),
            offset: 40.0,
            mag: 20.0,
            period: 600.0,
            start: SystemTime::now(),
        }
    }
}

impl Provider for Dummy {
    type Error = Infallible;
    async fn measure(&mut self) -> (Measurements, Vec<Self::Error>) {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.start).unwrap().as_secs_f64();
        let value = self.mag * (PI * elapsed / self.period).sin() + self.offset;
        (
            HashMap::from([(self.name.clone(), vec![Point { value, time: now }])]),
            Vec::default(),
        )
    }
}
