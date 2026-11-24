//! Residual analyses: observations (receiver A − receiver B), orbits (product A − product B)
use std::collections::BTreeMap;

use itertools::Itertools;

use gnss_qc::prelude::{Epoch, QcOutput, QcOutputItem, SV};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, elapsed_s, stats::fmt, Block, Chart, ChartKind, Section, Series, Stats,
        Table,
    },
    sink::RunSummary,
};

#[derive(Debug, Default)]
struct Track {
    stats: Stats,
    series: Option<Series>,
}

impl Track {
    fn push(&mut self, name: &str, x: f64, y: f64) {
        self.stats.push(y);
        self.series
            .get_or_insert_with(|| Series::new(name))
            .push(x, y);
    }
}

/// Unit of an observable
fn unit(code: &str) -> &'static str {
    match code.chars().next() {
        Some('C') | Some('P') => "m",
        Some('L') => "cycles",
        Some('D') => "Hz",
        Some('S') => "dB-Hz",
        _ => "",
    }
}

#[derive(Debug, Default)]
pub struct ObservationResiduals {
    pair: Option<(String, String)>,
    origin: Option<Epoch>,
    /// observable → satellite → residuals
    residuals: BTreeMap<String, BTreeMap<SV, Track>>,
}

impl Analysis for ObservationResiduals {
    fn consume(&mut self, item: &QcOutputItem) {
        if let QcOutput::ObservationResidual(residual) = &item.payload {
            if self.pair.is_none() {
                self.pair = Some((residual.lhs.to_string(), residual.rhs.to_string()));
            }
            let origin = *self.origin.get_or_insert(item.epoch);

            self.residuals
                .entry(residual.observable.to_string())
                .or_default()
                .entry(residual.sv)
                .or_default()
                .push(
                    &residual.sv.to_string(),
                    elapsed_s(origin, item.epoch),
                    residual.residual,
                );
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut section = Section::new("observation-residuals", "Observation residuals");

        let (Some((lhs, rhs)), Some(origin)) = (&self.pair, self.origin) else {
            section.push(Block::Notice(
                "No residual: the two receivers have no common observation (same satellite, same observable, close epochs).".to_string(),
            ));
            return vec![section];
        };

        section.push(Block::KeyValue(vec![
            ("A".to_string(), lhs.clone()),
            ("B".to_string(), rhs.clone()),
            ("Observables".to_string(), self.residuals.keys().join(", ")),
        ]));

        section.push(Block::Text(
            "A − B for each satellite and observable. Pseudo range residuals reflect the relative clock offset of both receivers and their hardware delays; phase residuals include the ambiguities.".to_string(),
        ));

        let mut table = Table::new(&[
            "Observable",
            "Satellites",
            "Residuals",
            "Mean",
            "σ",
            "RMS",
            "Unit",
        ]);

        for (code, satellites) in self.residuals.iter() {
            let mut all = Stats::default();
            for track in satellites.values() {
                if let Some(series) = &track.series {
                    for y in series.y.iter() {
                        all.push(*y);
                    }
                }
            }
            table.push(vec![
                code.clone(),
                satellites.len().to_string(),
                all.n.to_string(),
                fmt(all.mean(), 3),
                fmt(all.std(), 3),
                fmt(all.rms(), 3),
                unit(code).to_string(),
            ]);
        }
        section.push(Block::Table(table));

        for (code, satellites) in self.residuals.iter() {
            let mut chart = Chart::time(
                &format!("{} residuals (A − B)", code),
                &format!("{} ({})", code, unit(code)),
                origin,
            )
            .with_kind(ChartKind::Markers);

            for track in satellites.values() {
                if let Some(series) = &track.series {
                    chart.push(series.clone());
                }
            }
            section.push(Block::Chart(chart));
        }

        vec![section]
    }
}

#[derive(Debug, Default)]
struct OrbitTracks {
    norm: Track,
    radial: Track,
    along: Track,
    cross: Track,
    clock_ns: Track,
}

#[derive(Debug, Default)]
pub struct OrbitResiduals {
    pair: Option<(String, String)>,
    origin: Option<Epoch>,
    satellites: BTreeMap<SV, OrbitTracks>,
}

impl Analysis for OrbitResiduals {
    fn consume(&mut self, item: &QcOutputItem) {
        if let QcOutput::OrbitResidual(residual) = &item.payload {
            if self.pair.is_none() {
                self.pair = Some((
                    format!("{:?} ({})", residual.lhs_kind, residual.lhs_source),
                    format!("{:?} ({})", residual.rhs_kind, residual.rhs_source),
                ));
            }

            let origin = *self.origin.get_or_insert(item.epoch);
            let x = elapsed_s(origin, item.epoch);
            let name = residual.sv.to_string();

            let tracks = self.satellites.entry(residual.sv).or_default();
            tracks.norm.push(&name, x, residual.norm_m());
            tracks.radial.push(&name, x, residual.radial_m);
            tracks.along.push(&name, x, residual.along_track_m);
            tracks.cross.push(&name, x, residual.cross_track_m);
            if let Some(dt) = residual.clock_s {
                tracks.clock_ns.push(&name, x, dt * 1.0E9);
            }
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut section = Section::new("orbit-residuals", "Orbit residuals");

        let (Some((lhs, rhs)), Some(origin)) = (&self.pair, self.origin) else {
            section.push(Block::Notice(
                "No residual: both orbital products must cover the same satellites at the same instants.".to_string(),
            ));
            return vec![section];
        };

        section.push(Block::KeyValue(vec![
            ("A".to_string(), lhs.clone()),
            ("B".to_string(), rhs.clone()),
            ("Satellites".to_string(), self.satellites.len().to_string()),
        ]));

        // per constellation
        let mut table = Table::new(&[
            "Constellation",
            "Satellites",
            "RMS radial (m)",
            "RMS along (m)",
            "RMS cross (m)",
            "RMS 3D (m)",
            "Clock σ (ns)",
        ]);

        for (constellation, satellites) in self
            .satellites
            .iter()
            .into_group_map_by(|(sv, _)| sv.constellation.to_string())
            .into_iter()
            .sorted_by(|a, b| a.0.cmp(&b.0))
        {
            let mut stats = [Stats::default(); 5];
            for (_, tracks) in satellites.iter() {
                for (i, track) in [
                    &tracks.radial,
                    &tracks.along,
                    &tracks.cross,
                    &tracks.norm,
                    &tracks.clock_ns,
                ]
                .iter()
                .enumerate()
                {
                    if let Some(series) = &track.series {
                        for y in series.y.iter() {
                            stats[i].push(*y);
                        }
                    }
                }
            }
            table.push(vec![
                constellation,
                satellites.len().to_string(),
                fmt(stats[0].rms(), 3),
                fmt(stats[1].rms(), 3),
                fmt(stats[2].rms(), 3),
                fmt(stats[3].rms(), 3),
                fmt(stats[4].std(), 3),
            ]);
        }
        section.push(Block::Table(table));

        for (title, unit, select) in [
            (
                "3D residual",
                "m",
                (|t: &OrbitTracks| &t.norm) as fn(&OrbitTracks) -> &Track,
            ),
            ("Radial residual", "m", |t: &OrbitTracks| &t.radial),
            ("Along track residual", "m", |t: &OrbitTracks| &t.along),
            ("Cross track residual", "m", |t: &OrbitTracks| &t.cross),
            ("Clock residual", "ns", |t: &OrbitTracks| &t.clock_ns),
        ] {
            let mut chart = Chart::time(title, unit, origin);
            for tracks in self.satellites.values() {
                if let Some(series) = &select(tracks).series {
                    chart.push(series.clone());
                }
            }
            if !chart.is_empty() {
                section.push(Block::Chart(chart));
            }
        }

        // per satellite
        section.push(Block::Heading("Satellites".to_string()));
        let mut table = Table::new(&[
            "SV",
            "Residuals",
            "RMS radial (m)",
            "RMS along (m)",
            "RMS cross (m)",
            "Max 3D (m)",
            "Clock mean (ns)",
            "Clock σ (ns)",
        ]);

        for (sv, tracks) in self.satellites.iter() {
            table.push(vec![
                sv.to_string(),
                tracks.norm.stats.n.to_string(),
                fmt(tracks.radial.stats.rms(), 3),
                fmt(tracks.along.stats.rms(), 3),
                fmt(tracks.cross.stats.rms(), 3),
                fmt(tracks.norm.stats.max_abs(), 3),
                fmt(tracks.clock_ns.stats.mean(), 3),
                fmt(tracks.clock_ns.stats.std(), 3),
            ]);
        }
        section.push(Block::Table(table));

        vec![section]
    }
}
