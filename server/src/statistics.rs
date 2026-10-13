use chrono::{DateTime, Local};
use rtherm_common::{ChannelId, Measurements, Point};
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
    pub fn update(&mut self, meas: Measurements) {
        for (chan, points) in meas {
            self.channels.entry(chan).or_default().update(points);
        }
    }
}

#[derive(Clone, Default, Debug)]
pub struct ChannelHistory {
    /// Deque of measured points sorted by time
    window: VecDeque<Point>,
}

impl ChannelHistory {
    const MAX_LEN: usize = 20000;
    const MAX_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

    pub fn recent(&self, hours: u8, now: SystemTime) -> Self {
        let since = now
            .checked_sub(Duration::from_secs(u64::from(hours) * 3600))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        Self {
            window: self
                .window
                .iter()
                .copied()
                .filter(|p| p.time >= since && p.time <= now)
                .collect(),
        }
    }

    /// Preserve bucket extrema so short temperature excursions remain visible.
    pub fn chart_points(&self) -> Vec<Point> {
        let points: Vec<_> = self.window.iter().copied().collect();
        if points.len() <= 720 {
            return points;
        }
        let mut result = vec![points[0]];
        for bucket in points.chunks(points.len().div_ceil(359)) {
            let min = bucket
                .iter()
                .min_by(|a, b| a.value.total_cmp(&b.value))
                .unwrap();
            let max = bucket
                .iter()
                .max_by(|a, b| a.value.total_cmp(&b.value))
                .unwrap();
            if min.time <= max.time {
                result.extend([*min, *max]);
            } else {
                result.extend([*max, *min]);
            }
        }
        result.push(*points.last().unwrap());
        result.dedup_by(|a, b| a.time == b.time && a.value == b.value);
        result
    }

    pub fn gaps(&self) -> Vec<u64> {
        self.window
            .iter()
            .zip(self.window.iter().skip(1))
            .filter(|(a, b)| {
                b.time.duration_since(a.time).unwrap_or_default() > Duration::from_secs(240)
            })
            .map(|(_, b)| {
                b.time
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            })
            .collect()
    }

    pub fn update(&mut self, points: impl IntoIterator<Item = Point>) {
        let mut points: Vec<_> = points.into_iter().collect();
        if let Some(last) = self.window.back() {
            points.retain(|p| last.time < p.time);
        }
        points.sort_by_key(|p| p.time);

        self.window.extend(points);

        if let Some(last) = self.window.back().copied() {
            let drop_index = if let Some(drop_time) = last.time.checked_sub(Self::MAX_DURATION) {
                self.window.partition_point(|p| p.time < drop_time)
            } else {
                0
            };
            let drop_index = drop_index.max(self.window.len().saturating_sub(Self::MAX_LEN));
            self.window.drain(0..drop_index);
        }
    }

    pub fn statistics(&self) -> ChannelStatistics {
        let (sum, min, max) = self.window.iter().copied().fold(
            (0.0, f64::INFINITY, f64::NEG_INFINITY),
            |(sum, min, max), Point { value, .. }| (sum + value, min.min(value), max.max(value)),
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
    pub last: Option<Point>,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chart_is_bounded_ordered_and_preserves_spikes_and_endpoints() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100000);
        let mut history = ChannelHistory::default();
        history.update((0..20000).map(|i| Point {
            value: if i == 12345 { 100. } else { 10. },
            time: now - Duration::from_secs(20000 - i),
        }));
        let chart = history.chart_points();
        assert!(chart.len() <= 720);
        assert_eq!(
            chart.first().unwrap().time,
            now - Duration::from_secs(20000)
        );
        assert_eq!(chart.last().unwrap().time, now - Duration::from_secs(1));
        assert!(chart.iter().any(|p| p.value == 100.));
        assert!(chart.windows(2).all(|w| w[0].time <= w[1].time));
        assert_eq!(history.recent(1, now).window.len(), 3600);
        assert!(history
            .recent(1, now + Duration::from_secs(86400))
            .window
            .is_empty());
    }
}
