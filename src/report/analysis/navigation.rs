//! P.V.T solutions (nav command, cggtts --solutions)
use std::collections::BTreeMap;

use gnss_qc::{
    geodesy::{ecef_to_enu_vector, ecef_to_geodetic},
    prelude::{Epoch, QcNavigationReport, QcOutput, QcOutputItem},
};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, elapsed_s, sky::SkyAccumulator, stats::fmt, Block, Chart, ChartKind,
        Section, Series, Stats, Table,
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
    /// Moving rover: displacements rather than errors
    moving: bool,
    /// RTK base: identity and position
    base: Option<(String, Option<Ecef>)>,
    solutions: BTreeMap<String, Vec<Solution>>,
    reports: BTreeMap<String, QcNavigationReport>,
    /// Satellites used by the solver, per receiver
    skies: BTreeMap<String, SkyAccumulator>,
}

/// Map of the solutions: the track is colored by the elapsed time (minutes),
/// the RTK base (if any) is marked
fn trajectory_map(
    title: &str,
    receiver: &str,
    origin: Epoch,
    solutions: &[Solution],
    base: Option<(&str, Ecef)>,
    baseline_end: Option<Ecef>,
) -> Chart {
    let mut chart = Chart::xy(title, "Longitude (°)", "Latitude (°)")
        .with_kind(ChartKind::Map)
        .with_color_label("minutes");

    if let (Some((_, base_ecef_m)), Some(end)) = (base, baseline_end) {
        let (base_lat, base_long, _) = degrees(base_ecef_m);
        let (lat, long, _) = degrees(end);
        let mut baseline = Series::new("baseline").connected();
        baseline.push(base_long, base_lat);
        baseline.push(long, lat);
        chart.push(baseline);
    }

    let mut track = Series::new(receiver);
    let mut minutes = Vec::with_capacity(solutions.len());
    for s in solutions.iter() {
        track.push(s.lat_long_alt.1, s.lat_long_alt.0);
        minutes.push(elapsed_s(origin, s.epoch) / 60.0);
    }
    chart.push(track.with_colors(minutes));

    if let Some((name, base_ecef_m)) = base {
        let (lat, long, _) = degrees(base_ecef_m);
        let mut point = Series::new(&format!("base: {}", name)).anchor();
        point.push(long, lat);
        chart.push(point);
    }

    chart
}

/// Distance travelled, speed, altitude range of a moving rover
fn trajectory_summary(solutions: &[Solution]) -> Vec<(String, String)> {
    let mut distance = 0.0;
    let mut speeds = Stats::default();
    let mut climb = 0.0;
    for w in solutions.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let (e, n, u) = ecef_to_enu_vector(a.pos_m, sub3(b.pos_m, a.pos_m));
        let horizontal = (e * e + n * n).sqrt();
        distance += horizontal;
        if u > 0.0 {
            climb += u;
        }
        let dt = (b.epoch - a.epoch).to_seconds();
        if dt > 0.0 {
            speeds.push(horizontal / dt);
        }
    }
    let altitudes = solutions.iter().map(|s| s.lat_long_alt.2);
    let (alt_min, alt_max) = altitudes.fold((f64::MAX, f64::MIN), |(a, b), h| (a.min(h), b.max(h)));
    vec![
        (
            "Distance travelled (horizontal)".to_string(),
            format!("{:.0} m", distance),
        ),
        (
            "Speed".to_string(),
            format!(
                "mean {:.2} m/s ({:.1} km/h), max {:.2} m/s",
                speeds.mean(),
                speeds.mean() * 3.6,
                speeds.max
            ),
        ),
        (
            "Altitude".to_string(),
            format!(
                "{:.1} → {:.1} m (cumulated climb {:.0} m)",
                alt_min, alt_max, climb
            ),
        ),
    ]
}

/// World map, centered on the solutions (and the base)
fn locator(receiver: &str, solutions: &[Solution], base: Option<(&str, Ecef)>) -> Chart {
    let mut chart =
        Chart::xy("Location", "Longitude (°)", "Latitude (°)").with_kind(ChartKind::Globe);
    if let Some(last) = solutions.last() {
        let mut point = Series::new(receiver);
        point.push(last.lat_long_alt.1, last.lat_long_alt.0);
        chart.push(point);
    }
    if let Some((name, base_ecef_m)) = base {
        let (lat, long, _) = degrees(base_ecef_m);
        let mut point = Series::new(&format!("base: {}", name));
        point.push(long, lat);
        chart.push(point);
    }
    chart
}

impl NavigationAnalysis {
    pub fn new(context: &SessionContext) -> Self {
        Self {
            reference_ecef_m: context
                .navigation
                .as_ref()
                .and_then(|nav| nav.reference_ecef_m),
            rover: context.navigation.as_ref().map(|nav| nav.rover.clone()),
            moving: context.navigation.as_ref().is_some_and(|nav| nav.moving),
            base: context.navigation.as_ref().and_then(|nav| {
                nav.base
                    .as_ref()
                    .map(|base| (base.clone(), nav.base_ecef_m))
            }),
            solutions: BTreeMap::new(),
            reports: BTreeMap::new(),
            skies: BTreeMap::new(),
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
#[allow(clippy::too_many_arguments)]
fn baseline_section(
    section: &mut Section,
    origin: Epoch,
    base: &str,
    base_ecef_m: (f64, f64, f64),
    reference_ecef_m: Option<(f64, f64, f64)>,
    rover: &str,
    solutions: &[Solution],
    moving: bool,
) {
    // static rover: mean of the last 50 % (converged); moving rover: last solution
    let half = solutions.len() / 2;
    let (estimated, estimated_label) = if moving {
        (
            solutions.last().map(|s| s.pos_m).unwrap_or_default(),
            "Baseline (last solution)",
        )
    } else {
        (
            mean_position(&solutions[half..]),
            "Estimated baseline (last 50 %)",
        )
    };

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
            estimated_label.to_string(),
            format!(
                "{:.3} m — east {:.3} m, north {:.3} m, up {:.3} m",
                estimated_length, estimated_enu.0, estimated_enu.1, estimated_enu.2
            ),
        ),
    ];

    if moving {
        let lengths = solutions
            .iter()
            .map(|s| norm3(sub3(s.pos_m, base_ecef_m)))
            .collect::<Vec<_>>();
        let (min, max) = lengths
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), l| (a.min(*l), b.max(*l)));
        items.push((
            "Baseline length".to_string(),
            format!("{:.1} → {:.1} m over the session", min, max),
        ));
    }

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

    // map: base, rover track (time), baseline; world map
    section.push(Block::Chart(trajectory_map(
        "RTK: rover trajectory and base (color: minutes elapsed)",
        rover,
        origin,
        solutions,
        Some((base, base_ecef_m)),
        Some(estimated),
    )));
    section.push(Block::Chart(locator(
        rover,
        solutions,
        Some((base, base_ecef_m)),
    )));

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
    if moving && reference_ecef_m.is_none() {
        let mut chart = Chart::time("Baseline length", "m", origin);
        let mut series = Series::new("length");
        for s in solutions.iter() {
            series.push(
                elapsed_s(origin, s.epoch),
                norm3(sub3(s.pos_m, base_ecef_m)),
            );
        }
        chart.push(series);
        section.push(Block::Chart(chart));
    }

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

                let sky = self.skies.entry(solution.receiver.to_string()).or_default();
                for contribution in pvt.sv.iter() {
                    sky.seen(contribution.sv, pvt.epoch);
                    sky.position(
                        contribution.sv,
                        pvt.epoch,
                        contribution.azimuth_deg,
                        contribution.elevation_deg,
                    );
                }

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
            let moving = is_rover && self.moving;
            let (reference, reference_label) = match (is_rover, self.reference_ecef_m) {
                (true, Some(reference)) => (reference, "reference position"),
                _ if moving => (first.pos_m, "first solution"),
                _ => (mean_position(solutions), "mean position"),
            };
            // moving rover: displacements, not errors
            let errors = if moving { "Displacements" } else { "Errors" };

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

            if moving {
                section.push(Block::Heading("Trajectory".to_string()));
                section.push(Block::KeyValue(trajectory_summary(solutions)));
            }

            section.push(Block::Heading(format!(
                "{} (relative to the {})",
                errors, reference_label
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

            let mut chart = Chart::time(
                if moving {
                    "Displacements"
                } else {
                    "Position errors"
                },
                "m",
                origin,
            );
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

            let mut chart = Chart::xy(
                if moving {
                    "Horizontal displacements"
                } else {
                    "Horizontal errors"
                },
                "East (m)",
                "North (m)",
            )
            .with_equal_axes();
            let mut horizontal = Series::new("solutions");
            for (e, n, _) in enu.iter() {
                horizontal.push(*e, *n);
            }
            chart.push(horizontal);
            section.push(Block::Chart(chart));

            // RTK: the trajectory map is part of the baseline section
            let rtk_base = match (is_rover, &self.base) {
                (true, Some((base, Some(base_ecef_m)))) => Some((base.as_str(), *base_ecef_m)),
                _ => None,
            };

            if rtk_base.is_none() {
                section.push(Block::Chart(trajectory_map(
                    "Trajectory (color: minutes elapsed)",
                    receiver,
                    origin,
                    solutions,
                    None,
                    None,
                )));
                section.push(Block::Chart(locator(receiver, solutions, None)));
            }

            if let (true, Some((base, Some(base_ecef_m)))) = (is_rover, &self.base) {
                baseline_section(
                    &mut section,
                    origin,
                    base,
                    *base_ecef_m,
                    self.reference_ecef_m,
                    receiver,
                    solutions,
                    moving,
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

            if let Some(sky) = self.skies.get(receiver) {
                section.push(Block::Heading("Satellites".to_string()));
                sky.push_blocks(
                    &mut section,
                    "Sky plot: satellites used by the solver",
                    "Satellites used by the solver, over time",
                );
            }

            sections.push(section);
        }

        sections
    }
}
