//! Ionospheric Pierce Points (ipp command)
use std::collections::BTreeMap;

use gnss_qc::prelude::{QcOutput, QcOutputItem, SV};

use crate::{
    engine::SessionContext,
    report::{
        analysis::Analysis, stats::fmt, Block, Chart, ChartKind, Section, Series, Stats, Table,
    },
    sink::RunSummary,
};

#[derive(Debug, Default)]
struct SatellitePoints {
    elevation: Stats,
    obliquity: Stats,
    points: Option<Series>,
}

#[derive(Debug, Default)]
pub struct IppAnalysis {
    receivers: BTreeMap<String, BTreeMap<SV, SatellitePoints>>,
}

impl Analysis for IppAnalysis {
    fn consume(&mut self, item: &QcOutputItem) {
        if let QcOutput::IPP(ipp) = &item.payload {
            let sat = self
                .receivers
                .entry(ipp.receiver.to_string())
                .or_default()
                .entry(ipp.sv)
                .or_default();
            sat.elevation.push(ipp.elevation_deg);
            sat.obliquity.push(ipp.obliquity_factor);
            sat.points
                .get_or_insert_with(|| Series::new(&ipp.sv.to_string()))
                .push(ipp.longitude_deg, ipp.latitude_deg);
        }
    }

    fn sections(&mut self, _: &SessionContext, _: &RunSummary) -> Vec<Section> {
        let mut sections = Vec::new();

        for (index, (receiver, satellites)) in self.receivers.iter().enumerate() {
            let mut section = Section::new(
                &format!("ipp-{}", index),
                &format!("Ionospheric Pierce Points: {}", receiver),
            );

            let mut chart = Chart::xy("Pierce points", "Longitude (°)", "Latitude (°)")
                .with_kind(ChartKind::Map);
            for sat in satellites.values() {
                if let Some(points) = &sat.points {
                    chart.push(points.clone());
                }
            }
            section.push(Block::Chart(chart));

            let mut table = Table::new(&["SV", "Points", "Elevation (°)", "Mean obliquity"]);
            for (sv, sat) in satellites.iter() {
                table.push(vec![
                    sv.to_string(),
                    sat.elevation.n.to_string(),
                    format!(
                        "{} → {}",
                        fmt(sat.elevation.min, 1),
                        fmt(sat.elevation.max, 1)
                    ),
                    fmt(sat.obliquity.mean(), 3),
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
