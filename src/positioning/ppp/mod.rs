//! PPP solver
use crate::{
    cli::Context,
    positioning::{cast_rtk_carrier, EphemerisBuffer},
};

use std::{
    cell::{RefCell, RefMut},
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
    AbsoluteTime, Candidate, ClockProfile, EnvironmentalBias, EphemerisSource, Epoch, Observation,
    OrbitSource, PVTSolution, Solver, SpacebornBias, UserParameters, UserProfile,
};

pub fn resolve<
    'a,
    'b,
    EPH: EphemerisSource,
    ORB: OrbitSource,
    EB: EnvironmentalBias,
    SB: SpacebornBias,
    TIM: AbsoluteTime,
>(
    ctx: &Context,
    user_params: UserParameters,
    mut solver: Solver<EPH, ORB, EB, SB, TIM>,
    ephemeris_buffer: &mut RefMut<EphemerisBuffer<'a>>,
) -> BTreeMap<Epoch, PVTSolution> {
    let mut past_epoch = Option::<Epoch>::None;

    let mut solutions: BTreeMap<Epoch, PVTSolution> = BTreeMap::new();

    // infaillible, at this point
    let obs_data = ctx.data.observation().unwrap();

    let mut candidates = Vec::<Candidate>::with_capacity(4);
    let mut sv_observations = HashMap::<SV, Vec<Observation>>::new();

    for (epoch, signal) in obs_data.signal_observations_sampling_ok_iter() {
        ephemeris_buffer.new_epoch(epoch);

        let carrier = Carrier::from_observable(signal.sv.constellation, &signal.observable);

        if carrier.is_err() {
            error!(
                "{}({}/{}) - unknown signal {:?}",
                epoch,
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
                epoch,
                signal.sv.constellation,
                signal.observable,
                rtk_carrier.err().unwrap()
            );

            continue;
        }

        let rtk_carrier = rtk_carrier.unwrap();

        if let Some(past_t) = past_epoch {
            if epoch > past_t {
                // New epoch: solving attempt
                for (sv, observations) in sv_observations.iter() {
                    // Create new candidate
                    let mut cd = Candidate::new(*sv, past_t, observations.clone());

                    // // candidate "fixup" or customizations
                    // match clock.next_clock_at(past_t, *sv) {
                    //     Some(dt) => cd.set_clock_correction(dt),
                    //     None => error!("{} ({}) - no clock correction available", past_t, *sv),
                    // }

                    // if let Some((_, _, eph)) = eph.borrow_mut().select(past_t, *sv) {
                    //     if let Some(tgd) = eph.tgd() {
                    //         debug!("{} ({}) - tgd: {}", past_t, *sv, tgd);
                    //         cd.set_group_delay(tgd);
                    //     }
                    // }

                    candidates.push(cd);
                }

                match solver.ppp(past_t, user_params, &candidates) {
                    Ok(pvt) => {
                        info!(
                            "{} : new {:?} solution {:?} dt={}",
                            pvt.epoch, pvt.solution_type, pvt.pos_m, pvt.clock_offset_s
                        );

                        solutions.insert(pvt.epoch, pvt);
                    },
                    Err(e) => warn!("{} : solver error \"{}\"", past_t, e),
                }

                candidates.clear();
                sv_observations.clear();
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
        past_epoch = Some(epoch);
    }
    solutions
}
