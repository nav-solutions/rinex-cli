use log::info;

use crate::{cli::Context, positioning::cast_rtk_carrier};

use rinex::prelude::{
    obs::{EpochFlag, SignalObservation},
    Carrier, Epoch, Observable, Rinex, RinexType,
};

use gnss_rtk::prelude::{Candidate, RTKBase};

use std::collections::{BTreeMap, HashMap};

pub struct RTKBaseStation<'a> {
    /// Name of this station
    name: String,

    /// Reference position is mandatory
    reference_position_ecef_m: (f64, f64, f64),

    /// End of observations stream
    eos: bool,

    /// Iterator
    iter: Box<dyn Iterator<Item = (Epoch, &'a SignalObservation)> + 'a>,

    /// Buffer
    buffer: Vec<(Epoch, SignalObservation)>,
}

impl<'a> RTKBase for RTKBaseStation<'a> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn new_epoch(&mut self, epoch: Epoch) {
        // consume till EoS or new epoch
        loop {
            if self.eos {
                break;
            }

            if let Some((epoch, observation)) = self.iter.next() {
                self.buffer.push((epoch, observation.clone()));

                if epoch > epoch {
                    break;
                }
            } else {
                self.eos = true;
            }
        }
    }

    fn observe(&self, epoch: Epoch) -> Vec<Candidate> {
        vec![]
    }

    fn reference_position_ecef_m(&self, _: Epoch) -> (f64, f64, f64) {
        self.reference_position_ecef_m
    }
}

impl<'a> RTKBaseStation<'a> {
    pub fn new(rinex: &'a Rinex) -> Self {
        assert_eq!(
            rinex.header.rinex_type,
            RinexType::ObservationData,
            "base station file must be Observation RINEX!"
        );

        let reference_position_ecef_m = rinex
            .header
            .rx_position
            .as_ref()
            .expect("base station coordinates must be descrined in the RINEX header!");

        let s = Self {
            eos: false,
            name: "Base".to_string(),
            buffer: Vec::with_capacity(8),
            iter: rinex.signal_observations_sampling_ok_iter(),
            reference_position_ecef_m: *reference_position_ecef_m,
        };

        info!("{} - rtk base station deployed", s.name());
        s
    }
}
