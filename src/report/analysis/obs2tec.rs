//! TEC estimated from the observations (obs2tec command): pierce points of the network,
//! then the TEC maps, projected on the regions and points of interest.
use std::collections::BTreeMap;

use gnss_qc::prelude::{Epoch, QcOutputItem};

use crate::{
    engine::SessionContext,
    index::{format_duration, format_epoch},
    obs2tec::{Obs2TecSetup, TecCollector, TecMaps},
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, ChartKind, Heatmap, HeatmapFrame,
        Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

/// Points of the pierce point map (decimated)
const MAX_MAP_POINTS: usize = 4000;

/// Median (NaN when empty)
fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Statistics of the nodes of one region, one map
#[derive(Debug, Clone, Copy)]
struct RegionStatistics {
    nodes: usize,
    mean: f64,
    std: f64,
    min: f64,
    max: f64,
    max_location: (f64, f64),
}

fn region_statistics(
    maps: &TecMaps,
    region: &gnss_qc::prelude::QcTecRegion,
) -> BTreeMap<Epoch, RegionStatistics> {
    let mut statistics = BTreeMap::new();
    for (epoch, map) in maps.maps.iter() {
        let mut stats = Stats::default();
        let (mut min, mut max) = (f64::MAX, f64::MIN);
        let mut max_location = (0.0, 0.0);
        for ((i, j), node) in map.nodes.iter() {
            let (lat, lon) = (maps.latitude_deg(*i), maps.longitude_deg(*j));
            if !region.contains(lat, lon) {
                continue;
            }
            stats.push(node.tecu);
            min = min.min(node.tecu);
            if node.tecu > max {
                max = node.tecu;
                max_location = (lat, lon);
            }
        }
        if stats.n > 0 {
            statistics.insert(
                *epoch,
                RegionStatistics {
                    nodes: stats.n,
                    mean: stats.mean(),
                    std: stats.std(),
                    min,
                    max,
                    max_location,
                },
            );
        }
    }
    statistics
}

#[derive(Debug)]
pub struct Obs2TecAnalysis {
    setup: Obs2TecSetup,
    collector: TecCollector,
}

impl Obs2TecAnalysis {
    pub fn new(context: &SessionContext) -> Self {
        Self {
            setup: context.obs2tec.clone().unwrap_or_default(),
            collector: TecCollector::default(),
        }
    }

    /// The estimates at the pierce points
    fn estimates_section(&self, origin: Epoch) -> Section {
        let collector = &self.collector;
        let mut section = Section::new("obs2tec", "TEC from the observations");

        section.push(Block::Notice(
            "Uncalibrated TEC: the differential code biases (DCB) of the satellites and of the receivers are not corrected. Each receiver and satellite pair is biased by a few TECu to tens of TECu (2.85 TECu per ns of DCB, GPS L1/L2): negative values, and offsets between the receivers of a network, are expected. See the obs2tec documentation."
                .to_string(),
        ));

        let receivers = collector.receivers();
        let arcs = collector.arcs.len();
        let mut noise = collector
            .arcs
            .values()
            .map(|a| a.code_noise_tecu)
            .collect::<Vec<_>>();

        section.push(Block::KeyValue(vec![
            ("Receivers".to_string(), receivers.len().to_string()),
            (
                "Satellites".to_string(),
                collector.satellites().len().to_string(),
            ),
            (
                "Constellations".to_string(),
                collector
                    .constellations()
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            (
                "Estimates".to_string(),
                format!("{} (pierce points)", collector.samples.len()),
            ),
            ("Phase arcs".to_string(), arcs.to_string()),
            (
                "Code noise (median of the arcs)".to_string(),
                format!("{} TECu (slant)", fmt(median(&mut noise), 2)),
            ),
            (
                "First estimate".to_string(),
                format_epoch(collector.samples.iter().map(|s| s.epoch).min()),
            ),
            (
                "Last estimate".to_string(),
                format_epoch(collector.samples.iter().map(|s| s.epoch).max()),
            ),
        ]));

        section.push(Block::Text(
            "Slant TEC: geometry free phase, leveled onto the geometry free code over each continuous phase arc (exact leveling of the arc). Vertical TEC: slant TEC × cos(z') at the pierce point of the single layer (thin shell) model.".to_string(),
        ));

        // per receiver
        let mut table = Table::new(&[
            "Receiver",
            "Satellites",
            "Estimates",
            "Arcs",
            "Code noise (TECu)",
            "Median VTEC",
            "Min VTEC",
            "Negative VTEC",
            "VTEC − IONEX",
        ]);

        let mut charts = (
            Chart::time("Vertical TEC per receiver (median of the satellites)", "TECu", origin),
            Chart::time(
                "Vertical TEC − input IONEX maps, per receiver (median of the satellites): a constant offset is mostly the receiver DCB",
                "TECu",
                origin,
            ),
        );

        for receiver in receivers.iter() {
            let samples = collector
                .samples
                .iter()
                .filter(|s| &s.receiver == receiver)
                .collect::<Vec<_>>();

            let mut vtec = samples
                .iter()
                .map(|s| collector.vertical_tecu(s))
                .collect::<Vec<_>>();
            let negative = vtec.iter().filter(|v| **v < 0.0).count();
            let min = vtec.iter().copied().fold(f64::MAX, f64::min);

            let mut differences = samples
                .iter()
                .filter_map(|s| s.map_vertical_tecu.map(|m| collector.vertical_tecu(s) - m))
                .collect::<Vec<_>>();

            let mut noise = collector
                .arcs
                .iter()
                .filter(|(k, _)| &k.0 == receiver)
                .map(|(_, a)| a.code_noise_tecu)
                .collect::<Vec<_>>();

            table.push(vec![
                receiver.clone(),
                samples
                    .iter()
                    .map(|s| s.sv)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    .to_string(),
                samples.len().to_string(),
                noise.len().to_string(),
                fmt(median(&mut noise), 2),
                fmt(median(&mut vtec), 1),
                fmt(min, 1),
                format!(
                    "{:.1} %",
                    100.0 * negative as f64 / samples.len().max(1) as f64
                ),
                if differences.is_empty() {
                    "-".to_string()
                } else {
                    fmt(median(&mut differences), 1)
                },
            ]);

            // per epoch medians
            let mut epochs = BTreeMap::<Epoch, (Vec<f64>, Vec<f64>)>::new();
            for s in samples.iter() {
                let entry = epochs.entry(s.epoch).or_default();
                let v = collector.vertical_tecu(s);
                entry.0.push(v);
                if let Some(m) = s.map_vertical_tecu {
                    entry.1.push(v - m);
                }
            }
            let (mut series, mut residuals) = (Series::new(receiver), Series::new(receiver));
            for (t, (values, diffs)) in epochs.iter_mut() {
                series.push(elapsed_s(origin, *t), median(values));
                if !diffs.is_empty() {
                    residuals.push(elapsed_s(origin, *t), median(diffs));
                }
            }
            charts.0.push(series);
            charts.1.push(residuals);
        }

        section.push(Block::Table(table));
        section.push(Block::Chart(charts.0));
        if !charts.1.is_empty() {
            section.push(Block::Chart(charts.1));
        }

        // pierce points, colored by vertical TEC
        let step = collector.samples.len().div_ceil(MAX_MAP_POINTS).max(1);
        let mut points = Series::new("pierce points");
        let mut colors = Vec::new();
        for s in collector.samples.iter().step_by(step) {
            points.push(s.longitude_deg, s.latitude_deg);
            colors.push(collector.vertical_tecu(s));
        }
        let series = points.with_colors(colors);

        let mut chart = Chart::xy(
            "Pierce points of the network (vertical TEC, Earth-fixed)",
            "Longitude (°)",
            "Latitude (°)",
        )
        .with_kind(ChartKind::Region)
        .with_color_label("TECu");
        chart.push(series);
        section.push(Block::Chart(chart));

        section
    }

    /// The TEC maps
    fn maps_section(&self, maps: &TecMaps) -> Section {
        let mut section = Section::new("obs2tec-maps", "TEC maps (gridded estimates)");
        let origin = *maps.maps.keys().next().unwrap();

        let filled = maps.maps.values().map(|m| m.nodes.len()).sum::<usize>();

        section.push(Block::KeyValue(vec![
            ("Maps".to_string(), maps.maps.len().to_string()),
            (
                "Map interval".to_string(),
                format_duration(Some(maps.interval)),
            ),
            ("First map".to_string(), format_epoch(Some(origin))),
            (
                "Last map".to_string(),
                format_epoch(maps.maps.keys().next_back().copied()),
            ),
            (
                "Grid".to_string(),
                format!(
                    "latitudes {}° to {}° ({}°), longitudes {}° to {}° ({}°): {} nodes",
                    maps.latitude.start,
                    maps.latitude.end,
                    maps.latitude.spacing.abs(),
                    maps.longitude.start,
                    maps.longitude.end,
                    maps.longitude.spacing,
                    maps.num_nodes()
                ),
            ),
            (
                "Nodes with a value".to_string(),
                format!(
                    "{:.1} % (coverage of the network)",
                    100.0 * filled as f64 / (maps.num_nodes() * maps.maps.len()).max(1) as f64
                ),
            ),
            (
                "Layer altitude".to_string(),
                format!("{} km", maps.altitude_km),
            ),
        ]));

        section.push(Block::Text(format!(
            "Each map gathers the pierce points of ± {} around its epoch{}. Each node is the inverse distance weighted mean of the pierce points within {}° (at least {}), its RMS their weighted dispersion. The maps are uncalibrated (DCB not corrected).",
            format_duration(Some(gnss_qc::prelude::Duration::from_seconds(maps.interval.to_seconds() / 2.0))),
            if self.setup.rotation {
                ", rotated with the Sun onto the epoch of the map (Sun-fixed frame)"
            } else {
                ""
            },
            self.setup.radius_deg,
            self.setup.min_samples,
        )));

        // regions
        let regions = self
            .setup
            .regions
            .iter()
            .map(|r| (r.name.clone(), region_statistics(maps, r)))
            .filter(|(_, s)| !s.is_empty())
            .collect::<Vec<_>>();

        if !regions.is_empty() {
            let mut table = Table::new(&[
                "Region",
                "Maps",
                "Mean TEC",
                "Mean σ",
                "Min TEC",
                "Max TEC",
                "Max at (lat, lon)",
                "Max epoch",
            ]);

            for (name, statistics) in regions.iter() {
                let mut mean = Stats::default();
                let mut std = Stats::default();
                let mut min = f64::MAX;
                let mut max: Option<(Epoch, &RegionStatistics)> = None;
                for (t, s) in statistics.iter() {
                    mean.push(s.mean);
                    std.push(s.std);
                    min = min.min(s.min);
                    if max.is_none_or(|(_, m)| s.max > m.max) {
                        max = Some((*t, s));
                    }
                }
                let (max_epoch, max_stats) = max.unwrap();
                table.push(vec![
                    name.clone(),
                    statistics.len().to_string(),
                    fmt(mean.mean(), 2),
                    fmt(std.mean(), 2),
                    fmt(min, 1),
                    fmt(max_stats.max, 1),
                    format!(
                        "{:.1}°, {:.1}°",
                        max_stats.max_location.0, max_stats.max_location.1
                    ),
                    format_epoch(Some(max_epoch)),
                ]);
            }
            section.push(Block::Table(table));

            let mut chart = Chart::time("Mean TEC per region", "TECu", origin);
            for (name, statistics) in regions.iter() {
                let mut series = Series::new(name);
                for (t, s) in statistics.iter() {
                    series.push(elapsed_s(origin, *t), s.mean);
                }
                chart.push(series);
            }
            section.push(Block::Chart(chart));

            let mut chart = Chart::time("Grid nodes with a value, per region", "nodes", origin)
                .with_kind(ChartKind::Markers);
            for (name, statistics) in regions.iter() {
                let mut series = Series::new(name);
                for (t, s) in statistics.iter() {
                    series.push(elapsed_s(origin, *t), s.nodes as f64);
                }
                chart.push(series);
            }
            section.push(Block::Chart(chart));
        }

        // points of interest
        if !self.setup.points.is_empty() {
            section.push(Block::Heading("Points of interest".to_string()));

            let mut table = Table::new(&[
                "Point",
                "Latitude",
                "Longitude",
                "Maps",
                "Mean TEC",
                "Min TEC",
                "Max TEC",
                "Mean RMS",
            ]);
            let mut chart = Chart::time("Vertical TEC at the points of interest", "TECu", origin)
                .with_kind(ChartKind::Markers);

            for (name, lat, lon) in self.setup.points.iter() {
                let mut series = Series::new(name);
                let (mut tec, mut rms) = (Stats::default(), Stats::default());
                let (mut min, mut max) = (f64::MAX, f64::MIN);
                for (t, map) in maps.maps.iter() {
                    if let Some(node) = map.points.get(name) {
                        series.push(elapsed_s(origin, *t), node.tecu);
                        tec.push(node.tecu);
                        rms.push(node.rms_tecu);
                        min = min.min(node.tecu);
                        max = max.max(node.tecu);
                    }
                }
                let n = tec.n;
                table.push(vec![
                    name.clone(),
                    format!("{}°", lat),
                    format!("{}°", lon),
                    n.to_string(),
                    if n > 0 {
                        fmt(tec.mean(), 2)
                    } else {
                        "-".to_string()
                    },
                    if n > 0 { fmt(min, 1) } else { "-".to_string() },
                    if n > 0 { fmt(max, 1) } else { "-".to_string() },
                    if n > 0 {
                        fmt(rms.mean(), 2)
                    } else {
                        "-".to_string()
                    },
                ]);
                chart.push(series);
            }

            section.push(Block::Table(table));
            if !chart.is_empty() {
                section.push(Block::Chart(chart));
            }
        }

        // the maps
        let latitudes = (0..maps.latitude.num_points()).collect::<Vec<_>>();
        let longitudes = (0..maps.longitude.num_points()).collect::<Vec<_>>();

        let frames = maps
            .maps
            .iter()
            .map(|(t, map)| HeatmapFrame {
                label: format!(
                    "{} ({} pierce points, {} receivers)",
                    format_epoch(Some(*t)),
                    map.samples,
                    map.receivers
                ),
                // heatmap: increasing latitudes
                z: latitudes
                    .iter()
                    .rev()
                    .map(|i| {
                        longitudes
                            .iter()
                            .map(|j| map.nodes.get(&(*i, *j)).map(|n| n.tecu).unwrap_or(f64::NAN))
                            .collect()
                    })
                    .collect(),
            })
            .collect();

        section.push(Block::Heading("Maps".to_string()));
        section.push(Block::Heatmap(Heatmap {
            title: "Vertical TEC (uncalibrated)".to_string(),
            geographic: true,
            x_label: "Longitude (°)".to_string(),
            y_label: "Latitude (°)".to_string(),
            z_label: "TECu".to_string(),
            x: longitudes.iter().map(|j| maps.longitude_deg(*j)).collect(),
            y: latitudes
                .iter()
                .rev()
                .map(|i| maps.latitude_deg(*i))
                .collect(),
            frames,
        }));

        section
    }
}

impl Analysis for Obs2TecAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        self.collector.consume(item);
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let Some(origin) = self.collector.samples.iter().map(|s| s.epoch).min() else {
            let mut section = Section::new("obs2tec", "TEC from the observations");
            section.push(Block::Notice(
                "No TEC estimate: dual frequency code and phase observations, orbits (navigation or SP3) and the position of each receiver are required.".to_string(),
            ));
            return vec![section];
        };

        let mut sections = vec![self.estimates_section(origin)];

        if let Some(maps) = self.collector.grid(&self.setup) {
            sections.push(self.maps_section(&maps));
        }

        sections
    }
}

#[cfg(test)]
mod test {
    use super::median;

    #[test]
    fn medians() {
        assert!(median(&mut []).is_nan());
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
    }
}
