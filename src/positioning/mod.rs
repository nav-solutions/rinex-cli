use crate::cli::{Cli, Context};
use clap::ArgMatches;
use std::cell::RefCell;
use std::fs::read_to_string;

mod buffer;
pub use buffer::Buffer;

mod snapshot;
pub use snapshot::{CenteredDataPoints, CenteredSnapshot};

mod eph;
use eph::EphemerisSource;

mod precise;
use precise::PreciseOrbits;

mod time;
use time::Time;

mod ppp; // precise point positioning
use ppp::{
    post_process::{post_process as ppp_post_process, Error as PPPPostError},
    Report as PPPReport,
};

#[cfg(feature = "cggtts")]
mod cggtts; // CGGTTS special solver

#[cfg(feature = "cggtts")]
use cggtts::{post_process as cggtts_post_process, Report as CggttsReport};

// mod rtk;
// pub use rtk::RemoteRTKReference;

mod orbit;
use orbit::Orbits;

mod coords;
pub use coords::Coords3d;

mod clock;
use clock::Clock;
pub use clock::ClockStateProvider;

use rinex::{
    carrier::Carrier,
    prelude::{Constellation, Rinex},
};

use gnss_qc::prelude::QcExtraPage;

use gnss_rtk::prelude::{
    BiasRuntime, Carrier as RTKCarrier, ClockProfile, Config, Duration, EnvironmentalBias,
    Ephemeris as RTKEphemerisData, EphemerisSource as RTKEphemeris, Epoch, Error as RTKError,
    KbModel, Method, SatelliteClockCorrection, Solver, SpacebornBias, TroposphereModel,
    UserParameters, UserProfile, SV,
};

use thiserror::Error;

struct EnvironmentalBiases {}

impl EnvironmentalBias for EnvironmentalBiases {
    fn ionosphere_bias_m(&self, _: &BiasRuntime) -> f64 {
        0.0
    }

    fn troposphere_bias_m(&self, rtm: &BiasRuntime) -> f64 {
        TroposphereModel::Niel.bias_m(rtm)
    }
}

struct NullEphemeris {}

impl RTKEphemeris for NullEphemeris {
    fn ephemeris_data(&self, epoch: Epoch, sv: SV) -> Option<RTKEphemerisData> {
        None
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("solver error")]
    SolverError(#[from] RTKError),
    #[error("no solutions: check your settings or input")]
    NoSolutions,
    #[error("i/o error")]
    StdioError(#[from] std::io::Error),
    #[error("post process error")]
    PPPPost(#[from] PPPPostError),
}

/// Converts [RTKCarrier] to [Carrier]
pub fn rtk_carrier_cast(carrier: RTKCarrier) -> Carrier {
    match carrier {
        RTKCarrier::B1 => Carrier::B1,
        RTKCarrier::B3 => Carrier::B3,
        RTKCarrier::E5a5b => Carrier::E5a5b,
        RTKCarrier::L1 => Carrier::L1,
        // RTKCarrier::G1 => Carrier::G1(None),
        // RTKCarrier::G2 => Carrier::G2(None),
        // RTKCarrier::G3 => Carrier::G3,
        RTKCarrier::E5b => Carrier::E5b,
        // RTKCarrier::E6Lex => Carrier::E6,
        // RTKCarrier::G1a => Carrier::G1a,
        // RTKCarrier::G2a => Carrier::G2a,
        RTKCarrier::L5 => Carrier::L5,
        RTKCarrier::L2 => Carrier::L2,
        // RTKCarrier::S => Carrier::S,
    }
}

/// Converts [Carrier] to [RTKCarrier]
pub fn cast_rtk_carrier(carrier: Carrier) -> Result<RTKCarrier, RTKError> {
    let freq_mhz = carrier.frequency_mega_hz();
    RTKCarrier::from_frequency_mega_hz(freq_mhz)
}

// // helper in reference signal determination
// fn rtk_reference_carrier(carrier: RTKCarrier) -> bool {
//     matches!(
//         carrier,
//         RTKCarrier::L1 | RTKCarrier::E1 | RTKCarrier::B1c | RTKCarrier::B1i
//     )
// }

//use map_3d::{ecef2geodetic, rad2deg, Ellipsoid};

//pub fn tropo_components(meteo: Option<&Rinex>, t: Epoch, lat_ddeg: f64) -> Option<(f64, f64)> {
//    const MAX_LATDDEG_DELTA: f64 = 15.0;
//    let max_dt = Duration::from_hours(24.0);
//    let rnx = meteo?;
//    let meteo = rnx.header.meteo.as_ref().unwrap();
//
//    let delays: Vec<(Observable, f64)> = meteo
//        .sensors
//        .iter()
//        .filter_map(|s| match s.observable {
//            Observable::ZenithDryDelay => {
//                let (x, y, z, _) = s.position?;
//                let (lat, _, _) = ecef2geodetic(x, y, z, Ellipsoid::WGS84);
//                let lat = rad2deg(lat);
//                if (lat - lat_ddeg).abs() < MAX_LATDDEG_DELTA {
//                    let value = rnx
//                        .zenith_dry_delay()
//                        .filter(|(t_sens, _)| (*t_sens - t).abs() < max_dt)
//                        .min_by_key(|(t_sens, _)| (*t_sens - t).abs());
//                    let (_, value) = value?;
//                    debug!("{:?} lat={} zdd {}", t, lat_ddeg, value);
//                    Some((s.observable.clone(), value))
//                } else {
//                    None
//                }
//            },
//            Observable::ZenithWetDelay => {
//                let (x, y, z, _) = s.position?;
//                let (mut lat, _, _) = ecef2geodetic(x, y, z, Ellipsoid::WGS84);
//                lat = rad2deg(lat);
//                if (lat - lat_ddeg).abs() < MAX_LATDDEG_DELTA {
//                    let value = rnx
//                        .zenith_wet_delay()
//                        .filter(|(t_sens, _)| (*t_sens - t).abs() < max_dt)
//                        .min_by_key(|(t_sens, _)| (*t_sens - t).abs());
//                    let (_, value) = value?;
//                    debug!("{:?} lat={} zdd {}", t, lat_ddeg, value);
//                    Some((s.observable.clone(), value))
//                } else {
//                    None
//                }
//            },
//            _ => None,
//        })
//        .collect();
//
//    if delays.len() < 2 {
//        None
//    } else {
//        let zdd = delays
//            .iter()
//            .filter_map(|(obs, value)| {
//                if obs == &Observable::ZenithDryDelay {
//                    Some(*value)
//                } else {
//                    None
//                }
//            })
//            .reduce(|k, _| k)
//            .unwrap();
//
//        let zwd = delays
//            .iter()
//            .filter_map(|(obs, value)| {
//                if obs == &Observable::ZenithWetDelay {
//                    Some(*value)
//                } else {
//                    None
//                }
//            })
//            .reduce(|k, _| k)
//            .unwrap();
//
//        Some((zwd, zdd))
//    }
//}

/// Returns a [KbModel]
pub fn kb_model(nav: &Rinex, t: Epoch) -> Option<KbModel> {
    let (nav_key, model) = nav
        .nav_klobuchar_models_iter()
        .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;

    Some(KbModel {
        h_km: {
            match nav_key.sv.constellation {
                Constellation::BeiDou => 375.0,
                // we only expect GPS or BDS here,
                // badly formed RINEX will generate errors in the solutions
                _ => 350.0,
            }
        },
        alpha: model.alpha,
        beta: model.beta,
    })
}

// /*
//  * Grabs nearest BD model (in time)
//  */
// pub fn bd_model(nav: &Rinex, t: Epoch) -> Option<BdModel> {
//     let (_, model) = nav
//         .nav_bdgim_models_iter()
//         .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;

//     Some(BdModel { alpha: model.alpha })
// }

// /*
//  * Grabs nearest NG model (in time)
//  */
// pub fn ng_model(nav: &Rinex, t: Epoch) -> Option<NgModel> {
//     let (_, model) = nav
//         .nav_nequickg_models_iter()
//         .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;

//     Some(NgModel { a: model.a })
// }

pub fn precise_positioning(
    _cli: &Cli,
    ctx: &Context,
    is_rtk: bool,
    matches: &ArgMatches,
) -> Result<QcExtraPage, Error> {
    // Load custom configuration script, or Default
    let cfg = match matches.get_one::<String>("cfg") {
        Some(fp) => {
            let content = read_to_string(fp)
                .unwrap_or_else(|e| panic!("failed to read configuration: {}", e));

            let cfg: Config = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("failed to parse configuration: {}", e));

            /*
             * CGGTTS special case
             */
            #[cfg(not(feature = "cggtts"))]
            if matches.get_flag("cggtts") {
                panic!("--cggtts option not available: compile with cggtts option");
            }

            info!("Using custom solver configuration: {:#?}", cfg);
            cfg
        },
        None => {
            let method = Method::default();

            let cfg = Config::default().with_navigation_method(method);

            /*
             * CGGTTS special case
             */
            #[cfg(not(feature = "cggtts"))]
            if matches.get_flag("cggtts") {
                panic!("--cggtts option not available: compile with cggtts option");
            }

            info!("Using {:?} default preset: {:#?}", method, cfg);
            cfg
        },
    };

    /* Verify requirements and print helpful comments */
    assert!(
        ctx.data.observation().is_some(),
        "Positioning requires Observation RINEX"
    );

    if !is_rtk {
        assert!(
            ctx.data.brdc_navigation().is_some(),
            "Positioning requires Navigation RINEX"
        );
    }

    if let Some(obs_rinex) = ctx.data.observation() {
        if let Some(obs_header) = &obs_rinex.header.obs {
            if let Some(time_of_first_obs) = obs_header.timeof_first_obs {
                if let Some(clk_rinex) = ctx.data.clock() {
                    if let Some(clk_header) = &clk_rinex.header.clock {
                        if let Some(time_scale) = clk_header.timescale {
                            if time_scale == time_of_first_obs.time_scale {
                                info!("Temporal PPP compliancy");
                            } else {
                                error!("Working with different timescales in OBS/CLK RINEX is not PPP compatible and will generate tiny errors");
                                warn!("Consider using OBS/CLK RINEX files expressed in the same timescale for optimal results");
                            }
                        }
                    }
                } else if let Some(sp3) = ctx.data.sp3() {
                    if ctx.data.sp3_has_clock() {
                        if sp3.header.timescale == time_of_first_obs.time_scale {
                            info!("Temporal PPP compliancy");
                        } else {
                            error!("Working with different timescales in OBS/SP3 is not PPP compatible and will generate tiny errors");
                            if sp3.header.sampling_period >= Duration::from_seconds(300.0) {
                                warn!("Interpolating clock states from low sample rate SP3 will most likely introduce errors");
                            }
                        }
                    }
                }
            }
        }
    }

    // print config to be used
    info!("Using {:?} method", cfg.method);

    // create data providers
    let eph = RefCell::new(EphemerisSource::from_ctx(ctx));

    let clocks = Clock::new(&ctx, &eph);
    let time = Time::new(&ctx);
    let orbits = Orbits::new(&ctx, &eph);

    // let mut rtk_reference = RemoteRTKReference::from_ctx(&ctx);

    // reference point is mandatory to CGGTTS opmode
    #[cfg(feature = "cggtts")]
    if matches.get_flag("cggtts") {
        if ctx.rx_orbit.is_none() {
            panic!(
                "cggtts needs a reference point (x0, y0, z0).
If your dataset does not describe one, you can manually describe one, see --help."
            );
        }
    }

    let null_eph = NullEphemeris {};
    let env_biases = EnvironmentalBiases {};

    let apriori = ctx.rx_orbit;

    let apriori_ecef_m = match apriori {
        Some(apriori) => {
            let pos_vel = apriori.to_cartesian_pos_vel() * 1.0E3;
            Some((pos_vel[0], pos_vel[1], pos_vel[2]))
        },
        None => None,
    };

    let solver = Solver::new(
        ctx.data.almanac.clone(),
        ctx.data.earth_cef,
        cfg.clone(),
        null_eph.into(),
        orbits.into(),
        spaceborn_biases.into(),
        env_biases.into(),
        time,
        apriori_ecef_m,
    );

    let user_profile = if matches.get_flag("static") {
        UserProfile::Static
    } else {
        UserProfile::Pedestrian
    };

    let clock_profile = ClockProfile::Oscillator;

    let params = UserParameters::new(user_profile.clone(), clock_profile.clone());

    #[cfg(feature = "cggtts")]
    if matches.get_flag("cggtts") {
        //* CGGTTS special opmode */
        let tracks = cggtts::resolve(ctx, &eph, params, clocks, solver, cfg.method)?;
        if !tracks.is_empty() {
            cggtts_post_process(&ctx, &tracks, matches)?;
            let report = CggttsReport::new(&ctx, &tracks);
            return Ok(report.formalize());
        } else {
            error!("solver did not generate a single solution");
            error!("verify your input data and configuration setup");
            return Err(Error::NoSolutions);
        }
    }

    /* PPP */
    let solutions = ppp::resolve(ctx, &eph, params, clocks, solver);
    if !solutions.is_empty() {
        ppp_post_process(&ctx, &solutions, matches)?;
        let report = PPPReport::new(&cfg, &ctx, user_profile, clock_profile, &solutions);
        Ok(report.formalize())
    } else {
        error!("solver did not generate a single solution");
        error!("verify your input data and configuration setup");
        Err(Error::NoSolutions)
    }
}
