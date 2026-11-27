//! IONEX synthesis: the nodes of the TEC maps (native and interpolated) streamed by
//! GNSS-Qc are accumulated, per product, then formatted as new IONEX files.
use std::collections::BTreeMap;

use log::{info, warn};

use gnss_qc::prelude::{QcIndexing, QcOutput, QcOutputItem, QcTecInterpolation};

use ionex::prelude::{Key, Record, IONEX, TEC};

use crate::{
    error::Error,
    input::file_stem,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

pub struct IonexSink {
    workspace: Workspace,
    /// Original products: header of the synthesized files
    originals: Vec<(QcIndexing, IONEX)>,
    interpolation: QcTecInterpolation,
    gzip: bool,
    /// Accumulated nodes, per product
    records: BTreeMap<String, (QcIndexing, Record)>,
}

impl IonexSink {
    pub fn new(
        workspace: &Workspace,
        originals: &[(QcIndexing, IONEX)],
        interpolation: QcTecInterpolation,
        gzip: bool,
    ) -> Self {
        Self {
            workspace: workspace.clone(),
            originals: originals.to_vec(),
            interpolation,
            gzip,
            records: BTreeMap::new(),
        }
    }
}

/// "15M", "01H", "30S" (sampling interval, file names)
pub fn period_code(seconds: f64) -> String {
    if seconds >= 3600.0 && seconds % 3600.0 == 0.0 {
        format!("{:02}H", (seconds / 3600.0) as u32)
    } else if seconds >= 60.0 && seconds % 60.0 == 0.0 {
        format!("{:02}M", (seconds / 60.0) as u32)
    } else {
        format!("{:02}S", seconds as u32)
    }
}

impl Consumer for IonexSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        if let QcOutput::TecPoint(node) = &item.payload {
            let p = &node.point;

            let mut tec = TEC::from_tecu(p.tecu);
            if let Some(rms) = p.rms_tecu {
                tec = tec.with_rms(rms);
            }

            self.records
                .entry(node.source.to_string())
                .or_insert_with(|| (node.source.clone(), Record::default()))
                .1
                .insert(
                    Key::from_decimal_degrees_km(
                        item.epoch,
                        p.latitude_deg,
                        p.longitude_deg,
                        p.altitude_km,
                    ),
                    tec,
                );
        }
        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        for (_, (identity, record)) in std::mem::take(&mut self.records) {
            let Some((_, original)) = self.originals.iter().find(|(id, _)| *id == identity) else {
                warn!("{}: original IONEX not found, not synthesized", identity);
                continue;
            };

            let mut synthesized = original.with_record(record);
            let epochs = synthesized.epoch_iter().collect::<Vec<_>>();

            if epochs.len() < 2 {
                continue;
            }

            let period = epochs
                .windows(2)
                .map(|w| w[1] - w[0])
                .min()
                .unwrap_or_default();

            let header = &mut synthesized.header;
            header.epoch_of_first_map = epochs[0];
            header.epoch_of_last_map = *epochs.last().unwrap();
            header.number_of_maps = epochs.len() as u32;
            header.sampling_period = period;
            header.comments.push(format!(
                "{} maps ({} interval), interpolated in time ({:?})",
                epochs.len(),
                period,
                self.interpolation
            ));
            header.comments.push(format!(
                "by rinex-cli v{} (GNSS-Qc)",
                env!("CARGO_PKG_VERSION")
            ));

            // CKMG0080.09I (2h maps), x2 -> CKMG0080_01H.09I
            let stem = original
                .attributes
                .as_ref()
                .map(|_| original.generate_standardized_filename())
                .map(|name| file_stem(std::path::Path::new(&name)))
                .unwrap_or_else(|| identity.to_string().replace([':', ' ', '/'], "_"));

            let (year, _, _, _, _, _, _) = epochs[0].to_gregorian_utc();

            let name = format!(
                "IONEX/{}_{}.{:02}I{}",
                stem,
                period_code(period.to_seconds()),
                year % 100,
                if self.gzip { ".gz" } else { "" }
            );

            let path = self.workspace.path(&name);

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let result = if self.gzip {
                synthesized.to_gzip_file(&path)
            } else {
                synthesized.to_file(&path)
            };

            result.map_err(|e| Error::Config(format!("IONEX formatting: {:?}", e)))?;

            info!(
                "{} generated ({} maps, one every {})",
                path.display(),
                epochs.len(),
                period
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::period_code;

    #[test]
    fn period_codes() {
        assert_eq!(period_code(3600.0), "01H");
        assert_eq!(period_code(1800.0), "30M");
        assert_eq!(period_code(30.0), "30S");
    }
}
