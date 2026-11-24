//! Ionospheric Pierce Points (ipp command)
use std::collections::{BTreeMap, HashMap};

use gnss_qc::{
    geodesy::ecef_to_geodetic,
    prelude::{Epoch, QcOutput, QcOutputItem, SV},
};

use crate::{
    engine::SessionContext,
    input::ProductKind,
    report::{
        analysis::Analysis,
        elapsed_s,
        sky::{sv_color, SkyAccumulator},
        stats::fmt,
        Block, Chart, ChartKind, Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

/// A pierce point track is interrupted when two points are further apart (s)
const TRACK_GAP_S: f64 = 600.0;

/// Minimal interval between two points of a track, on the maps and charts (s)
const TRACK_STEP_S: f64 = 30.0;

/// Mean Earth radius (km), for the ground distances
const EARTH_RADIUS_KM: f64 = 6371.0;

#[derive(Debug, Default)]
struct SatellitePoints {
    elevation: Stats,
    obliquity: Stats,
    latitude: Stats,
    longitude: Stats,
    vtec: Stats,
    /// Pierce point track (longitude, latitude), interrupted (NaN) over the gaps
    track: Option<Series>,
    /// Vertical TEC over time (TECu)
    vtec_series: Option<Series>,
    /// Pierce points with a vertical TEC (longitude, latitude), interrupted over the gaps
    vtec_track: Option<Series>,
    /// Vertical TEC of each point of the vtec_track (TECu)
    vtec_colors: Vec<f64>,
    last: Option<Epoch>,
    /// Last point presented on the maps and charts
    last_sample: Option<Epoch>,
}

#[derive(Debug, Default)]
struct ReceiverPoints {
    origin: Option<Epoch>,
    satellites: BTreeMap<SV, SatellitePoints>,
    /// Pierce points with a vertical TEC
    vtec_points: usize,
    /// Orbital products the satellite positions were resolved from
    orbits: BTreeMap<String, usize>,
    /// Shell altitude (km)
    shell_km: Option<f64>,
    /// Lines of sight
    sky: SkyAccumulator,
    /// Largest ground distance receiver → pierce point (km)
    max_distance_km: f64,
}

#[derive(Debug, Default)]
pub struct IppAnalysis {
    receivers: BTreeMap<String, ReceiverPoints>,
    /// Receiver positions (latitude °, longitude °), RINEX header
    positions: HashMap<String, (f64, f64)>,
}

impl IppAnalysis {
    pub fn new(context: &SessionContext) -> Self {
        Self {
            positions: context
                .manifest
                .iter()
                .filter(|entry| entry.kind == ProductKind::Observation)
                .filter_map(|entry| {
                    let (lat, long, _) = ecef_to_geodetic(entry.position_ecef_m?);
                    Some((
                        entry.identity.to_string(),
                        (lat.to_degrees(), long.to_degrees()),
                    ))
                })
                .collect(),
            ..Default::default()
        }
    }
}

/// Great circle distance (km) between two (latitude °, longitude °)
fn ground_distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (p1, p2) = (a.0.to_radians(), b.0.to_radians());
    let (dp, dl) = (p2 - p1, (b.1 - a.1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

impl Analysis for IppAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        let QcOutput::IPP(ipp) = &item.payload else {
            return;
        };

        let receiver = ipp.receiver.to_string();
        let position = self.positions.get(&receiver).copied();
        let rx = self.receivers.entry(receiver).or_default();
        let origin = *rx.origin.get_or_insert(item.epoch);
        let x = elapsed_s(origin, item.epoch);

        rx.shell_km.get_or_insert(ipp.altitude_km);
        *rx.orbits.entry(format!("{:?}", ipp.orbit)).or_default() += 1;
        rx.sky.seen(ipp.sv, item.epoch);
        rx.sky
            .position(ipp.sv, item.epoch, ipp.azimuth_deg, ipp.elevation_deg);

        if let Some(position) = position {
            let distance = ground_distance_km(position, (ipp.latitude_deg, ipp.longitude_deg));
            rx.max_distance_km = rx.max_distance_km.max(distance);
        }

        let sat = rx.satellites.entry(ipp.sv).or_default();
        sat.elevation.push(ipp.elevation_deg);
        sat.obliquity.push(ipp.obliquity_factor);
        sat.latitude.push(ipp.latitude_deg);
        sat.longitude.push(ipp.longitude_deg);

        let gap = sat
            .last
            .is_some_and(|last| (item.epoch - last).to_seconds() > TRACK_GAP_S);

        sat.last = Some(item.epoch);
        let vtec = ipp.vertical_tecu.filter(|v| v.is_finite());
        if let Some(vtec) = vtec {
            sat.vtec.push(vtec);
            rx.vtec_points += 1;
        }

        // maps and charts: one point every TRACK_STEP_S at most
        let sample = gap
            || sat
                .last_sample
                .is_none_or(|last| (item.epoch - last).to_seconds() >= TRACK_STEP_S);
        if !sample {
            return;
        }
        sat.last_sample = Some(item.epoch);

        let track = sat
            .track
            .get_or_insert_with(|| Series::new(&ipp.sv.to_string()).with_color(&sv_color(ipp.sv)));
        if gap {
            track.push(f64::NAN, f64::NAN);
        }
        track.push(ipp.longitude_deg, ipp.latitude_deg);

        if let Some(vtec) = vtec {
            sat.vtec_series
                .get_or_insert_with(|| Series::new(&ipp.sv.to_string()))
                .push(x, vtec);
            let points = sat
                .vtec_track
                .get_or_insert_with(|| Series::new(&ipp.sv.to_string()));
            if gap && !points.x.is_empty() {
                points.push(f64::NAN, f64::NAN);
                sat.vtec_colors.push(f64::NAN);
            }
            points.push(ipp.longitude_deg, ipp.latitude_deg);
            sat.vtec_colors.push(vtec);
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        for (index, (receiver, rx)) in self.receivers.iter().enumerate() {
            let mut section = Section::new(
                &format!("ipp-{}", index),
                &format!("Ionospheric Pierce Points: {}", receiver),
            );

            let Some(origin) = rx.origin else {
                continue;
            };
            let position = self.positions.get(receiver).copied();
            let station = position.map(|(lat, long)| {
                let mut series = Series::new(receiver).anchor();
                series.push(long, lat);
                series
            });

            // summary
            let points = rx.satellites.values().map(|s| s.elevation.n).sum::<usize>();
            let (mut latitude, mut longitude) = (Stats::default(), Stats::default());
            for sat in rx.satellites.values() {
                latitude.push(sat.latitude.min);
                latitude.push(sat.latitude.max);
                longitude.push(sat.longitude.min);
                longitude.push(sat.longitude.max);
            }

            let mut summary = vec![
                ("Pierce points".to_string(), points.to_string()),
                ("Satellites".to_string(), rx.satellites.len().to_string()),
                (
                    "Shell altitude".to_string(),
                    rx.shell_km
                        .map(|km| format!("{} km", fmt(km, 0)))
                        .unwrap_or_default(),
                ),
                (
                    "Orbital states".to_string(),
                    rx.orbits
                        .iter()
                        .map(|(kind, n)| format!("{} {}", n, kind.to_lowercase()))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                (
                    "Extent of the pierce points".to_string(),
                    format!(
                        "latitude {}° → {}°, longitude {}° → {}°",
                        fmt(latitude.min, 1),
                        fmt(latitude.max, 1),
                        fmt(longitude.min, 1),
                        fmt(longitude.max, 1)
                    ),
                ),
            ];
            if let Some((lat, long)) = position {
                summary.push((
                    "Receiver".to_string(),
                    format!(
                        "{:.4}°, {:.4}° — pierce points up to {} km away",
                        lat,
                        long,
                        fmt(rx.max_distance_km, 0)
                    ),
                ));
            }
            if rx.vtec_points > 0 {
                summary.push((
                    "Vertical TEC (IONEX)".to_string(),
                    format!(
                        "{} pierce points ({:.1} %) covered by the TEC maps",
                        rx.vtec_points,
                        100.0 * rx.vtec_points as f64 / points.max(1) as f64
                    ),
                ));
            }
            section.push(Block::KeyValue(summary));

            // map of the tracks
            let mut chart = Chart::xy(
                "Pierce points, per satellite",
                "Longitude (°)",
                "Latitude (°)",
            )
            .with_kind(ChartKind::Region);
            for sat in rx.satellites.values() {
                if let Some(track) = &sat.track {
                    chart.push(track.clone());
                }
            }
            if let Some(station) = &station {
                chart.push(station.clone());
            }
            section.push(Block::Chart(chart));

            // map of the vertical TEC
            if rx.vtec_points > 0 {
                let mut chart = Chart::xy(
                    "Vertical TEC at the pierce points",
                    "Longitude (°)",
                    "Latitude (°)",
                )
                .with_kind(ChartKind::Region)
                .with_color_label("TECu");
                // one series per satellite: the thin lines follow the tracks
                for sat in rx.satellites.values() {
                    if let Some(track) = &sat.vtec_track {
                        chart.push(track.clone().with_colors(sat.vtec_colors.clone()));
                    }
                }
                if let Some(station) = &station {
                    chart.push(station.clone());
                }
                section.push(Block::Chart(chart));
            }

            // world map
            let mut chart =
                Chart::xy("Location", "Longitude (°)", "Latitude (°)").with_kind(ChartKind::Globe);
            for sat in rx.satellites.values() {
                if let Some(track) = &sat.track {
                    chart.push(track.clone());
                }
            }
            if let Some(station) = &station {
                chart.push(station.clone());
            }
            section.push(Block::Chart(chart));

            // lines of sight
            section.push(Block::Heading("Lines of sight".to_string()));
            rx.sky.push_blocks(
                &mut section,
                "Sky plot: lines of sight through the ionospheric shell",
                "Pierce points, over time",
            );

            // vertical TEC over time
            if rx.vtec_points > 0 {
                let mut chart = Chart::time("Vertical TEC at the pierce points", "TECu", origin)
                    .with_kind(ChartKind::Markers);
                for sat in rx.satellites.values() {
                    if let Some(series) = &sat.vtec_series {
                        chart.push(series.clone());
                    }
                }
                section.push(Block::Chart(chart));
            }

            section.push(Block::Heading("Satellites".to_string()));
            let mut table = Table::new(&[
                "SV",
                "Points",
                "Elevation (°)",
                "Mean obliquity",
                "Latitude (°)",
                "Longitude (°)",
                "Mean VTEC (TECu)",
            ]);
            for (sv, sat) in rx.satellites.iter() {
                table.push(vec![
                    sv.to_string(),
                    sat.elevation.n.to_string(),
                    format!(
                        "{} → {}",
                        fmt(sat.elevation.min, 1),
                        fmt(sat.elevation.max, 1)
                    ),
                    fmt(sat.obliquity.mean(), 3),
                    format!(
                        "{} → {}",
                        fmt(sat.latitude.min, 1),
                        fmt(sat.latitude.max, 1)
                    ),
                    format!(
                        "{} → {}",
                        fmt(sat.longitude.min, 1),
                        fmt(sat.longitude.max, 1)
                    ),
                    if sat.vtec.n > 0 {
                        fmt(sat.vtec.mean(), 1)
                    } else {
                        "-".to_string()
                    },
                ]);
            }
            section.push(Block::Table(table));
            sections.push(section);
        }

        if sections.is_empty() {
            let mut section = Section::new("ipp", "Ionospheric Pierce Points");
            section.push(Block::Notice(
                "No pierce point: receivers must describe their position, and orbital products must cover the satellites in sight.".to_string(),
            ));
            sections.push(section);
        }

        sections
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use gnss_qc::prelude::{QcIndexing, QcIppOutput, QcOrbitKind};
    use std::str::FromStr;

    /// Synthetic pass of one satellite over a receiver in Svalbard, with vertical TEC
    fn pass(analysis: &mut IppAnalysis, sv: &str, t0: Epoch, offset_deg: f64) {
        for k in 0..60 {
            let t = t0 + gnss_qc::prelude::Duration::from_seconds(30.0 * k as f64);
            let ipp = QcIppOutput {
                receiver: QcIndexing::Receiver("rover".to_string()),
                sv: SV::from_str(sv).unwrap(),
                orbit: QcOrbitKind::Broadcast,
                elevation_deg: 20.0 + k as f64,
                azimuth_deg: 90.0 + offset_deg,
                latitude_deg: 75.0 + 0.2 * k as f64,
                longitude_deg: offset_deg + 0.5 * k as f64,
                altitude_km: 450.0,
                obliquity_factor: 1.5,
                vertical_tecu: Some(5.0 + 0.1 * k as f64),
            };
            analysis.consume(&QcOutputItem {
                epoch: t,
                payload: QcOutput::IPP(ipp),
            });
        }
    }

    #[test]
    fn pierce_point_maps() {
        let context = SessionContext {
            title: "test".to_string(),
            session: "test".to_string(),
            command: "ipp".to_string(),
            command_line: String::new(),
            setup: Vec::new(),
            manifest: Vec::new(),
            #[cfg(feature = "navigation")]
            navigation: None,
            qc: false,
        };
        let mut analysis = IppAnalysis::new(&context);
        analysis.positions.insert(
            QcIndexing::Receiver("rover".to_string()).to_string(),
            (78.9, 12.1),
        );

        let t0 = Epoch::from_str("2024-09-20T08:00:00 GPST").unwrap();
        pass(&mut analysis, "G01", t0, 0.0);
        pass(&mut analysis, "E05", t0, 20.0);
        // second pass of G01, two hours later: the track is interrupted
        pass(
            &mut analysis,
            "G01",
            t0 + gnss_qc::prelude::Duration::from_hours(2.0),
            40.0,
        );

        let sections = analysis.sections(&context, &RunSummary::default());
        assert_eq!(sections.len(), 1);
        let charts = sections[0]
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Chart(chart) => Some(chart),
                _ => None,
            })
            .collect::<Vec<_>>();

        let regions = charts
            .iter()
            .filter(|chart| chart.kind == ChartKind::Region)
            .collect::<Vec<_>>();
        assert_eq!(regions.len(), 2, "tracks and vertical TEC maps");
        // tracks: one per satellite and the receiver
        assert_eq!(regions[0].series.len(), 3);
        let g01 = regions[0].series.iter().find(|s| s.name == "G01").unwrap();
        assert_eq!(g01.x.iter().filter(|x| x.is_nan()).count(), 1);
        assert!(regions[1].series.iter().any(|s| s.colors.is_some()));
        assert!(charts.iter().any(|chart| chart.kind == ChartKind::Globe));
        assert!(sections[0]
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Skyplot(_))));

        #[cfg(feature = "pdf")]
        for chart in regions {
            let svg = crate::report::svg::render(chart);
            assert!(!svg.contains("NaN"));
            if let Ok(dir) = std::env::var("RINEX_CLI_TEST_SVG") {
                std::fs::write(
                    format!("{}/{}.svg", dir, chart.title.replace(' ', "_")),
                    svg,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn distances() {
        assert!(ground_distance_km((0.0, 0.0), (0.0, 0.0)).abs() < 1e-9);
        // one degree of a great circle
        let d = ground_distance_km((78.9, 12.0), (79.9, 12.0));
        assert!((d - 111.19).abs() < 0.01);
    }
}
