//! P.V.T solutions (nav command, cggtts --solutions)
use std::collections::BTreeMap;

use gnss_qc::{
    geodesy::{ecef_to_enu_vector, ecef_to_geodetic},
    prelude::{Epoch, QcNavigationReport, QcOutput, QcOutputItem},
};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, ChartKind, Section, Series, Stats,
        Table,
    },
    sink::RunSummary,
};

#[derive(Debug, Clone, Copy)]
struct Solution {
    epoch: Epoch,
    pos_m: (f64, f64, f64),
    lat_long_alt: (f64, f64, f64),
    clock_offset_s: f64,
    gdop: f64,
    hdop: f64,
    vdop: f64,
    tdop: f64,
    satellites: usize,
}

/// ECEF coordinates (m)
type Ecef = (f64, f64, f64);

pub struct NavigationAnalysis {
    /// Reference position of the rover
    reference_ecef_m: Option<(f64, f64, f64)>,
    rover: Option<String>,
    /// RTK base: identity and position
    base: Option<(String, Option<Ecef>)>,
    solutions: BTreeMap<String, Vec<Solution>>,
    reports: BTreeMap<String, QcNavigationReport>,
}

impl NavigationAnalysis {
    pub fn new(context: &SessionContext) -> Self {
        Self {
            reference_ecef_m: context
                .navigation
                .as_ref()
                .and_then(|nav| nav.reference_ecef_m),
            rover: context.navigation.as_ref().map(|nav| nav.rover.clone()),
            base: context.navigation.as_ref().and_then(|nav| {
                nav.base
                    .as_ref()
                    .map(|base| (base.clone(), nav.base_ecef_m))
            }),
            solutions: BTreeMap::new(),
            reports: BTreeMap::new(),
        }
    }
}

/// Error statistics over a set of solutions
#[derive(Default)]
struct ErrorStats {
    east: Stats,
    north: Stats,
    up: Stats,
    horizontal: Stats,
    error_3d: Stats,
}

impl ErrorStats {
    fn push(&mut self, (e, n, u): (f64, f64, f64)) {
        self.east.push(e);
        self.north.push(n);
        self.up.push(u);
        self.horizontal.push((e * e + n * n).sqrt());
        self.error_3d.push((e * e + n * n + u * u).sqrt());
    }

    fn row(&self, label: &str) -> Vec<String> {
        vec![
            label.to_string(),
            self.east.n.to_string(),
            format!("{} ± {}", fmt(self.east.mean(), 3), fmt(self.east.std(), 3)),
            format!(
                "{} ± {}",
                fmt(self.north.mean(), 3),
                fmt(self.north.std(), 3)
            ),
            format!("{} ± {}", fmt(self.up.mean(), 3), fmt(self.up.std(), 3)),
            fmt(self.horizontal.rms(), 3),
            fmt(self.error_3d.rms(), 3),
            fmt(self.horizontal.max, 3),
            fmt(self.up.max_abs(), 3),
        ]
    }
}

fn sub3(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

fn norm3(a: (f64, f64, f64)) -> f64 {
    (a.0 * a.0 + a.1 * a.1 + a.2 * a.2).sqrt()
}

fn degrees(ecef_m: (f64, f64, f64)) -> (f64, f64, f64) {
    let (lat, long, alt) = ecef_to_geodetic(ecef_m);
    (lat.to_degrees(), long.to_degrees(), alt)
}

/// RTK baseline: base → rover vector (local frame of the base), estimated and reference
fn baseline_section(
    section: &mut Section,
    origin: Epoch,
    base: &str,
    base_ecef_m: (f64, f64, f64),
    reference_ecef_m: Option<(f64, f64, f64)>,
    rover: &str,
    solutions: &[Solution],
) {
    let half = solutions.len() / 2;
    let estimated = mean_position(&solutions[half..]);

    let estimated_enu = ecef_to_enu_vector(base_ecef_m, sub3(estimated, base_ecef_m));
    let estimated_length = norm3(sub3(estimated, base_ecef_m));

    let (base_lat, base_long, base_alt) = degrees(base_ecef_m);

    section.push(Block::Heading("RTK baseline".to_string()));
    section.push(Block::Text(
        "Differential navigation: the rover is positioned relative to the base, whose coordinates are fixed. The baseline is the base → rover vector, expressed in the local frame (east, north, up) of the base.".to_string(),
    ));

    let mut items = vec![
        ("Base".to_string(), base.to_string()),
        (
            "Base position".to_string(),
            format!(
                "ECEF {:.3} {:.3} {:.3} m — {:.6}°, {:.6}°, {:.3} m",
                base_ecef_m.0, base_ecef_m.1, base_ecef_m.2, base_lat, base_long, base_alt
            ),
        ),
        (
            "Estimated baseline (last 50 %)".to_string(),
            format!(
                "{:.3} m — east {:.3} m, north {:.3} m, up {:.3} m",
                estimated_length, estimated_enu.0, estimated_enu.1, estimated_enu.2
            ),
        ),
    ];

    if let Some(reference) = reference_ecef_m {
        let reference_enu = ecef_to_enu_vector(base_ecef_m, sub3(reference, base_ecef_m));
        let reference_length = norm3(sub3(reference, base_ecef_m));
        let error = sub3(estimated_enu, reference_enu);

        items.push((
            "Reference baseline".to_string(),
            format!(
                "{:.3} m — east {:.3} m, north {:.3} m, up {:.3} m",
                reference_length, reference_enu.0, reference_enu.1, reference_enu.2
            ),
        ));
        items.push((
            "Baseline error (last 50 %)".to_string(),
            format!(
                "length {:+.3} m — east {:+.3} m, north {:+.3} m, up {:+.3} m ({:.1} ppm)",
                estimated_length - reference_length,
                error.0,
                error.1,
                error.2,
                1.0E6 * norm3(error) / reference_length.max(1.0)
            ),
        ));
    }

    section.push(Block::KeyValue(items));

    // map: base, rover, baseline
    let mut chart =
        Chart::xy("RTK baseline", "Longitude (°)", "Latitude (°)").with_kind(ChartKind::Map);

    let rover_geo = degrees(estimated);

    let mut baseline = Series::new("baseline").connected();
    baseline.push(base_long, base_lat);
    baseline.push(rover_geo.1, rover_geo.0);
    chart.push(baseline);

    let mut base_point = Series::new(&format!("base: {}", base));
    base_point.push(base_long, base_lat);
    chart.push(base_point);

    let mut track = Series::new(&format!("rover: {}", rover));
    for s in solutions.iter() {
        track.push(s.lat_long_alt.1, s.lat_long_alt.0);
    }
    chart.push(track);
    section.push(Block::Chart(chart));

    // local frame of the base (km)
    let mut chart = Chart::xy(
        "RTK baseline, local frame of the base",
        "East (km)",
        "North (km)",
    )
    .with_equal_axes();

    let mut baseline = Series::new("baseline").connected();
    baseline.push(0.0, 0.0);
    baseline.push(estimated_enu.0 / 1.0E3, estimated_enu.1 / 1.0E3);
    chart.push(baseline);

    let mut rover_points = Series::new("rover solutions");
    for s in solutions.iter() {
        let enu = ecef_to_enu_vector(base_ecef_m, sub3(s.pos_m, base_ecef_m));
        rover_points.push(enu.0 / 1.0E3, enu.1 / 1.0E3);
    }
    chart.push(rover_points);
    section.push(Block::Chart(chart));

    // baseline length over time
    if let Some(reference) = reference_ecef_m {
        let reference_length = norm3(sub3(reference, base_ecef_m));
        let mut chart = Chart::time("Baseline length error (estimated − reference)", "m", origin);
        let mut series = Series::new("length error");
        for s in solutions.iter() {
            series.push(
                elapsed_s(origin, s.epoch),
                norm3(sub3(s.pos_m, base_ecef_m)) - reference_length,
            );
        }
        chart.push(series);
        section.push(Block::Chart(chart));
    }
}

fn mean_position(solutions: &[Solution]) -> (f64, f64, f64) {
    let n = solutions.len().max(1) as f64;
    let (x, y, z) = solutions.iter().fold((0.0, 0.0, 0.0), |acc, s| {
        (acc.0 + s.pos_m.0, acc.1 + s.pos_m.1, acc.2 + s.pos_m.2)
    });
    (x / n, y / n, z / n)
}

impl Analysis for NavigationAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        match &item.payload {
            QcOutput::PVTSolution(solution) => {
                let pvt = &solution.solution;
                self.solutions
                    .entry(solution.receiver.to_string())
                    .or_default()
                    .push(Solution {
                        epoch: pvt.epoch,
                        pos_m: pvt.pos_m,
                        lat_long_alt: pvt.lat_long_alt_deg_deg_m,
                        clock_offset_s: pvt.clock_offset_s,
                        gdop: pvt.gdop,
                        hdop: pvt.hdop,
                        vdop: pvt.vdop,
                        tdop: pvt.tdop,
                        satellites: pvt.sv.len(),
                    });
            },
            QcOutput::NavigationReport(report) => {
                self.reports
                    .insert(report.receiver.to_string(), report.clone());
            },
            _ => {},
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        let mut receivers = self
            .solutions
            .keys()
            .chain(self.reports.keys())
            .cloned()
            .collect::<Vec<_>>();
        receivers.sort();
        receivers.dedup();

        if receivers.is_empty() {
            let mut section = Section::new("navigation", "Navigation");
            section.push(Block::Notice(
                "No solution: verify the navigation products (NAV, SP3) cover the observations, and the constellations selected.".to_string(),
            ));
            sections.push(section);
            return sections;
        }

        for (index, receiver) in receivers.iter().enumerate() {
            let mut section = Section::new(
                &format!("navigation-{}", index),
                &format!("Navigation: {}", receiver),
            );

            let empty = Vec::new();
            let solutions = self.solutions.get(receiver).unwrap_or(&empty);

            if let Some(report) = self.reports.get(receiver) {
                let rate = 100.0 * report.solutions as f64 / report.epochs.max(1) as f64;
                section.push(Block::KeyValue(vec![
                    ("Epochs presented".to_string(), report.epochs.to_string()),
                    (
                        "Solutions".to_string(),
                        format!("{} ({:.1} %)", report.solutions, rate),
                    ),
                    (
                        "Orbital states".to_string(),
                        format!(
                            "{} precise, {} broadcast",
                            report.precise_orbits, report.broadcast_orbits
                        ),
                    ),
                    (
                        "Satellite clock states".to_string(),
                        format!(
                            "{} clock products, {} precise products, {} broadcast",
                            report.precise_clocks,
                            report.precise_state_clocks,
                            report.broadcast_clocks
                        ),
                    ),
                ]));

                if !report.failures.is_empty() {
                    let mut table = Table::new(&["Failure", "Epochs"]);
                    for (reason, count) in report.failures.iter() {
                        table.push(vec![reason.clone(), count.to_string()]);
                    }
                    section.push(Block::Table(table));
                }
            }

            let Some(first) = solutions.first() else {
                section.push(Block::Notice("No solution resolved.".to_string()));
                sections.push(section);
                continue;
            };

            let origin = first.epoch;

            // reference: rover reference position, otherwise the mean solution
            let is_rover = self.rover.as_deref() == Some(receiver.as_str());
            let (reference, reference_label) = match (is_rover, self.reference_ecef_m) {
                (true, Some(reference)) => (reference, "reference position"),
                _ => (mean_position(solutions), "mean position"),
            };

            let enu = solutions
                .iter()
                .map(|s| {
                    let delta = (
                        s.pos_m.0 - reference.0,
                        s.pos_m.1 - reference.1,
                        s.pos_m.2 - reference.2,
                    );
                    ecef_to_enu_vector(reference, delta)
                })
                .collect::<Vec<_>>();

            // position
            let half = solutions.len() / 2;
            let converged = mean_position(&solutions[half..]);
            let distance = ((converged.0 - reference.0).powi(2)
                + (converged.1 - reference.1).powi(2)
                + (converged.2 - reference.2).powi(2))
            .sqrt();

            let (lat, long, alt) = solutions.last().map(|s| s.lat_long_alt).unwrap_or_default();

            section.push(Block::Heading("Position".to_string()));
            section.push(Block::KeyValue(vec![
                (
                    "Reference".to_string(),
                    format!(
                        "{} — ECEF {:.3} {:.3} {:.3} m",
                        reference_label, reference.0, reference.1, reference.2
                    ),
                ),
                (
                    "Mean solution (last 50 %)".to_string(),
                    format!(
                        "ECEF {:.3} {:.3} {:.3} m — {:.3} m from the {}",
                        converged.0, converged.1, converged.2, distance, reference_label
                    ),
                ),
                (
                    "Last solution".to_string(),
                    format!("{:.8}°, {:.8}°, {:.3} m", lat, long, alt),
                ),
            ]));

            // errors
            let mut all = ErrorStats::default();
            let mut last_half = ErrorStats::default();
            for (i, error) in enu.iter().enumerate() {
                all.push(*error);
                if i >= half {
                    last_half.push(*error);
                }
            }

            section.push(Block::Heading(format!(
                "Errors (relative to the {})",
                reference_label
            )));
            let mut table = Table::new(&[
                "Period",
                "Solutions",
                "East (m)",
                "North (m)",
                "Up (m)",
                "RMS 2D (m)",
                "RMS 3D (m)",
                "Max horizontal (m)",
                "Max vertical (m)",
            ]);
            table.push(all.row("Session"));
            table.push(last_half.row("Last 50 %"));
            section.push(Block::Table(table));

            let mut chart = Chart::time("Position errors", "m", origin);
            let mut series = [Series::new("East"), Series::new("North"), Series::new("Up")];
            for (s, (e, n, u)) in solutions.iter().zip(enu.iter()) {
                let x = elapsed_s(origin, s.epoch);
                series[0].push(x, *e);
                series[1].push(x, *n);
                series[2].push(x, *u);
            }
            for s in series {
                chart.push(s);
            }
            section.push(Block::Chart(chart));

            let mut chart =
                Chart::xy("Horizontal errors", "East (m)", "North (m)").with_equal_axes();
            let mut horizontal = Series::new("solutions");
            for (e, n, _) in enu.iter() {
                horizontal.push(*e, *n);
            }
            chart.push(horizontal);
            section.push(Block::Chart(chart));

            let mut chart =
                Chart::xy("Trajectory", "Longitude (°)", "Latitude (°)").with_kind(ChartKind::Map);
            let mut track = Series::new(receiver);
            for s in solutions.iter() {
                track.push(s.lat_long_alt.1, s.lat_long_alt.0);
            }
            chart.push(track);
            section.push(Block::Chart(chart));

            if let (true, Some((base, Some(base_ecef_m)))) = (is_rover, &self.base) {
                baseline_section(
                    &mut section,
                    origin,
                    base,
                    *base_ecef_m,
                    self.reference_ecef_m,
                    receiver,
                    solutions,
                );
            }

            // differential navigation: no receiver clock
            let rtk = is_rover && self.base.is_some();

            if !rtk {
                let mut chart = Chart::time("Receiver clock offset", "µs", origin);
                let mut clock = Series::new("clock offset");
                for s in solutions.iter() {
                    clock.push(elapsed_s(origin, s.epoch), s.clock_offset_s * 1.0E6);
                }
                chart.push(clock);
                section.push(Block::Chart(chart));
            }

            let mut chart = Chart::time("Dilution of precision", "DOP", origin);
            let mut dops = [
                Series::new("GDOP"),
                Series::new("HDOP"),
                Series::new("VDOP"),
                Series::new("TDOP"),
            ];
            let mut svs = Series::new("satellites");
            for s in solutions.iter() {
                let x = elapsed_s(origin, s.epoch);
                dops[0].push(x, s.gdop);
                dops[1].push(x, s.hdop);
                dops[2].push(x, s.vdop);
                dops[3].push(x, s.tdop);
                svs.push(x, s.satellites as f64);
            }
            for s in dops {
                chart.push(s);
            }
            section.push(Block::Chart(chart));

            let mut chart = Chart::time("Satellites used", "satellites", origin);
            chart.push(svs);
            section.push(Block::Chart(chart));

            sections.push(section);
        }

        sections
    }
}
