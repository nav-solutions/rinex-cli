//! TEC maps (tec command): statistics per region, map after map, and the maps themselves
use std::collections::{BTreeMap, BTreeSet, HashMap};

use gnss_qc::prelude::{Epoch, QcOutput, QcOutputItem};

use crate::{
    engine::SessionContext,
    index::{format_duration, format_epoch},
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, ChartKind, Heatmap, HeatmapFrame,
        Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

fn q(value: f64) -> i64 {
    (value * 1.0E3).round() as i64
}

#[derive(Debug, Clone)]
struct MapStatistics {
    interpolated: bool,
    nodes: usize,
    mean: f64,
    std: f64,
    min: f64,
    max: f64,
    max_location: (f64, f64),
}

/// Nodes of one map: (latitude, longitude) → TEC
type MapNodes = HashMap<(i64, i64), f64>;

#[derive(Debug, Default)]
struct Product {
    /// region → epoch → statistics
    regions: BTreeMap<String, BTreeMap<Epoch, MapStatistics>>,
    /// epoch → (interpolated, (lat, lon) → TEC), first altitude layer
    maps: BTreeMap<Epoch, (bool, MapNodes)>,
    altitude: Option<i64>,
    latitudes: BTreeSet<i64>,
    longitudes: BTreeSet<i64>,
}

#[derive(Debug, Default)]
pub struct TecAnalysis {
    origin: Option<Epoch>,
    products: BTreeMap<String, Product>,
}

impl Analysis for TecAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        match &item.payload {
            QcOutput::TecMap(s) => {
                let origin = self.origin.get_or_insert(item.epoch);
                if item.epoch < *origin {
                    *origin = item.epoch;
                }
                self.products
                    .entry(s.source.to_string())
                    .or_default()
                    .regions
                    .entry(s.region.clone())
                    .or_default()
                    .insert(
                        item.epoch,
                        MapStatistics {
                            interpolated: s.interpolated,
                            nodes: s.nodes,
                            mean: s.mean_tecu,
                            std: s.std_tecu,
                            min: s.min_tecu,
                            max: s.max_tecu,
                            max_location: (s.max_latitude_deg, s.max_longitude_deg),
                        },
                    );
            },
            QcOutput::TecPoint(node) => {
                let product = self.products.entry(node.source.to_string()).or_default();
                let p = &node.point;
                let altitude = *product.altitude.get_or_insert(q(p.altitude_km));
                if q(p.altitude_km) != altitude {
                    return;
                }
                let (lat, lon) = (q(p.latitude_deg), q(p.longitude_deg));
                product.latitudes.insert(lat);
                product.longitudes.insert(lon);
                product
                    .maps
                    .entry(item.epoch)
                    .or_insert_with(|| (node.interpolated, HashMap::new()))
                    .1
                    .insert((lat, lon), p.tecu);
            },
            _ => {},
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        let Some(origin) = self.origin else {
            let mut section = Section::new("tec", "TEC maps");
            section.push(Block::Notice(
                "No TEC map: provide IONEX files.".to_string(),
            ));
            return vec![section];
        };

        for (index, (source, product)) in self.products.iter().enumerate() {
            let mut section =
                Section::new(&format!("tec-{}", index), &format!("TEC maps: {}", source));

            let epochs = product
                .regions
                .values()
                .flat_map(|maps| maps.iter().map(|(t, s)| (*t, s.interpolated)))
                .collect::<BTreeMap<_, _>>();

            let interpolated = epochs.values().filter(|i| **i).count();
            let period = epochs
                .keys()
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| *w[1] - *w[0])
                .min();

            let nodes = product
                .regions
                .get("Global")
                .and_then(|maps| maps.values().next())
                .map(|s| s.nodes);

            section.push(Block::KeyValue(vec![
                ("Maps".to_string(), epochs.len().to_string()),
                (
                    "Interpolated maps".to_string(),
                    format!("{} (in time, from the maps of the product)", interpolated),
                ),
                ("Map interval".to_string(), format_duration(period)),
                (
                    "First map".to_string(),
                    format_epoch(epochs.keys().next().copied()),
                ),
                (
                    "Last map".to_string(),
                    format_epoch(epochs.keys().next_back().copied()),
                ),
                (
                    "Grid".to_string(),
                    format!(
                        "{} latitudes × {} longitudes{}",
                        product.latitudes.len(),
                        product.longitudes.len(),
                        nodes.map(|n| format!(" ({} nodes)", n)).unwrap_or_default()
                    ),
                ),
                (
                    "Layer altitude".to_string(),
                    product
                        .altitude
                        .map(|alt| format!("{} km", alt as f64 / 1.0E3))
                        .unwrap_or("-".to_string()),
                ),
            ]));

            section.push(Block::Text(
                "Vertical Total Electron Content (TECu: 10¹⁶ electrons/m²). Statistics are computed over the grid nodes of each region, map after map. Interpolated maps are synthesized from the surrounding maps of the product.".to_string(),
            ));

            // per region
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

            for (region, maps) in product.regions.iter() {
                let mut mean = Stats::default();
                let mut std = Stats::default();
                let mut min = f64::MAX;
                let mut max: Option<(Epoch, &MapStatistics)> = None;

                for (t, s) in maps.iter() {
                    mean.push(s.mean);
                    std.push(s.std);
                    min = min.min(s.min);
                    if max.is_none_or(|(_, m)| s.max > m.max) {
                        max = Some((*t, s));
                    }
                }

                let (max_epoch, max_stats) = max.unwrap();

                table.push(vec![
                    region.clone(),
                    maps.len().to_string(),
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

            // time series
            for (title, select) in [
                (
                    "Mean TEC per region",
                    (|s: &MapStatistics| s.mean) as fn(&MapStatistics) -> f64,
                ),
                ("Maximal TEC per region", |s: &MapStatistics| s.max),
            ] {
                let mut chart = Chart::time(title, "TECu", origin);
                for (region, maps) in product.regions.iter() {
                    let mut series = Series::new(region);
                    for (t, s) in maps.iter() {
                        series.push(elapsed_s(origin, *t), select(s));
                    }
                    chart.push(series);
                }
                section.push(Block::Chart(chart));
            }

            if let Some(global) = product.regions.get("Global") {
                let mut chart = Chart::time(
                    "Global mean TEC: maps of the product and interpolated maps",
                    "TECu",
                    origin,
                )
                .with_kind(ChartKind::Markers);

                let (mut native, mut synthesized) = (
                    Series::new("product maps"),
                    Series::new("interpolated maps"),
                );

                for (t, s) in global.iter() {
                    let series = if s.interpolated {
                        &mut synthesized
                    } else {
                        &mut native
                    };
                    series.push(elapsed_s(origin, *t), s.mean);
                }

                chart.push(native);
                chart.push(synthesized);
                section.push(Block::Chart(chart));

                let mut chart = Chart::xy(
                    "Location of the maximal TEC (global), map after map",
                    "Longitude (°)",
                    "Latitude (°)",
                );
                let mut series = Series::new("maximal TEC");
                for s in global.values() {
                    series.push(s.max_location.1, s.max_location.0);
                }
                chart.push(series);
                section.push(Block::Chart(chart));
            }

            // the maps
            if !product.maps.is_empty() {
                let latitudes = product.latitudes.iter().copied().collect::<Vec<_>>();
                let longitudes = product.longitudes.iter().copied().collect::<Vec<_>>();

                let frames = product
                    .maps
                    .iter()
                    .map(|(t, (interpolated, values))| HeatmapFrame {
                        label: format!(
                            "{}{}",
                            format_epoch(Some(*t)),
                            if *interpolated { " (interpolated)" } else { "" }
                        ),
                        z: latitudes
                            .iter()
                            .map(|lat| {
                                longitudes
                                    .iter()
                                    .map(|lon| {
                                        values.get(&(*lat, *lon)).copied().unwrap_or(f64::NAN)
                                    })
                                    .collect()
                            })
                            .collect(),
                    })
                    .collect();

                section.push(Block::Heading("Maps".to_string()));

                section.push(Block::Heatmap(Heatmap {
                    title: format!("Vertical TEC: {}", source),
                    geographic: true,
                    x_label: "Longitude (°)".to_string(),
                    y_label: "Latitude (°)".to_string(),
                    z_label: "TECu".to_string(),
                    x: longitudes.iter().map(|v| *v as f64 / 1.0E3).collect(),
                    y: latitudes.iter().map(|v| *v as f64 / 1.0E3).collect(),
                    frames,
                }));
            }

            // per map
            if let Some(global) = product.regions.get("Global") {
                section.push(Block::Heading(
                    "Global statistics, map after map".to_string(),
                ));

                let mut table = Table::new(&[
                    "Epoch",
                    "Map",
                    "Mean TEC",
                    "σ",
                    "Min",
                    "Max",
                    "Max at (lat, lon)",
                ]);

                for (t, s) in global.iter() {
                    table.push(vec![
                        format_epoch(Some(*t)),
                        if s.interpolated {
                            "interpolated".to_string()
                        } else {
                            "product".to_string()
                        },
                        fmt(s.mean, 2),
                        fmt(s.std, 2),
                        fmt(s.min, 1),
                        fmt(s.max, 1),
                        format!("{:.1}°, {:.1}°", s.max_location.0, s.max_location.1),
                    ]);
                }

                section.push(Block::Table(table));
            }

            sections.push(section);
        }

        sections
    }
}
