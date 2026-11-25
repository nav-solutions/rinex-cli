//! Meteo sensors (meteo command): statistics and time series of every sensor
use std::collections::BTreeMap;

use gnss_qc::prelude::{Epoch, QcOutput, QcOutputItem};

use crate::{
    engine::SessionContext,
    index::format_epoch,
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

/// Description and unit of a meteo observable (RINEX code)
pub fn describe(code: &str) -> (&'static str, &'static str) {
    match code {
        "PR" => ("Pressure", "hPa"),
        "TD" => ("Dry temperature", "°C"),
        "HR" => ("Relative humidity", "%"),
        "ZW" => ("Wet zenith path delay", "mm"),
        "ZD" => ("Dry zenith path delay", "mm"),
        "ZT" => ("Total zenith path delay", "mm"),
        "WD" => ("Wind direction", "°"),
        "WS" => ("Wind speed", "m/s"),
        "RI" => ("Rain increment", "0.1 mm"),
        "HI" => ("Hail indicator", "-"),
        _ => ("Unknown observable", "-"),
    }
}

#[derive(Debug, Default)]
struct Sensor {
    stats: Stats,
    first: Option<(Epoch, f64)>,
    last: Option<(Epoch, f64)>,
    min_epoch: Option<Epoch>,
    max_epoch: Option<Epoch>,
    /// largest variation between two consecutive samples, per hour
    max_rate_h: Stats,
    series: Vec<(Epoch, f64)>,
}

impl Sensor {
    fn push(&mut self, t: Epoch, value: f64) {
        if self.stats.n == 0 || value < self.stats.min {
            self.min_epoch = Some(t);
        }
        if self.stats.n == 0 || value > self.stats.max {
            self.max_epoch = Some(t);
        }

        if let Some((t0, v0)) = self.last {
            let dt_h = (t - t0).to_seconds() / 3600.0;
            if dt_h > 0.0 {
                self.max_rate_h.push((value - v0) / dt_h);
            }
        }

        self.stats.push(value);
        self.first.get_or_insert((t, value));
        self.last = Some((t, value));
        self.series.push((t, value));
    }
}

#[derive(Debug, Default)]
pub struct MeteoAnalysis {
    origin: Option<Epoch>,
    /// observable → source → sensor
    sensors: BTreeMap<String, BTreeMap<String, Sensor>>,
}

impl Analysis for MeteoAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        if let QcOutput::Meteo(sample) = &item.payload {
            let origin = self.origin.get_or_insert(item.epoch);
            if item.epoch < *origin {
                *origin = item.epoch;
            }
            self.sensors
                .entry(sample.observable.to_string())
                .or_default()
                .entry(sample.source.to_string())
                .or_default()
                .push(item.epoch, sample.value);
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut section = Section::new("meteo", "Meteo sensors");

        let Some(origin) = self.origin else {
            section.push(Block::Notice(
                "No meteo observation: provide Meteo RINEX files.".to_string(),
            ));
            return vec![section];
        };

        let sources = self
            .sensors
            .values()
            .flat_map(|sources| sources.keys().cloned())
            .collect::<std::collections::BTreeSet<_>>();

        section.push(Block::KeyValue(vec![
            (
                "Stations".to_string(),
                sources.into_iter().collect::<Vec<_>>().join(", "),
            ),
            (
                "Sensors".to_string(),
                self.sensors
                    .keys()
                    .map(|code| format!("{} ({})", code, describe(code).0))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ]));

        let mut table = Table::new(&[
            "Sensor",
            "Station",
            "Unit",
            "Samples",
            "Mean",
            "σ",
            "Min",
            "Max",
            "Last − first",
            "Max rate (/h)",
        ]);

        for (code, sources) in self.sensors.iter() {
            let (name, unit) = describe(code);
            for (source, sensor) in sources.iter() {
                let s = &sensor.stats;
                let delta = match (sensor.first, sensor.last) {
                    (Some((_, a)), Some((_, b))) => b - a,
                    _ => f64::NAN,
                };
                table.push(vec![
                    format!("{} ({})", name, code),
                    source.clone(),
                    unit.to_string(),
                    s.n.to_string(),
                    fmt(s.mean(), 2),
                    fmt(s.std(), 2),
                    format!("{} ({})", fmt(s.min, 2), format_epoch(sensor.min_epoch)),
                    format!("{} ({})", fmt(s.max, 2), format_epoch(sensor.max_epoch)),
                    fmt(delta, 2),
                    fmt(sensor.max_rate_h.max_abs(), 2),
                ]);
            }
        }
        section.push(Block::Table(table));

        for (code, sources) in self.sensors.iter() {
            let (name, unit) = describe(code);
            let mut chart = Chart::time(&format!("{} ({})", name, code), unit, origin);

            for (source, sensor) in sources.iter() {
                let mut series = Series::new(source);
                for (t, value) in sensor.series.iter() {
                    series.push(elapsed_s(origin, *t), *value);
                }
                chart.push(series);
            }

            section.push(Block::Chart(chart));
        }

        // hourly statistics, per sensor
        section.push(Block::Heading("Hourly statistics".to_string()));
        section.push(Block::Text(
            "Mean, minimum and maximum of each sensor, hour by hour.".to_string(),
        ));

        for (code, sources) in self.sensors.iter() {
            let (name, unit) = describe(code);

            for (source, sensor) in sources.iter() {
                let mut hours = BTreeMap::<i64, Stats>::new();
                for (t, value) in sensor.series.iter() {
                    let hour = (elapsed_s(origin, *t) / 3600.0).floor() as i64;
                    hours.entry(hour).or_default().push(*value);
                }

                let mut chart = Chart::time(
                    &format!("{} ({}), hourly mean, min and max: {}", name, code, source),
                    unit,
                    origin,
                );

                for (label, select) in [
                    ("mean", (|s: &Stats| s.mean()) as fn(&Stats) -> f64),
                    ("min", |s: &Stats| s.min),
                    ("max", |s: &Stats| s.max),
                ] {
                    let mut series = Series::new(label);
                    for (hour, stats) in hours.iter() {
                        series.push(*hour as f64 * 3600.0 + 1800.0, select(stats));
                    }
                    chart.push(series);
                }

                section.push(Block::Chart(chart));
            }
        }

        vec![section]
    }
}
