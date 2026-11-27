//! obs2tec --ionex: the TEC estimated from the observations, gridded into maps and
//! formatted as a standard IONEX file, that can be injected back into rinex-cli (tec).
use log::{info, warn};

use gnss_qc::prelude::{Constellation, QcOutputItem};

use ionex::prelude::{Header, Key, Linspace, MappingFunction, Record, ReferenceSystem, IONEX, TEC};

use crate::{
    error::Error,
    obs2tec::{Obs2TecSetup, TecCollector, TecMaps},
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

pub struct Obs2TecIonexSink {
    workspace: Workspace,
    /// File name prefix (session)
    name: String,
    setup: Obs2TecSetup,
    gzip: bool,
    collector: TecCollector,
}

impl Obs2TecIonexSink {
    pub fn new(workspace: &Workspace, name: &str, setup: &Obs2TecSetup, gzip: bool) -> Self {
        Self {
            workspace: workspace.clone(),
            name: name.to_string(),
            setup: setup.clone(),
            gzip,
            collector: TecCollector::default(),
        }
    }
}

/// IONEX file name: 9 characters (V3 like) prefix, map interval, year
fn file_name(name: &str, maps: &TecMaps, gzip: bool) -> String {
    let mut prefix = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .take(9)
        .collect::<String>();

    if prefix.is_empty() {
        prefix = "OBS2TEC".to_string();
    }

    let first = maps.maps.keys().next().copied().unwrap_or_default();
    let (year, _, _, _, _, _, _) = first.to_gregorian_utc();

    format!(
        "IONEX/{}_{}.{:02}I{}",
        prefix,
        crate::sink::period_code(maps.interval.to_seconds()),
        year % 100,
        if gzip { ".gz" } else { "" }
    )
}

/// Builds the IONEX of these maps
pub fn ionex(collector: &TecCollector, setup: &Obs2TecSetup, maps: &TecMaps) -> Option<IONEX> {
    let mut record = Record::default();

    for (epoch, map) in maps.maps.iter() {
        for ((i, j), node) in map.nodes.iter() {
            record.insert(
                Key::from_decimal_degrees_km(
                    *epoch,
                    maps.latitude_deg(*i),
                    maps.longitude_deg(*j),
                    maps.altitude_km,
                ),
                TEC::from_tecu(node.tecu).with_rms(node.rms_tecu),
            );
        }
    }

    let first = *maps.maps.keys().next()?;
    let last = *maps.maps.keys().next_back()?;

    let constellations = collector.constellations();
    let constellation = if constellations.len() == 1 {
        *constellations.iter().next().unwrap()
    } else {
        Constellation::Mixed
    };

    let receivers = collector.receivers();

    let mut header = Header::default()
        .with_latitude_grid(maps.latitude)
        .with_longitude_grid(maps.longitude)
        .with_altitude_grid(Linspace::new(maps.altitude_km, maps.altitude_km, 0.0).ok()?)
        .with_mapping_function(MappingFunction::CosZ)
        .with_elevation_cutoff(setup.elevation_mask_deg as f32)
        .with_base_radius_km(setup.earth_radius_km as f32)
        .with_map_dimension(2)
        .with_reference_system(ReferenceSystem::Constellation(constellation))
        .with_description("Vertical TEC from the observations (rinex-cli obs2tec)")
        .with_epoch_of_first_map(first)
        .with_epoch_of_last_map(last)
        .with_number_of_maps(maps.maps.len() as u32);

    header.sampling_period = maps.interval;
    header.num_stations = receivers.len() as u32;
    header.num_satellites = collector.satellites().len() as u16;
    header.program = Some(format!("rinex-cli v{}", env!("CARGO_PKG_VERSION")));
    // no agency: the IONEX injected back into rinex-cli is identified by its file name
    header.run_by = None;

    for comment in [
        "TEC estimated from dual frequency code and phase observations:".to_string(),
        "phase leveled onto the code over each continuous phase arc,".to_string(),
        format!(
            "single layer model ({} km), elevation mask {} deg.",
            setup.shell_altitude_km, setup.elevation_mask_deg
        ),
        "UNCALIBRATED: satellite and receiver differential code biases".to_string(),
        "(DCB) are NOT corrected. TEC biased by the DCB of each pair.".to_string(),
        format!(
            "Gridding: inverse distance weighted, radius {} deg, {}",
            setup.radius_deg,
            if setup.rotation {
                "Sun-fixed"
            } else {
                "Earth-fixed"
            }
        ),
        format!(
            "frame, >= {} pierce points per node. RMS: dispersion.",
            setup.min_samples
        ),
        format!(
            "Receivers: {}",
            receivers.iter().cloned().collect::<Vec<_>>().join(" ")
        ),
    ] {
        // IONEX comments: 60 characters
        let mut line = String::new();
        for word in comment.split_whitespace() {
            if !line.is_empty() && line.len() + word.len() + 1 > 60 {
                header.comments.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            header.comments.push(line);
        }
    }

    Some(IONEX::new(header, record))
}

impl Consumer for Obs2TecIonexSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        self.collector.consume(item);
        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        let Some(maps) = self.collector.grid(&self.setup) else {
            warn!("obs2tec: no TEC estimate, no IONEX");
            return Ok(());
        };

        let Some(ionex) = ionex(&self.collector, &self.setup, &maps) else {
            warn!("obs2tec: invalid grid, no IONEX");
            return Ok(());
        };

        let path = self
            .workspace
            .path(&file_name(&self.name, &maps, self.gzip));

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let result = if self.gzip {
            ionex.to_gzip_file(&path)
        } else {
            ionex.to_file(&path)
        };

        result.map_err(|e| Error::Config(format!("IONEX formatting: {:?}", e)))?;

        info!(
            "{} generated ({} maps, one every {}, {} × {} nodes), uncalibrated: DCB not corrected",
            path.display(),
            maps.maps.len(),
            maps.interval,
            maps.latitude.num_points(),
            maps.longitude.num_points(),
        );

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::obs2tec::{Node, TecMap};
    use gnss_qc::prelude::{Duration, Epoch};
    use std::{collections::BTreeMap, str::FromStr};

    #[test]
    fn ionex_formatting() {
        let t0 = Epoch::from_str("2020-06-25T00:00:00 UTC").unwrap();

        let mut maps = TecMaps {
            latitude: Linspace::new(57.5, 52.5, -2.5).unwrap(),
            longitude: Linspace::new(5.0, 15.0, 5.0).unwrap(),
            altitude_km: 450.0,
            interval: Duration::from_hours(1.0),
            maps: BTreeMap::new(),
        };

        for k in 0..3 {
            let mut map = TecMap::default();
            map.nodes.insert(
                (1, 1),
                Node {
                    tecu: 10.0 + k as f64,
                    rms_tecu: 1.0,
                    samples: 5,
                },
            );
            // negative: uncalibrated DCB
            map.nodes.insert(
                (0, 0),
                Node {
                    tecu: -3.2,
                    rms_tecu: 0.5,
                    samples: 5,
                },
            );
            maps.maps.insert(t0 + Duration::from_hours(k as f64), map);
        }

        assert_eq!(
            file_name("ESBC+MOJN tec", &maps, false),
            "IONEX/ESBCMOJNT_01H.20I"
        );
        assert_eq!(file_name("", &maps, true), "IONEX/OBS2TEC_01H.20I.gz");

        let collector = TecCollector::default();
        let ionex = ionex(&collector, &Obs2TecSetup::default(), &maps).unwrap();

        assert_eq!(ionex.header.number_of_maps, 3);
        assert_eq!(ionex.header.mapf, MappingFunction::CosZ);
        assert!(ionex
            .header
            .comments
            .iter()
            .any(|c| c.contains("UNCALIBRATED")));
        assert!(ionex.header.comments.iter().all(|c| c.len() <= 60));

        // formatted, then parsed back: a standard IONEX
        let path = std::env::temp_dir().join("rinex-cli-obs2tec-test.20I");
        ionex.to_file(&path).unwrap();
        let parsed = IONEX::from_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(parsed.header.number_of_maps, 3);
        assert_eq!(parsed.header.grid.latitude.num_points(), 3);
        assert_eq!(parsed.header.grid.longitude.num_points(), 3);

        let key = Key::from_decimal_degrees_km(t0 + Duration::from_hours(2.0), 55.0, 10.0, 450.0);
        let tec = parsed
            .record
            .get(&key)
            .expect("node (55°N, 10°E) of the third map");
        assert!((tec.tecu() - 12.0).abs() < 0.1);

        let key = Key::from_decimal_degrees_km(t0, 57.5, 5.0, 450.0);
        assert!((parsed.record.get(&key).unwrap().tecu() + 3.2).abs() < 0.1);

        // missing nodes (9999) are not part of the record
        let key = Key::from_decimal_degrees_km(t0, 52.5, 15.0, 450.0);
        assert!(parsed.record.get(&key).is_none());
    }
}
