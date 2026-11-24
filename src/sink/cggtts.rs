//! CGGTTS files: one file per receiver and constellation
use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
};

use log::info;

use cggtts::prelude::{Coordinates, CGGTTS};

use gnss_qc::prelude::{QcOutput, QcOutputItem};

use crate::{
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

/// CGGTTS header settings
#[derive(Debug, Clone)]
pub struct CggttsHeader {
    /// Station name
    pub station: String,

    /// Reference frame of the coordinates
    pub frame: String,

    /// Antenna phase center (ECEF m)
    pub apc_ecef_m: Option<(f64, f64, f64)>,

    /// Comment
    pub comment: String,
}

pub struct CggttsSink {
    workspace: Workspace,
    header: CggttsHeader,
    /// (receiver, constellation) → file
    files: BTreeMap<(String, String), CGGTTS>,
}

impl CggttsSink {
    pub fn new(workspace: &Workspace, header: CggttsHeader) -> Self {
        Self {
            workspace: workspace.clone(),
            header,
            files: BTreeMap::new(),
        }
    }

    fn new_file(&self) -> CGGTTS {
        let mut file = CGGTTS::default();
        file.header = file
            .header
            .with_station(&self.header.station)
            .with_reference_frame(&self.header.frame)
            .with_comment(&self.header.comment);

        if let Some((x, y, z)) = self.header.apc_ecef_m {
            file.header = file.header.with_apc_coordinates(Coordinates { x, y, z });
        }

        file
    }
}

impl Consumer for CggttsSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        if let QcOutput::CggttsTrack(track) = &item.payload {
            let key = (
                track.receiver.to_string(),
                format!("{:x}", track.track.sv.constellation),
            );
            if !self.files.contains_key(&key) {
                let file = self.new_file();
                self.files.insert(key.clone(), file);
            }
            self.files
                .get_mut(&key)
                .unwrap()
                .tracks
                .push((*track.track).clone());
        }
        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        let multiple_receivers = self
            .files
            .keys()
            .map(|(receiver, _)| receiver)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1;

        for ((receiver, constellation), file) in self.files.iter() {
            let mut name = format!("{}_{}", self.header.station, constellation);
            if multiple_receivers {
                name.push_str(&format!(
                    "_{}",
                    receiver
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>()
                ));
            }
            let name = format!("{}.cggtts", name);

            let fd = self.workspace.create_file(&name)?;
            let mut writer = BufWriter::new(fd);
            file.format(&mut writer)
                .map_err(|e| Error::Config(format!("CGGTTS formatting: {:?}", e)))?;
            writer.flush()?;

            info!(
                "{} generated ({} tracks)",
                self.workspace.path(&name).display(),
                file.tracks.len()
            );
        }
        Ok(())
    }
}
