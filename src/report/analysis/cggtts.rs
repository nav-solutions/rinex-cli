//! CGGTTS tracks (cggtts command)
use std::collections::BTreeMap;

use gnss_qc::prelude::{Epoch, QcCggttsReport, QcOutput, QcOutputItem, SV};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, elapsed_s, sky::SkyAccumulator, stats::fmt, Block, Chart, ChartKind,
        Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

#[derive(Debug, Default)]
struct SatelliteTracks {
    refsys_ns: Stats,
    refsv_ns: Stats,
    elevation: Stats,
    dsg_ns: Stats,
    refsys: Option<Series>,
}

#[derive(Debug, Default)]
pub struct CggttsAnalysis {
    origin: Option<Epoch>,
    satellites: BTreeMap<SV, SatelliteTracks>,
    reports: Vec<QcCggttsReport>,
    /// Tracks, in the sky of the receiver (azimuth and elevation at midpoint)
    sky: SkyAccumulator,
}

impl Analysis for CggttsAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        match &item.payload {
            QcOutput::CggttsTrack(track) => {
                let trk = &track.track;
                let origin = *self.origin.get_or_insert(trk.epoch);
                // sky: the track covers [epoch, epoch + duration]
                let middle = trk.epoch + trk.duration / 2;
                self.sky.seen(trk.sv, trk.epoch);
                self.sky.seen(trk.sv, middle);
                self.sky.seen(trk.sv, trk.epoch + trk.duration);
                self.sky
                    .position(trk.sv, middle, trk.azimuth_deg, trk.elevation_deg);

                let sat = self.satellites.entry(trk.sv).or_default();
                sat.refsys_ns.push(trk.data.refsys * 1.0E9);
                sat.refsv_ns.push(trk.data.refsv * 1.0E9);
                sat.elevation.push(trk.elevation_deg);
                sat.dsg_ns.push(trk.data.dsg * 1.0E9);
                sat.refsys
                    .get_or_insert_with(|| Series::new(&trk.sv.to_string()))
                    .push(elapsed_s(origin, trk.epoch), trk.data.refsys * 1.0E9);
            },
            QcOutput::CggttsReport(report) => self.reports.push(report.clone()),
            _ => {},
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut section = Section::new("cggtts", "CGGTTS");

        for report in self.reports.iter() {
            section.push(Block::KeyValue(vec![
                ("Receiver".to_string(), report.receiver.to_string()),
                ("Tracks".to_string(), report.tracks.to_string()),
                (
                    "Short tracks (discarded)".to_string(),
                    report.short_tracks.to_string(),
                ),
                ("Fit errors".to_string(), report.fit_errors.to_string()),
            ]));
        }

        let Some(origin) = self.origin else {
            section.push(Block::Notice("No CGGTTS track resolved.".to_string()));
            return vec![section];
        };

        let mut chart = Chart::time("REFSYS", "ns", origin).with_kind(ChartKind::Markers);
        let mut all = Stats::default();
        for sat in self.satellites.values() {
            if let Some(series) = &sat.refsys {
                for y in series.y.iter() {
                    all.push(*y);
                }
                chart.push(series.clone());
            }
        }
        section.push(Block::Chart(chart));

        section.push(Block::Text(format!(
            "REFSYS over all tracks: mean {} ns, σ {} ns.",
            fmt(all.mean(), 3),
            fmt(all.std(), 3)
        )));

        let mut table = Table::new(&[
            "SV",
            "Tracks",
            "REFSYS mean (ns)",
            "REFSYS σ (ns)",
            "REFSV mean (ns)",
            "DSG mean (ns)",
            "Elevation (°)",
        ]);

        for (sv, sat) in self.satellites.iter() {
            table.push(vec![
                sv.to_string(),
                sat.refsys_ns.n.to_string(),
                fmt(sat.refsys_ns.mean(), 3),
                fmt(sat.refsys_ns.std(), 3),
                fmt(sat.refsv_ns.mean(), 3),
                fmt(sat.dsg_ns.mean(), 3),
                format!(
                    "{} → {}",
                    fmt(sat.elevation.min, 1),
                    fmt(sat.elevation.max, 1)
                ),
            ]);
        }
        section.push(Block::Table(table));

        section.push(Block::Heading("Tracks in the sky".to_string()));
        self.sky.push_blocks(
            &mut section,
            "Sky plot: CGGTTS tracks (azimuth and elevation at midpoint)",
            "CGGTTS tracks, over time",
        );

        vec![section]
    }
}
