//! Navigation setup: ANISE almanac, navigation configuration
use std::path::Path;

use log::info;

use gnss_qc::prelude::{
    Almanac, ClockProfile, MetaAlmanac, Method, QcIndexing, QcNavigationConfig,
    QcNavigationTechnique, UserParameters, UserProfile,
};

use crate::{
    cli::{
        parse_constellations, parse_coordinates, ClockArg, MethodArg, NavigationArgs, ProfileArg,
    },
    error::Error,
    input::{Inputs, ProductKind},
};

/// Navigation context, presented in the reports
#[derive(Debug, Clone, serde::Serialize)]
pub struct NavigationSetup {
    /// Rover
    pub rover: String,

    /// RTK base
    pub base: Option<String>,

    /// Reference position of the rover (ECEF m), to express the errors
    pub reference_ecef_m: Option<(f64, f64, f64)>,

    /// Configuration summary
    pub description: Vec<(String, String)>,
}

/// Loads the ANISE almanac
pub fn almanac(dir: Option<&Path>) -> Result<Almanac, Error> {
    if let Some(dir) = dir {
        let spk = ["de440s.bsp", "de440.bsp", "de421.bsp"]
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
            .ok_or(Error::Navigation(format!(
                "no planetary ephemeris (de440s.bsp) in {}",
                dir.display()
            )))?;

        let pck = dir.join("pck11.pca");

        info!("ANISE dataset: {}, {}", spk.display(), pck.display());

        return Almanac::new(&spk.to_string_lossy())
            .and_then(|almanac| almanac.load(&pck.to_string_lossy()))
            .map_err(|e| Error::Navigation(format!("ANISE dataset: {}", e)));
    }

    #[cfg(feature = "embed_ephem")]
    {
        info!("ANISE dataset: embedded");
        Almanac::until_2035().map_err(|e| Error::Navigation(format!("ANISE dataset: {}", e)))
    }

    #[cfg(not(feature = "embed_ephem"))]
    {
        info!("ANISE dataset: latest (download on first use, cached afterwards)");
        MetaAlmanac::latest().map_err(|e| {
            Error::Navigation(format!(
                "ANISE dataset: {} (offline? use --anise-dir or the embed_ephem feature)",
                e
            ))
        })
    }
}

fn method(arg: MethodArg) -> Method {
    match arg {
        MethodArg::Spp => Method::SPP,
        MethodArg::Cpp => Method::CPP,
        MethodArg::Ppp => Method::PPP,
    }
}

fn profile(arg: ProfileArg) -> UserProfile {
    match arg {
        ProfileArg::Static => UserProfile::Static,
        ProfileArg::Pedestrian => UserProfile::Pedestrian,
        ProfileArg::Car => UserProfile::Car,
        ProfileArg::Airplane => UserProfile::Airplane,
        ProfileArg::Rocket => UserProfile::Rocket,
    }
}

fn clock(arg: ClockArg) -> ClockProfile {
    match arg {
        ClockArg::Quartz => ClockProfile::Quartz,
        ClockArg::Oscillator => ClockProfile::Oscillator,
        ClockArg::Atomic => ClockProfile::Atomic,
        ClockArg::HydrogenMaser => ClockProfile::HydrogenMaser,
    }
}

/// Builds the navigation configuration: configuration file, then command line options.
/// `base`: RTK base designated by the user, with its possible position.
pub fn configure(
    args: &NavigationArgs,
    base: Option<(&str, Option<&str>)>,
    inputs: &Inputs,
) -> Result<(QcNavigationConfig, NavigationSetup), Error> {
    let mut cfg = match &args.cfg {
        Some(path) => {
            let content = std::fs::read_to_string(path)?;
            serde_json::from_str::<QcNavigationConfig>(&content)?
        },
        None => QcNavigationConfig::default().with_method(Method::CPP),
    };

    if let Some(m) = args.method {
        cfg = cfg.with_method(method(m));
    }

    if !args.constellations.is_empty() {
        cfg = cfg.with_constellations(&parse_constellations(&args.constellations)?);
    }

    if let Some(arg) = args.orbits {
        cfg = cfg.with_orbits(arg.to_preference());
    }

    let user_profile = if args.is_static {
        Some(UserProfile::Static)
    } else {
        args.profile.map(profile)
    };

    let user_description = if user_profile.is_some() || args.clock.is_some() {
        let (user_profile, clock_profile) = (
            user_profile.unwrap_or(UserProfile::Pedestrian),
            args.clock.map(clock).unwrap_or(ClockProfile::Quartz),
        );
        cfg = cfg.with_user_parameters(UserParameters::new(user_profile, clock_profile));
        format!("{:?}, {:?} clock", user_profile, clock_profile)
    } else {
        cfg.user.to_string()
    };

    // RTK base
    let base = match base {
        Some((text, position)) => {
            let identity = inputs.resolve(text, ProductKind::Observation)?;
            let position = position.map(parse_coordinates).transpose()?;
            cfg.technique = QcNavigationTechnique::Rtk {
                base: identity.clone(),
                position_ecef_m: position,
            };
            Some(identity)
        },
        None => match &cfg.technique {
            QcNavigationTechnique::Rtk { base, .. } => Some(base.clone()),
            QcNavigationTechnique::Absolute => None,
        },
    };

    // Rover: designated, configured, or first signal source that is not the base
    let rover = match &args.rover {
        Some(text) => inputs.resolve(text, ProductKind::Observation)?,
        None => match &cfg.rover {
            Some(rover) => rover.clone(),
            None => inputs
                .entries(ProductKind::Observation)
                .map(|entry| entry.identity.clone())
                .find(|identity| Some(identity) != base.as_ref())
                .ok_or(Error::Config(
                    "navigation requires observations (signal source)".to_string(),
                ))?,
        },
    };

    if inputs.entries(ProductKind::Observation).count() > 1 && args.rover.is_none() {
        info!("rover: \"{}\" (select another one with --rover)", rover);
    }

    cfg = cfg.with_rover(rover.clone());

    // A priori position
    if let Some(apriori) = &args.apriori {
        cfg.apriori_ecef_m = if apriori.trim() == "header" {
            Some(
                inputs
                    .reference_position(&rover)
                    .ok_or(Error::Config(format!(
                        "\"{}\" does not describe its position",
                        rover
                    )))?,
            )
        } else {
            Some(parse_coordinates(apriori)?)
        };
    }

    let reference_ecef_m = match &args.reference {
        Some(text) => Some(parse_coordinates(text)?),
        None => inputs.reference_position(&rover).or(cfg.apriori_ecef_m),
    };

    let mut description = vec![
        ("Method".to_string(), format!("{:?}", cfg.solver.method)),
        ("Rover".to_string(), rover.to_string()),
        (
            "Technique".to_string(),
            match &base {
                Some(base) => format!("RTK, base \"{}\"", base),
                None => "Absolute".to_string(),
            },
        ),
        (
            "Constellations".to_string(),
            cfg.constellations
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        ("Orbits".to_string(), format!("{:?}", cfg.orbits)),
        ("User profile".to_string(), user_description),
    ];

    if let Some((x, y, z)) = cfg.apriori_ecef_m {
        description.push((
            "A priori".to_string(),
            format!("ECEF {:.3} {:.3} {:.3} m", x, y, z),
        ));
    }

    Ok((
        cfg,
        NavigationSetup {
            rover: rover.to_string(),
            base: base.as_ref().map(QcIndexing::to_string),
            reference_ecef_m,
            description,
        },
    ))
}
