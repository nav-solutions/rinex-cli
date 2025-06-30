//! CGGTTS special resolution opmode.
use std::{cell::RefCell, collections::HashMap};

mod post_process;
pub use post_process::post_process;

mod report;
pub use report::Report;

use rinex::{
    carrier::Carrier,
    prelude::{Constellation, Observable, SV},
};

use gnss_rtk::prelude::{
    AbsoluteTime, Candidate, Carrier as RTKCarrier, ClockProfile, EnvironmentalBias,
    EphemerisSource as RTKEphemerisSource, Method, Observation, OrbitSource, Solver, SpacebornBias,
    UserParameters, UserProfile, SPEED_OF_LIGHT_M_S,
};

use cggtts::prelude::{
    CommonViewCalendar, CommonViewClass, Observation as FitObservation, SVTracker, Track,
};

use crate::{
    cli::Context,
    positioning::{
        cast_rtk_carrier, ClockStateProvider, EphemerisSource, Error as PositioningError,
    },
};

fn rinex_ref_observable(
    ppp: bool,
    constellation: Constellation,
    observations: &[Observation],
) -> String {
    let l1_observation = observations
        .iter()
        .filter(|obs: &&Observation| obs.carrier == RTKCarrier::L1)
        .reduce(|k, _| k);

    let observable = if let Some(l1_observation) = l1_observation {
        if ppp {
            Observable::from_phase_range_frequency_mega_hz(
                constellation,
                l1_observation.carrier.frequency_mega_hz(),
            )
        } else {
            Observable::from_pseudo_range_frequency_mega_hz(
                constellation,
                l1_observation.carrier.frequency_mega_hz(),
            )
        }
    } else {
        let carrier = observations
            .iter()
            .map(|obs| obs.carrier)
            .reduce(|k, _| k)
            .expect("internal error: no carrier signals found");

        if ppp {
            Observable::from_phase_range_frequency_mega_hz(
                constellation,
                carrier.frequency_mega_hz(),
            )
        } else {
            Observable::from_pseudo_range_frequency_mega_hz(
                constellation,
                carrier.frequency_mega_hz(),
            )
        }
    };

    if let Ok(observable) = observable {
        observable.to_string()
    } else {
        if ppp {
            "L1C".to_string()
        } else {
            "C1C".to_string()
        }
    }
}

/// Resolves CGGTTS tracks from input context
pub fn resolve<
    'a,
    'b,
    CK: ClockStateProvider,
    EPH: RTKEphemerisSource,
    ORB: OrbitSource,
    SB: SpacebornBias,
    EB: EnvironmentalBias,
    TIM: AbsoluteTime,
>(
    ctx: &Context,
    eph: &'a RefCell<EphemerisSource<'b>>,
    params: UserParameters,
    mut clock: CK,
    mut solver: Solver<EPH, ORB, EB, SB, TIM>,
    method: Method,
) -> Result<Vec<Track>, PositioningError> {
    let obs_data = ctx
        .data
        .observation()
        .expect("RNX2CGGTTS requires OBS RINEX");

    let t0 = obs_data
        .first_epoch()
        .expect("failed to determine first epoch, empty observations?");

    let sampling_period = obs_data
        .dominant_sampling_interval()
        .expect("RNX2CGGTTS requires steady GNSS observations");

    // scheduling
    let mut past_t = t0;

    // TODO: allow customizations
    let cv_calendar = CommonViewCalendar::bipm();

    let mut collecting = false;
    let mut next_period_start = cv_calendar.next_period_start_after(t0);
    let mut next_collection_start = cv_calendar.next_data_collection_after(t0);

    let mut candidates = Vec::<Candidate>::with_capacity(4);

    let mut tracks = Vec::<Track>::new();
    let mut trackers = HashMap::<(SV, String), SVTracker>::with_capacity(16);
    let mut sv_observations = HashMap::<SV, Vec<Observation>>::new();
    let mut sv_ref_observables = HashMap::<SV, String>::new();

    info!(
        "{} - CGGTTS mode deployed - {} until next tracking",
        t0,
        next_collection_start - t0
    );

    let mut release = false;

    for (index, (t, signal)) in obs_data.signal_observations_sampling_ok_iter().enumerate() {
        if index > 0 && t > past_t {
            if collecting {
                info!("{} - new epoch", past_t);
                for (sv, observations) in sv_observations.iter() {
                    // create new candidate
                    let mut cd = Candidate::new(*sv, past_t, observations.clone());

                    // fixup and customizations
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

                // makes sure we have a reference observable
                for (sv, observations) in sv_observations.iter() {
                    if sv_ref_observables.get(&sv).is_none() {
                        let ref_observable = rinex_ref_observable(
                            method == Method::PPP,
                            sv.constellation,
                            observations,
                        );
                        sv_ref_observables.insert(*sv, ref_observable);
                    }
                }

                match solver.ppp_solving(past_t, params, &candidates) {
                    Ok(pvt) => {
                        for sv_contrib in pvt.sv.iter() {
                            let (azim_deg, elev_deg) =
                                (sv_contrib.azimuth_deg, sv_contrib.elevation_deg);

                            let refsys = pvt.clock_offset_s;

                            let refsv = refsys
                                + sv_contrib.clock_correction.unwrap_or_default().to_seconds();

                            // tracker
                            info!(
                                "{} ({}) : new pvt solution (elev={:.2}°, azim={:.2}°, refsv={:.3E}, refsys={:.3E})",
                                past_t, signal.sv, elev_deg, azim_deg, refsv, refsys,
                            );

                            // tropod model
                            let mdtr =
                                sv_contrib.tropo_bias.unwrap_or_default() / SPEED_OF_LIGHT_M_S;

                            // ionod
                            let mdio = 0.0;
                            // match pvt_data.iono_bias {
                            //     Some(IonosphereBias::Modeled(bias)) => Some(bias),
                            //     _ => None,
                            // };

                            let msio = None;
                            // match pvt_data.iono_bias {
                            //     Some(IonosphereBias::Measured(bias)) => Some(bias),
                            //     _ => None,
                            // };

                            // tracking
                            let data = FitObservation {
                                refsv,
                                refsys,
                                mdtr,
                                msio,
                                mdio,
                                epoch: past_t,
                                azimuth: azim_deg,
                                elevation: elev_deg,
                            };

                            let ref_observable =
                                sv_ref_observables.get(&sv_contrib.sv).unwrap_or_else(|| {
                                    panic!(
                                        "internal error: no reference observable found for {}!",
                                        sv_contrib.sv
                                    )
                                });

                            if let Some(tracker) =
                                trackers.get_mut(&(sv_contrib.sv, ref_observable.clone()))
                            {
                                tracker.new_observation(data);
                            } else {
                                let mut tracker = SVTracker::new(sv_contrib.sv)
                                    .with_gap_tolerance(sampling_period);

                                tracker.new_observation(data);
                                trackers.insert((sv_contrib.sv, ref_observable.clone()), tracker);
                            }
                        }
                    },
                    Err(e) => {
                        // any PVT solution failure will introduce a gap in the track fitter
                        error!("{} - solver error: {}", past_t, e);
                    },
                }

                candidates.clear();
                sv_observations.clear();
            } // collecting
        } // new epoch

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

        // update only on new epochs
        if index > 0 && t > past_t {
            if collecting {
                if t > next_period_start {
                    release = true;
                    // end of period: release attempt
                    for ((sv, sv_ref), tracker) in trackers.iter_mut() {
                        match tracker.fit() {
                            Ok(fitted) => {
                                debug!(
                                    "{}({}) - new CGGTTS fit - azim={:.3}° - elev={:.3}° - refsv={:.5E}s srsv={:.5E}s/s",
                                    past_t,
                                    sv,
                                    fitted.azimuth_deg,
                                    fitted.elevation_deg,
                                    fitted.refsv_s,
                                    fitted.srsv_s_s,
                                );

                                let data = 0; // TODO
                                let class = CommonViewClass::SingleChannel; // TODO
                                let new_track = fitted.to_track(class, data, &sv_ref);

                                tracks.push(new_track);
                            },
                            Err(e) => {
                                error!("{}({}) - CGGTTS fit error: {}", past_t, sv, e);
                            },
                        }
                    }
                } else {
                    info!("{} - {} until CGGTTS release", t, next_period_start - t);
                }
            } else {
                // not collecting
                if t >= next_collection_start {
                    next_period_start = cv_calendar.next_period_start_after(t);
                    collecting = true;
                    debug!("{} - CGGTTS tracking started", t);
                    info!("{} - {} until CGGTTS release", t, next_period_start - t);
                }

                if !collecting {
                    debug!("{} - {} until next tracking", t, next_collection_start - t);
                }
            }
        }

        if release {
            // reset
            release = false;
            next_period_start = cv_calendar.next_period_start_after(t);
            next_collection_start = cv_calendar.next_data_collection_after(past_t);
            collecting = t > next_collection_start;

            if collecting {
                debug!("{} - CGGTTS tracking started", t);
            } else {
                debug!("{} - {} until next tracking", t, next_collection_start - t);
            }

            info!("{} - {} until CGGTTS release", t, next_period_start - t);
        }

        past_t = t;
    }

    Ok(tracks)
}
