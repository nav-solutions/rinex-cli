//! CSV files: one file per kind of result
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
};

use log::info;

use gnss_qc::prelude::{QcOutput, QcOutputItem};

use crate::{
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

/// Quotes a CSV field when required
pub fn field(text: &str) -> Cow<'_, str> {
    if text.contains([',', '"', '\n']) {
        Cow::Owned(format!("\"{}\"", text.replace('"', "\"\"")))
    } else {
        Cow::Borrowed(text)
    }
}

fn opt<T: std::fmt::Display>(value: &Option<T>) -> String {
    value.as_ref().map(|v| v.to_string()).unwrap_or_default()
}

pub struct CsvSink {
    workspace: Workspace,
    raw: bool,
    files: HashMap<&'static str, BufWriter<File>>,
}

impl CsvSink {
    pub fn new(workspace: &Workspace, raw: bool) -> Self {
        Self {
            workspace: workspace.clone(),
            raw,
            files: HashMap::new(),
        }
    }

    /// Returns the file for this kind of result, created on first use
    fn file(&mut self, name: &'static str, header: &str) -> Result<&mut BufWriter<File>, Error> {
        if !self.files.contains_key(name) {
            let mut fd = self.workspace.create_file(&format!("data/{}.csv", name))?;
            writeln!(fd, "{}", header)?;
            self.files.insert(name, fd);
        }
        Ok(self.files.get_mut(name).unwrap())
    }
}

impl Consumer for CsvSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        let epoch = item.epoch;

        match &item.payload {
            QcOutput::Signal(signal) => {
                if self.raw {
                    let fd =
                        self.file("signals", "epoch,receiver,flag,sv,observable,value,lli,snr")?;
                    let obs = &signal.signal;
                    writeln!(
                        fd,
                        "{},{},{:?},{},{},{},{},{}",
                        epoch,
                        field(&signal.receiver.to_string()),
                        signal.flag,
                        obs.sv,
                        obs.observable,
                        obs.value,
                        obs.lli
                            .map(|lli| lli.bits().to_string())
                            .unwrap_or_default(),
                        obs.snr
                            .map(|snr| f64::from(snr).to_string())
                            .unwrap_or_default(),
                    )?;
                }
            },
            QcOutput::EphemerisCoverage(coverage) => {
                let fd = self.file("coverage", "epoch,receiver,sv,covered,source,toc")?;
                writeln!(
                    fd,
                    "{},{},{},{},{},{}",
                    epoch,
                    field(&coverage.receiver.to_string()),
                    coverage.sv,
                    coverage.source.is_some(),
                    field(&opt(&coverage.source)),
                    opt(&coverage.toc),
                )?;
            },
            QcOutput::ObservationResidual(residual) => {
                let fd = self.file(
                    "observation_residuals",
                    "epoch,lhs,rhs,sv,observable,residual,rhs_epoch",
                )?;
                writeln!(
                    fd,
                    "{},{},{},{},{},{},{}",
                    epoch,
                    field(&residual.lhs.to_string()),
                    field(&residual.rhs.to_string()),
                    residual.sv,
                    residual.observable,
                    residual.residual,
                    residual.rhs_epoch,
                )?;
            },
            QcOutput::OrbitResidual(residual) => {
                let fd = self.file(
                    "orbit_residuals",
                    "epoch,sv,lhs_kind,lhs_source,lhs_toc,rhs_kind,rhs_source,rhs_toc,dx_m,dy_m,dz_m,norm_m,radial_m,along_track_m,cross_track_m,clock_s",
                )?;
                let (dx, dy, dz) = residual.ecef_m;
                writeln!(
                    fd,
                    "{},{},{:?},{},{},{:?},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{}",
                    epoch,
                    residual.sv,
                    residual.lhs_kind,
                    field(&residual.lhs_source.to_string()),
                    opt(&residual.lhs_toc),
                    residual.rhs_kind,
                    field(&residual.rhs_source.to_string()),
                    opt(&residual.rhs_toc),
                    dx,
                    dy,
                    dz,
                    residual.norm_m(),
                    residual.radial_m,
                    residual.along_track_m,
                    residual.cross_track_m,
                    residual
                        .clock_s
                        .map(|dt| format!("{:e}", dt))
                        .unwrap_or_default(),
                )?;
            },
            #[cfg(feature = "navigation")]
            QcOutput::PVTSolution(solution) => {
                let fd = self.file(
                    "solutions",
                    "epoch,receiver,type,x_m,y_m,z_m,lat_deg,long_deg,alt_m,vx_m_s,vy_m_s,vz_m_s,clock_offset_s,timescale,gdop,hdop,vdop,tdop,satellites",
                )?;
                let pvt = &solution.solution;
                let (x, y, z) = pvt.pos_m;
                let (lat, long, alt) = pvt.lat_long_alt_deg_deg_m;
                let (vx, vy, vz) = pvt.vel_m_s;
                writeln!(
                    fd,
                    "{},{},{:?},{:.4},{:.4},{:.4},{:.9},{:.9},{:.4},{:.4},{:.4},{:.4},{:e},{},{:.3},{:.3},{:.3},{:.3},{}",
                    epoch,
                    field(&solution.receiver.to_string()),
                    pvt.solution_type,
                    x,
                    y,
                    z,
                    lat,
                    long,
                    alt,
                    vx,
                    vy,
                    vz,
                    pvt.clock_offset_s,
                    pvt.timescale,
                    pvt.gdop,
                    pvt.hdop,
                    pvt.vdop,
                    pvt.tdop,
                    pvt.sv.len(),
                )?;
            },
            #[cfg(feature = "cggtts")]
            QcOutput::CggttsTrack(track) => {
                let fd = self.file(
                    "tracks",
                    "epoch,receiver,sv,class,duration_s,elevation_deg,azimuth_deg,refsv_s,srsv_s_s,refsys_s,srsys_s_s,dsg_s,ioe,mdtr_s,smdt_s_s,mdio_s,smdi_s_s,frc",
                )?;
                let trk = &track.track;
                writeln!(
                    fd,
                    "{},{},{},{:?},{},{:.2},{:.2},{:e},{:e},{:e},{:e},{:e},{},{:e},{:e},{:e},{:e},{}",
                    trk.epoch,
                    field(&track.receiver.to_string()),
                    trk.sv,
                    trk.class,
                    trk.duration.to_seconds(),
                    trk.elevation_deg,
                    trk.azimuth_deg,
                    trk.data.refsv,
                    trk.data.srsv,
                    trk.data.refsys,
                    trk.data.srsys,
                    trk.data.dsg,
                    trk.data.ioe,
                    trk.data.mdtr,
                    trk.data.smdt,
                    trk.data.mdio,
                    trk.data.smdi,
                    trk.frc,
                )?;
            },
            #[cfg(feature = "ionex")]
            QcOutput::IPP(ipp) => {
                let fd = self.file(
                    "ipp",
                    "epoch,receiver,sv,orbit,elevation_deg,azimuth_deg,latitude_deg,longitude_deg,altitude_km,obliquity_factor",
                )?;
                writeln!(
                    fd,
                    "{},{},{},{:?},{:.3},{:.3},{:.5},{:.5},{:.1},{:.5}",
                    epoch,
                    field(&ipp.receiver.to_string()),
                    ipp.sv,
                    ipp.orbit,
                    ipp.elevation_deg,
                    ipp.azimuth_deg,
                    ipp.latitude_deg,
                    ipp.longitude_deg,
                    ipp.altitude_km,
                    ipp.obliquity_factor,
                )?;
            },
            // end of session summaries: see summary.json
            _ => {},
        }
        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        let mut names = self.files.keys().copied().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let fd = self.files.get_mut(name).unwrap();
            fd.flush()?;
            info!(
                "{} generated",
                self.workspace.path(&format!("data/{}.csv", name)).display()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::field;

    #[test]
    fn csv_fields() {
        assert_eq!(
            field("receiver:SEPT POLARX5 #1"),
            "receiver:SEPT POLARX5 #1"
        );
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("a\"b"), "\"a\"\"b\"");
    }
}
