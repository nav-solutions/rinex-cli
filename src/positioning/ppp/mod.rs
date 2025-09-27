//! PPP solver
use crate::{
    cli::Context,
    positioning::{cast_rtk_carrier, ClockStateProvider, EphemerisSource},
};

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
};

use rinex::{
    carrier::Carrier,
    prelude::{Observable, SV},
};

mod report;
pub use report::Report;

pub mod post_process;

use gnss_rtk::prelude::{
    AbsoluteTime, Bias, Candidate, Epoch, Observation, OrbitSource, PVTSolution, UserProfile, PPP,
};

pub fn resolve<'a, 'b, CK: ClockStateProvider, O: OrbitSource, B: Bias, T: AbsoluteTime>(
    ctx: &Context,
    eph: &'a RefCell<EphemerisSource<'b>>,
    user_profile: UserProfile,
    mut clock: CK,
    mut solver: PPP<O, B, T>,
) -> BTreeMap<Epoch, PVTSolution> {
    let mut past_epoch = Option::<Epoch>::None;

    let mut solutions: BTreeMap<Epoch, PVTSolution> = BTreeMap::new();

    // infaillible, at this point
    let obs_data = ctx.data.observation().unwrap();

    let mut candidates = Vec::<Candidate>::with_capacity(4);
    let mut sv_observations = HashMap::<SV, Vec<Observation>>::new();

    // TODO: RTK
    let mut remote_observations = Vec::<Observation>::new();

    for (t, signal) in obs_data.signal_observations_sampling_ok_iter() {
        let carrier = Carrier::from_observable(signal.sv.constellation, &signal.observable);

        if carrier.is_err() {
            error!(
                "{}({}/{}) - unknown signal {:?}",
                t,
                signal.sv.constellation,
                signal.observable,
                carrier.err().unwrap()
            );
            continue;
        }

        let carrier = carrier.unwrap();

        let rtk_carrier = cast_rtk_carrier(carrier);

        if rtk_carrier.is_err() {
            error!(
                "{}({}/{}) - unknown frequency: {}",
                t,
                signal.sv.constellation,
                signal.observable,
                rtk_carrier.err().unwrap()
            );
            continue;
        }

        let rtk_carrier = rtk_carrier.unwrap();

        if let Some(past_t) = past_epoch {
            if t > past_t {
                // New epoch: solving attempt
                for (sv, observations) in sv_observations.iter() {
                    // Create new candidate
                    let mut cd = Candidate::new(*sv, past_t, observations.clone());

                    // candidate "fixup" or customizations
                    match clock.next_clock_at(past_t, *sv) {
                        Some(dt) => cd.set_clock_correction(dt),
                        None => error!("{} ({}) - no clock correction available", past_t, *sv),
                    }

                    if let Some((_, _, eph)) = eph.borrow_mut().select(past_t, *sv) {
                        if let Some(tgd) = eph.tgd() {
                            debug!("{} ({}) - tgd: {}", past_t, *sv, tgd);
                            cd.set_group_delay(tgd);
                        }
                    }

                    candidates.push(cd);
                }

                match solver.resolve(user_profile, past_t, &candidates) {
                    Ok(pvt) => {
                        info!(
                            "{} : new pvt solution {:?} dt={}",
                            pvt.epoch, pvt.pos_m, pvt.clock_offset_s
                        );
                        solutions.insert(pvt.epoch, pvt);
                    },
                    Err(e) => warn!("{} : pvt solver error \"{}\"", past_t, e),
                }

                candidates.clear();
                sv_observations.clear();
                remote_observations.clear();
            }
        }

        if let Some((_, observations)) = sv_observations
            .iter_mut()
            .filter(|(k, _)| **k == signal.sv)
            .reduce(|k, _| k)
        {
            if let Some(observation) = observations
                .iter_mut()
                .filter(|k| k.carrier == rtk_carrier)
                .reduce(|k, _| k)
            {
                match signal.observable {
                    Observable::PhaseRange(_) => {
                        observation.phase_range_m = Some(signal.value);
                    },
                    Observable::PseudoRange(_) => {
                        observation.pseudo_range_m = Some(signal.value);
                    },
                    Observable::Doppler(_) => {
                        observation.doppler = Some(signal.value);
                    },
                    _ => {},
                }
            } else {
                match signal.observable {
                    Observable::PhaseRange(_) => {
                        observations.push(Observation::ambiguous_phase_range(
                            rtk_carrier,
                            signal.value,
                            None,
                        ));
                    },
                    Observable::PseudoRange(_) => {
                        observations.push(Observation::pseudo_range(
                            rtk_carrier,
                            signal.value,
                            None,
                        ));
                    },
                    Observable::Doppler(_) => {
                        observations.push(Observation::doppler(rtk_carrier, signal.value, None));
                    },
                    _ => {},
                }
            }
        } else {
            match signal.observable {
                Observable::PhaseRange(_) => {
                    sv_observations.insert(
                        signal.sv,
                        vec![Observation::ambiguous_phase_range(
                            rtk_carrier,
                            signal.value,
                            None,
                        )],
                    );
                },
                Observable::PseudoRange(_) => {
                    sv_observations.insert(
                        signal.sv,
                        vec![Observation::pseudo_range(rtk_carrier, signal.value, None)],
                    );
                },
                Observable::Doppler(_) => {
                    sv_observations.insert(
                        signal.sv,
                        vec![Observation::doppler(rtk_carrier, signal.value, None)],
                    );
                },
                _ => {},
            }
        }

        past_epoch = Some(t);
    }
    solutions
}
