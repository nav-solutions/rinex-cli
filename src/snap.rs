//! Epoch snapping: aligns the epochs of an observation RINEX on the integer second.
//!
//! Low cost receivers often time tag their observations in their own (free running) time
//! base: 16:25:42.999, 16:25:43.004.. while geodetic stations sample on the second
//! (16:25:43.000). Two receivers are only presented together, epoch by epoch, when their
//! epochs are identical: RTK requires the rover and the base to be aligned.
//!
//! Moving the time tag of an epoch by Δt is equivalent to a receiver clock offset change
//! of Δt (a "clock jump"). The observations remain consistent if they are corrected
//! accordingly:
//!
//! - pseudo ranges: C' = C + c·Δt
//! - phase ranges:  L' = L + f·Δt (cycles)
//! - receiver clock offset (if any): dt' = dt + Δt
//!
//! Doppler shifts and signal strengths are not affected. The geometry is not modified:
//! the transmission time (t_rx - C/c) remains the same, the receiver clock estimate absorbs Δt,
//! and the double differences (RTK) cancel it.
use std::collections::HashMap;

use log::warn;

use gnss_qc::prelude::{Constellation, Duration, SV};
use rinex::prelude::{
    obs::{Carrier, ObsKey},
    Observable, Rinex,
};

/// Speed of light (m/s)
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Outcome of the epoch snapping
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SnapReport {
    /// Epochs that were moved
    pub shifted: usize,
    /// Largest time tag correction (s)
    pub max_shift_s: f64,
    /// Epochs dropped: another epoch was snapped to the same second
    pub collisions: usize,
}

/// Carrier frequency (Hz) of a phase observable
fn frequency_hz(sv: SV, observable: &Observable, glo_channels: &HashMap<SV, i8>) -> Option<f64> {
    let code = observable.to_string();

    if sv.constellation == Constellation::Glonass {
        // FDMA signals: the frequency depends on the channel number
        let k = *glo_channels.get(&sv)? as f64;
        return match code.chars().nth(1) {
            Some('1') => Some(1602.0E6 + k * 0.5625E6),
            Some('2') => Some(1246.0E6 + k * 0.4375E6),
            Some('3') => Some(1202.025E6),
            _ => None,
        };
    }

    Carrier::from_observable(sv.constellation, observable)
        .ok()
        .map(|carrier| carrier.frequency())
}

/// Snaps the epochs of this observation RINEX to the nearest integer second,
/// correcting the observations (see the module documentation).
/// Other RINEX types are not modified.
pub fn snap_epochs(rinex: &mut Rinex) -> SnapReport {
    let mut report = SnapReport::default();
    let glo_channels = rinex.header.glo_channels.clone();

    let Some(record) = rinex.record.as_mut_obs() else {
        return report;
    };

    let mut unknown_frequency = false;
    let epochs = std::mem::take(record);

    for (key, mut observations) in epochs {
        let snapped = key.epoch.round(Duration::from_seconds(1.0));
        let dt = (snapped - key.epoch).to_seconds();

        if dt != 0.0 {
            report.shifted += 1;
            report.max_shift_s = report.max_shift_s.max(dt.abs());

            observations.signals.retain_mut(|signal| {
                if signal.observable.is_pseudo_range_observable() {
                    signal.value += SPEED_OF_LIGHT_M_S * dt;
                } else if signal.observable.is_phase_range_observable() {
                    match frequency_hz(signal.sv, &signal.observable, &glo_channels) {
                        Some(f) => signal.value += f * dt,
                        None => {
                            // cannot be corrected: dropped
                            unknown_frequency = true;
                            return false;
                        },
                    }
                }
                true
            });

            if let Some(clock) = &mut observations.clock {
                clock.offset_s += dt;
            }
        }

        let key = ObsKey {
            epoch: snapped,
            flag: key.flag,
        };

        if record.contains_key(&key) {
            report.collisions += 1;
            continue;
        }

        record.insert(key, observations);
    }

    if unknown_frequency {
        warn!("epoch snapping: phase ranges of unknown frequency (Glonass channel?) dropped");
    }

    report
}

/// Keeps the epochs that are integer multiples of `period` (counted from the GPS
/// time origin, hence aligned on midnight for any period dividing a day). Unlike
/// a decimation, the epochs that are retained do not depend on the first epoch:
/// the rover and the base retain the same instants. Returns the number of epochs dropped.
pub fn align_sampling(rinex: &mut Rinex, period: Duration) -> usize {
    let Some(record) = rinex.record.as_mut_obs() else {
        return 0;
    };
    let total = record.len();
    record.retain(|key, _| key.epoch.round(period) == key.epoch);
    total - record.len()
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn epoch_alignment() {
        use gnss_qc::prelude::Epoch;
        let period = Duration::from_seconds(30.0);
        for (t, aligned) in [
            ("2024-05-06T16:26:00 GPST", true),
            ("2024-05-06T16:26:30 GPST", true),
            ("2024-05-06T16:26:13 GPST", false),
            ("2024-05-06T16:25:59.999 GPST", false),
        ] {
            let t = Epoch::from_str(t).unwrap();
            assert_eq!(t.round(period) == t, aligned, "{}", t);
        }
    }

    #[test]
    fn frequencies() {
        let gps = SV::from_str("G01").unwrap();
        let l1 = Observable::from_str("L1C").unwrap();
        let f = frequency_hz(gps, &l1, &HashMap::new()).unwrap();
        assert_eq!(f, 1575.42E6);

        let glo = SV::from_str("R09").unwrap();
        let mut channels = HashMap::new();
        channels.insert(glo, -2);
        let g1 = Observable::from_str("L1C").unwrap();
        assert_eq!(
            frequency_hz(glo, &g1, &channels).unwrap(),
            1602.0E6 - 2.0 * 0.5625E6
        );
        assert!(frequency_hz(glo, &g1, &HashMap::new()).is_none());
    }
}
