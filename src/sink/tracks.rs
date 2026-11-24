//! P.V.T solutions formatted as tracks: GPX, KML
use std::{collections::BTreeMap, io::Write};

use log::info;

use gnss_qc::prelude::{Epoch, QcOutput, QcOutputItem};

use crate::{
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackFormat {
    Gpx,
    Kml,
}

#[derive(Debug, Clone, Copy)]
struct Point {
    epoch: Epoch,
    lat_deg: f64,
    long_deg: f64,
    alt_m: f64,
}

/// Collects the solutions of each receiver, formats them at the end of the session
pub struct TrackSink {
    workspace: Workspace,
    format: TrackFormat,
    tracks: BTreeMap<String, Vec<Point>>,
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// File name compatible name
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// ISO 8601 (UTC)
fn iso8601(epoch: Epoch) -> String {
    let (y, m, d, hh, mm, ss, _) = epoch.to_gregorian_utc();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hh, mm, ss)
}

impl TrackSink {
    pub fn new(workspace: &Workspace, format: TrackFormat) -> Self {
        Self {
            workspace: workspace.clone(),
            format,
            tracks: BTreeMap::new(),
        }
    }

    fn write_gpx<W: Write>(fd: &mut W, name: &str, points: &[Point]) -> std::io::Result<()> {
        writeln!(fd, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            fd,
            r#"<gpx version="1.1" creator="rinex-cli {}" xmlns="http://www.topografix.com/GPX/1/1">"#,
            env!("CARGO_PKG_VERSION")
        )?;
        writeln!(fd, "  <trk><name>{}</name><trkseg>", xml_escape(name))?;
        for p in points {
            writeln!(
                fd,
                r#"    <trkpt lat="{:.9}" lon="{:.9}"><ele>{:.3}</ele><time>{}</time></trkpt>"#,
                p.lat_deg,
                p.long_deg,
                p.alt_m,
                iso8601(p.epoch)
            )?;
        }
        writeln!(fd, "  </trkseg></trk>")?;
        writeln!(fd, "</gpx>")
    }

    fn write_kml<W: Write>(fd: &mut W, name: &str, points: &[Point]) -> std::io::Result<()> {
        writeln!(fd, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            fd,
            r#"<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:gx="http://www.google.com/kml/ext/2.2">"#
        )?;
        writeln!(fd, "<Document><name>{}</name>", xml_escape(name))?;
        writeln!(fd, "<Placemark><name>{}</name><gx:Track>", xml_escape(name))?;
        writeln!(fd, "<altitudeMode>absolute</altitudeMode>")?;
        for p in points {
            writeln!(fd, "<when>{}</when>", iso8601(p.epoch))?;
        }
        for p in points {
            writeln!(
                fd,
                "<gx:coord>{:.9} {:.9} {:.3}</gx:coord>",
                p.long_deg, p.lat_deg, p.alt_m
            )?;
        }
        writeln!(fd, "</gx:Track></Placemark></Document>")?;
        writeln!(fd, "</kml>")
    }
}

impl Consumer for TrackSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        if let QcOutput::PVTSolution(solution) = &item.payload {
            let (lat_deg, long_deg, alt_m) = solution.solution.lat_long_alt_deg_deg_m;
            self.tracks
                .entry(solution.receiver.to_string())
                .or_default()
                .push(Point {
                    epoch: solution.solution.epoch,
                    lat_deg,
                    long_deg,
                    alt_m,
                });
        }
        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        for (receiver, points) in self.tracks.iter() {
            let extension = match self.format {
                TrackFormat::Gpx => "gpx",
                TrackFormat::Kml => "kml",
            };

            let name = format!("data/{}.{}", sanitize(receiver), extension);
            let mut fd = self.workspace.create_file(&name)?;

            match self.format {
                TrackFormat::Gpx => Self::write_gpx(&mut fd, receiver, points)?,
                TrackFormat::Kml => Self::write_kml(&mut fd, receiver, points)?,
            }

            fd.flush()?;
            info!("{} generated", self.workspace.path(&name).display());
        }
        Ok(())
    }
}
