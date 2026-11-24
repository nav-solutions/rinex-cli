//! Quality control of the signal sources (qc command)
use std::collections::{BTreeMap, BTreeSet, HashMap};

use itertools::Itertools;

use gnss_qc::prelude::{Constellation, Duration, Epoch, QcOutput, QcOutputItem, SV};
use rinex::prelude::obs::{EpochFlag, LliFlags};

use gnss_qc::orbit::QcKepler;

use crate::{
    engine::SessionContext,
    input::ProductKind,
    report::{
        analysis::Analysis,
        elapsed_s,
        sky::{azimuth_elevation, SkyAccumulator},
        stats::fmt,
        Block, Chart, Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

/// Broadcast ephemerides are used up to this distance from their time of clock (s)
const MAX_EPHEMERIS_AGE_S: f64 = 4.0 * 3600.0;

/// Maximal number of data gaps listed
const MAX_GAPS: usize = 50;

#[derive(Debug, Default)]
struct SatelliteState {
    /// Epochs this satellite was observed
    epochs: usize,
    /// Epochs this satellite was covered by an ephemeris
    covered: usize,
    /// Currently covered
    covered_now: bool,
    /// Coverage losses
    coverage_losses: usize,
    last_epoch: Option<Epoch>,
    /// Lock loss indications
    lock_losses: usize,
    observables: BTreeSet<String>,
    /// Signal strength, per observable
    ssi: BTreeMap<String, Stats>,
}

#[derive(Debug, Default)]
struct ReceiverState {
    epochs: Vec<Epoch>,
    /// Abnormal epoch flags
    flags: BTreeMap<String, usize>,
    satellites: BTreeMap<SV, SatelliteState>,
    /// Satellites in view, per constellation, for the current epoch
    in_view: BTreeMap<String, usize>,
    /// Satellites in view, per constellation, over time
    in_view_series: BTreeMap<String, Series>,
    /// Signal strength observable presented, per constellation
    primary_ssi: HashMap<Constellation, String>,
    /// Signal strength over time, per satellite (primary observable)
    ssi_series: BTreeMap<SV, Series>,
    /// Ephemeris coverage analyzed
    coverage: bool,
}

impl ReceiverState {
    fn origin(&self) -> Option<Epoch> {
        self.epochs.first().copied()
    }

    /// Ends the current epoch
    fn flush_epoch(&mut self) {
        let (Some(origin), Some(last)) = (self.origin(), self.epochs.last().copied()) else {
            return;
        };

        let x = elapsed_s(origin, last);
        for (constellation, count) in std::mem::take(&mut self.in_view) {
            self.in_view_series
                .entry(constellation.clone())
                .or_insert_with(|| Series::new(&constellation))
                .push(x, count as f64);
        }
    }

    /// Dominant sampling interval
    fn sampling(&self) -> Option<Duration> {
        let mut histogram = HashMap::<i128, usize>::new();
        for (a, b) in self.epochs.iter().tuple_windows() {
            let dt = (*b - *a).total_nanoseconds();
            if dt > 0 {
                *histogram.entry(dt).or_default() += 1;
            }
        }
        histogram
            .into_iter()
            .max_by_key(|(dt, count)| (*count, -dt))
            .map(|(dt, _)| Duration::from_total_nanoseconds(dt))
    }

    /// Data gaps: (start, end), larger than 1.5 x the sampling interval
    fn gaps(&self, sampling: Duration) -> Vec<(Epoch, Epoch)> {
        self.epochs
            .iter()
            .tuple_windows()
            .filter(|(a, b)| (**b - **a).to_seconds() > 1.5 * sampling.to_seconds())
            .map(|(a, b)| (*a, *b))
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct SignalAnalysis {
    receivers: BTreeMap<String, ReceiverState>,
    /// Reference position of each signal source (RINEX header)
    positions: HashMap<String, (f64, f64, f64)>,
    /// Broadcast ephemerides, per satellite
    ephemerides: HashMap<SV, Vec<QcKepler>>,
    /// Sky of each signal source
    skies: BTreeMap<String, SkyAccumulator>,
    /// Signal sources positioned by the navigation solutions (instead of the RINEX header)
    #[cfg_attr(not(feature = "navigation"), allow(dead_code))]
    solved: BTreeSet<String>,
}

impl SignalAnalysis {
    pub fn new(context: &SessionContext) -> Self {
        Self {
            positions: context
                .manifest
                .iter()
                .filter(|entry| entry.kind == ProductKind::Observation)
                .filter_map(|entry| Some((entry.identity.to_string(), entry.position_ecef_m?)))
                .collect(),
            ..Default::default()
        }
    }

    /// Broadcast state of the satellite (ECEF m), closest ephemeris
    fn satellite_position(&self, sv: SV, t: Epoch) -> Option<(f64, f64, f64)> {
        let kepler = self
            .ephemerides
            .get(&sv)?
            .iter()
            .map(|k| (k, (t - k.toc).to_seconds().abs()))
            .filter(|(_, age)| *age <= MAX_EPHEMERIS_AGE_S)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?
            .0;
        Some(kepler.state(t).position_m)
    }
}

impl Analysis for SignalAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        match &item.payload {
            // navigation: the sky view follows the receiver
            #[cfg(feature = "navigation")]
            QcOutput::PVTSolution(solution) => {
                self.positions
                    .insert(solution.receiver.to_string(), solution.solution.pos_m);
                self.solved.insert(solution.receiver.to_string());
            },
            QcOutput::Signal(signal) => {
                let epoch = item.epoch;
                let rx = self
                    .receivers
                    .entry(signal.receiver.to_string())
                    .or_default();

                if rx.epochs.last() != Some(&epoch) {
                    rx.flush_epoch();
                    rx.epochs.push(epoch);
                    if signal.flag != EpochFlag::Ok {
                        *rx.flags.entry(format!("{:?}", signal.flag)).or_default() += 1;
                    }
                }

                let obs = &signal.signal;

                // sky: satellite seen, position from the broadcast ephemerides
                let receiver = signal.receiver.to_string();
                let position = self.positions.get(&receiver).copied();
                let sv_position = match position {
                    Some(_) => self
                        .skies
                        .get(&receiver)
                        .map(|sky| sky.needs_position(obs.sv, epoch))
                        .unwrap_or(true)
                        .then(|| self.satellite_position(obs.sv, epoch))
                        .flatten(),
                    None => None,
                };
                let sky = self.skies.entry(receiver).or_default();
                sky.seen(obs.sv, epoch);
                if let (Some(rx), Some(sv)) = (position, sv_position) {
                    let (azimuth, elevation) = azimuth_elevation(rx, sv);
                    sky.position(obs.sv, epoch, azimuth, elevation);
                }

                let rx = self
                    .receivers
                    .entry(signal.receiver.to_string())
                    .or_default();
                let origin = rx.origin().unwrap_or(epoch);
                let sat = rx.satellites.entry(obs.sv).or_default();

                if sat.last_epoch != Some(epoch) {
                    sat.last_epoch = Some(epoch);
                    sat.epochs += 1;
                    if sat.covered_now {
                        sat.covered += 1;
                    }
                    *rx.in_view
                        .entry(obs.sv.constellation.to_string())
                        .or_default() += 1;
                }

                let code = obs.observable.to_string();

                if let Some(lli) = obs.lli {
                    if lli.intersects(LliFlags::LOCK_LOSS) {
                        sat.lock_losses += 1;
                    }
                }

                if obs.observable.is_ssi_observable() {
                    sat.ssi.entry(code.clone()).or_default().push(obs.value);

                    let primary = rx
                        .primary_ssi
                        .entry(obs.sv.constellation)
                        .or_insert_with(|| code.clone());

                    if *primary == code {
                        rx.ssi_series
                            .entry(obs.sv)
                            .or_insert_with(|| Series::new(&obs.sv.to_string()))
                            .push(elapsed_s(origin, epoch), obs.value);
                    }
                }

                if !sat.observables.contains(&code) {
                    sat.observables.insert(code);
                }
            },
            QcOutput::Ephemeris(eph) => {
                if let Some(kepler) = QcKepler::from_ephemeris(eph.sv, eph.toc, &eph.ephemeris) {
                    let list = self.ephemerides.entry(eph.sv).or_default();
                    if !list.iter().any(|k| k.toc == kepler.toc) {
                        list.push(kepler);
                    }
                    // bounded memory: ephemerides of the last 12 hours
                    let toc = eph.toc;
                    list.retain(|k| (toc - k.toc).to_seconds() < 12.0 * 3600.0);
                }
            },
            QcOutput::EphemerisCoverage(coverage) => {
                let rx = self
                    .receivers
                    .entry(coverage.receiver.to_string())
                    .or_default();
                rx.coverage = true;

                let sat = rx.satellites.entry(coverage.sv).or_default();
                let covered = coverage.source.is_some();
                if sat.covered_now && !covered {
                    sat.coverage_losses += 1;
                }
                sat.covered_now = covered;
            },
            _ => {},
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        for (index, (receiver, rx)) in self.receivers.iter_mut().enumerate() {
            rx.flush_epoch();

            let (Some(first), Some(last)) = (rx.epochs.first(), rx.epochs.last()) else {
                continue;
            };
            let (first, last) = (*first, *last);

            let mut section = Section::new(
                &format!("receiver-{}", index),
                &format!("Observations: {}", receiver),
            );

            let sampling = rx.sampling();
            let gaps = sampling.map(|dt| rx.gaps(dt)).unwrap_or_default();

            let constellations = rx
                .satellites
                .keys()
                .map(|sv| sv.constellation.to_string())
                .unique()
                .join(", ");

            let mut summary = vec![
                ("Period".to_string(), format!("{} → {}", first, last)),
                ("Duration".to_string(), (last - first).to_string()),
                ("Epochs".to_string(), rx.epochs.len().to_string()),
                (
                    "Sampling".to_string(),
                    sampling.map(|dt| dt.to_string()).unwrap_or("-".to_string()),
                ),
                (
                    "Data gaps".to_string(),
                    if gaps.is_empty() {
                        "none".to_string()
                    } else {
                        let total = gaps
                            .iter()
                            .fold(Duration::ZERO, |acc, (a, b)| acc + (*b - *a));
                        format!("{} (total {})", gaps.len(), total)
                    },
                ),
                (
                    "Satellites".to_string(),
                    format!("{} ({})", rx.satellites.len(), constellations),
                ),
            ];

            if !rx.flags.is_empty() {
                summary.push((
                    "Abnormal epochs".to_string(),
                    rx.flags
                        .iter()
                        .map(|(flag, count)| format!("{}: {}", flag, count))
                        .join(", "),
                ));
            }

            if rx.coverage {
                let (observed, covered) = rx
                    .satellites
                    .values()
                    .fold((0, 0), |(o, c), sat| (o + sat.epochs, c + sat.covered));
                let uncovered = rx
                    .satellites
                    .iter()
                    .filter(|(_, sat)| sat.covered < sat.epochs)
                    .map(|(sv, _)| sv.to_string())
                    .join(", ");

                summary.push((
                    "Ephemeris coverage".to_string(),
                    format!(
                        "{:.2} % of the observations",
                        100.0 * covered as f64 / observed.max(1) as f64
                    ),
                ));

                if !uncovered.is_empty() {
                    summary.push(("Not fully covered".to_string(), uncovered));
                }
            }

            section.push(Block::KeyValue(summary));

            if !gaps.is_empty() {
                section.push(Block::Heading("Data gaps".to_string()));
                let mut table = Table::new(&["Start", "End", "Duration"]);
                for (start, end) in gaps.iter().take(MAX_GAPS) {
                    table.push(vec![
                        start.to_string(),
                        end.to_string(),
                        (*end - *start).to_string(),
                    ]);
                }
                section.push(Block::Table(table));
                if gaps.len() > MAX_GAPS {
                    section.push(Block::Text(format!(
                        "{} more gaps not listed.",
                        gaps.len() - MAX_GAPS
                    )));
                }
            }

            // satellites in view
            let mut chart = Chart::time("Satellites in view", "satellites", first);
            for series in rx.in_view_series.values() {
                chart.push(series.clone());
            }
            section.push(Block::Chart(chart));

            if let Some(sky) = self.skies.get(receiver) {
                if !sky.has_positions() {
                    section.push(Block::Text(
                        "Sky plot: requires the reference position of the receiver (RINEX header) and broadcast ephemerides (NAV RINEX, Keplerian constellations).".to_string(),
                    ));
                }
                sky.push_blocks(
                    &mut section,
                    if self.solved.contains(receiver) {
                        "Sky plot (broadcast ephemerides, position of the navigation solutions)"
                    } else {
                        "Sky plot (broadcast ephemerides, RINEX header position)"
                    },
                    "Satellites observed, over time",
                );
            }

            // signal strength, per constellation
            for (constellation, observable) in
                rx.primary_ssi.iter().sorted_by_key(|(c, _)| c.to_string())
            {
                let mut chart = Chart::time(
                    &format!("{} signal strength ({})", constellation, observable),
                    observable,
                    first,
                );
                for (_, series) in rx
                    .ssi_series
                    .iter()
                    .filter(|(sv, _)| sv.constellation == *constellation)
                {
                    chart.push(series.clone());
                }
                if !chart.is_empty() {
                    section.push(Block::Chart(chart));
                }
            }

            // per constellation
            section.push(Block::Heading("Constellations".to_string()));
            let mut table = Table::new(&["Constellation", "Satellites", "Observables", "Mean SSI"]);

            for (constellation, satellites) in rx
                .satellites
                .iter()
                .into_group_map_by(|(sv, _)| sv.constellation.to_string())
                .into_iter()
                .sorted_by(|a, b| a.0.cmp(&b.0))
            {
                let observables = satellites
                    .iter()
                    .flat_map(|(_, sat)| sat.observables.iter())
                    .unique()
                    .sorted()
                    .join(" ");

                let mut ssi = BTreeMap::<&String, Stats>::new();
                for (_, sat) in satellites.iter() {
                    for (code, stats) in sat.ssi.iter() {
                        let entry = ssi.entry(code).or_default();
                        entry.push(stats.mean());
                    }
                }

                table.push(vec![
                    constellation,
                    satellites.len().to_string(),
                    observables,
                    ssi.iter()
                        .map(|(code, stats)| format!("{} {}", code, fmt(stats.mean(), 1)))
                        .join(", "),
                ]);
            }
            section.push(Block::Table(table));

            // per satellite
            section.push(Block::Heading("Satellites".to_string()));
            let mut headers = vec!["SV", "Epochs", "Lock losses", "Observables", "SSI"];
            if rx.coverage {
                headers.insert(2, "Ephemeris coverage");
            }
            let mut table = Table::new(&headers);

            for (sv, sat) in rx
                .satellites
                .iter()
                .sorted_by_key(|(sv, _)| (sv.constellation.to_string(), sv.prn))
            {
                let primary = rx.primary_ssi.get(&sv.constellation);
                let ssi = primary
                    .and_then(|code| sat.ssi.get(code))
                    .map(|stats| format!("{} (σ {})", fmt(stats.mean(), 1), fmt(stats.std(), 1)))
                    .unwrap_or("-".to_string());

                let mut row = vec![
                    sv.to_string(),
                    sat.epochs.to_string(),
                    sat.lock_losses.to_string(),
                    sat.observables.len().to_string(),
                    ssi,
                ];

                if rx.coverage {
                    row.insert(
                        2,
                        format!(
                            "{:.1} %{}",
                            100.0 * sat.covered as f64 / sat.epochs.max(1) as f64,
                            if sat.coverage_losses > 0 {
                                format!(" ({} losses)", sat.coverage_losses)
                            } else {
                                String::new()
                            }
                        ),
                    );
                }

                table.push(row);
            }
            section.push(Block::Table(table));

            sections.push(section);
        }

        if sections.is_empty() {
            let mut section = Section::new("receivers", "Signal sources");
            section.push(Block::Notice(
                "No signal observations: the qc command analyzes observation RINEX.".to_string(),
            ));
            sections.push(section);
        }

        sections
    }
}
