mod buffer;
mod coords;
mod orbit;
mod precise;
mod snapshot;
mod time;
// mod clock;
mod biases;
mod ephemeris;
mod ppp; // precise point positioning
mod rtk; // RTK positioning

#[cfg(feature = "cggtts")]
mod cggtts; // CGGTTS special solver

pub use buffer::Buffer;
pub use coords::Coords3d;
pub use snapshot::{CenteredDataPoints, CenteredSnapshot};

use orbit::Orbits;
use precise::PreciseOrbits;
use time::Time;

use biases::{environment::EnvironmentalBiases, space::SpacebornBiases};

use ephemeris::{EphemerisBuffer, NullEphemerisSource};

use ppp::{
    post_process::{post_process as ppp_post_process, Error as PPPPostError},
    Report as PPPReport,
};

use rtk::RTKBaseStation;

#[cfg(feature = "cggtts")]
use cggtts::{post_process as cggtts_post_process, Report as CggttsReport};

use log::error;

use rinex::{carrier::Carrier, prelude::Rinex};

use gnss_qc::prelude::QcExtraPage;

use gnss_rtk::prelude::{
    Carrier as RTKCarrier, ClockProfile, Config, Duration, Error as RTKError, Method, Rc, Solver,
    UserParameters, UserProfile,
};

use thiserror::Error;

use crate::cli::{Cli, Context};
use clap::ArgMatches;
use std::cell::RefCell;
use std::fs::read_to_string;

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
        RTKCarrier::E5b => Carrier::E5b,
        RTKCarrier::L5 => Carrier::L5,
        RTKCarrier::L2 => Carrier::L2,
    }
}

/// Converts [Carrier] to [RTKCarrier]
pub fn cast_rtk_carrier(carrier: Carrier) -> Result<RTKCarrier, RTKError> {
    match carrier {
        Carrier::B1 => Ok(RTKCarrier::B1),
        Carrier::B3 => Ok(RTKCarrier::B3),
        Carrier::E5a5b => Ok(RTKCarrier::E5a5b),
        Carrier::L2 => Ok(RTKCarrier::L2),
        Carrier::L5 | Carrier::E5a => Ok(RTKCarrier::L5),
        Carrier::L1 | Carrier::E1 => Ok(RTKCarrier::L1),
        Carrier::E5b => Ok(RTKCarrier::E5b),
        carrier => {
            error!("{} - signal not supported", carrier);
            Err(RTKError::UnknownCarrierFrequency)
        },
    }
}

pub fn precise_positioning(
    ctx: &Context,
    uses_rtk: bool,
    matches: &ArgMatches,
) -> Result<QcExtraPage, Error> {
    // Load custom configuration script, or Default
    let cfg = match matches.get_one::<String>("cfg") {
        Some(fp) => {
            let content = read_to_string(fp)
                .unwrap_or_else(|e| panic!("failed to read configuration: {}", e));

            let cfg: Config = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("failed to parse configuration: {}", e));

            cfg
        },
        None => {
            let cfg = Config::default();
            cfg
        },
    };

    /* Verify requirements and print helpful comments */
    assert!(
        ctx.data.observation().is_some(),
        "Positioning requires Observation RINEX"
    );

    assert!(
        ctx.data.brdc_navigation().is_some(),
        "Positioning requires Navigation RINEX"
    );

    /*
     * CGGTTS special case
     */
    #[cfg(not(feature = "cggtts"))]
    if matches.get_flag("cggtts") {
        panic!("--cggtts option not available: compile with cggtts option");
    }

    info!("Using custom solver configuration: {:#?}", cfg);

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

    let rtk_obs = if uses_rtk {
        rtk::parse_rinex(&matches)
    } else {
        Rinex::basic_obs()
    };

    // Deploy base station if needed
    let base_station = if uses_rtk {
        Some(RTKBaseStation::new(&rtk_obs))
    } else {
        None
    };

    let time = Time::new(&ctx);

    let ephemeris_buffer = EphemerisBuffer::new(&ctx);
    let ephemeris_buffer = Rc::new(RefCell::new(ephemeris_buffer));

    let env_biases = EnvironmentalBiases::new();
    let orbits = Orbits::new(&ctx, Rc::clone(&ephemeris_buffer));
    let space_biases = SpacebornBiases::new(Rc::clone(&ephemeris_buffer));

    // Ephemeris interface is not used by this application
    let null_eph = NullEphemerisSource::new();

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
        space_biases.into(),
        env_biases.into(),
        time,
        apriori_ecef_m,
    );

    let user_profile = if matches.get_flag("static") {
        UserProfile::Static
    } else if matches.get_flag("car") {
        UserProfile::Car
    } else if matches.get_flag("airplane") {
        UserProfile::Airplane
    } else if matches.get_flag("rocket") {
        UserProfile::Rocket
    } else {
        UserProfile::Pedestrian
    };

    let clock_profile = if matches.get_flag("quartz") {
        ClockProfile::Quartz
    } else if matches.get_flag("atomic") {
        ClockProfile::Atomic
    } else if matches.get_flag("h-maser") {
        ClockProfile::H_MASER
    } else {
        ClockProfile::Oscillator
    };

    info!(
        "deployed with {} profile - clock profile: {:?}",
        user_profile, clock_profile
    );

    let user_params = UserParameters::new(user_profile.clone(), clock_profile.clone());

    // PPP+CGGTTS special case
    #[cfg(feature = "cggtts")]
    if matches.get_flag("cggtts") {
        //* CGGTTS special opmode */
        let tracks = cggtts::resolve(ctx, cfg.method, user_params, solver, ephemeris_buffer)?;

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

    // PPP/RTK
    let solutions = ppp::resolve(ctx, user_params, solver, ephemeris_buffer);

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
