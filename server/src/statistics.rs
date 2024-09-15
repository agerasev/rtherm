use crate::recepient::ChannelId;
use chrono::{DateTime, Local};
use rtherm_common::Measurement;
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    fmt::{self, Display},
    time::{Duration, SystemTime},
};

#[derive(Default, Debug)]
pub struct Statistics {
    pub channels: HashMap<ChannelId, ChannelHistory>,
}

impl Statistics {
    pub fn update(&mut self, chan: ChannelId, meas: Measurement) {
        self.channels.entry(chan).or_default().update(meas);
    }
}

#[derive(Clone, Default, Debug)]
pub struct ChannelHistory {
    window: VecDeque<Measurement>,
}

impl ChannelHistory {
    const MAX_LEN: usize = 20000;
    const MAX_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

    pub fn last(&self) -> Option<Measurement> {
        self.window.back().copied()
    }

    pub fn update(&mut self, meas: Measurement) {
        if let Some(last) = self.window.back() {
            if last.time >= meas.time {
                return;
            }
        }
        self.window.push_back(meas);
        loop {
            if let Some(first) = self.window.front() {
                if self.window.len() > Self::MAX_LEN
                    || meas
                        .time
                        .checked_sub(Self::MAX_DURATION)
                        .unwrap_or(SystemTime::UNIX_EPOCH)
                        > first.time
                {
                    self.window.pop_front();
                    continue;
                }
            }
            break;
        }
    }

    pub fn statistics(&self) -> ChannelStatistics {
        let (sum, min, max) = self.window.iter().copied().fold(
            (0.0, f64::INFINITY, f64::NEG_INFINITY),
            |(sum, min, max), Measurement { value, .. }| {
                (sum + value, min.min(value), max.max(value))
            },
        );
        ChannelStatistics {
            last: self.window.back().copied(),
            mean: sum / self.window.len() as f64,
            min,
            max,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChannelStatistics {
    pub last: Option<Measurement>,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

impl Display for ChannelStatistics {
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
