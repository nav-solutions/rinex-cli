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

    /// Position of the RTK base (ECEF m): configuration, otherwise its RINEX header
    pub base_ecef_m: Option<(f64, f64, f64)>,

    /// Reference position of the rover (ECEF m), to express the errors
    pub reference_ecef_m: Option<(f64, f64, f64)>,

    /// Moving rover (user profile other than static): its RINEX header position is not a
    /// reference, the solutions are presented as displacements
    pub moving: bool,

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

/// Merges `patch` into `target`: objects are merged field by field,
/// any other value (including arrays) is replaced.
fn merge_json(target: &mut serde_json::Value, patch: serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        target.insert(key, value);
                    },
                }
            }
        },
        (target, patch) => *target = patch,
    }
}

/// Parses a (possibly partial) navigation configuration: the fields that are
/// not described in `content` keep the value of `defaults`.
fn partial_configuration(
    defaults: &QcNavigationConfig,
    content: &str,
) -> Result<QcNavigationConfig, Error> {
    let mut value = serde_json::to_value(defaults)?;
    let patch: serde_json::Value = serde_json::from_str(content)?;

    if let serde_json::Value::Object(fields) = &patch {
        let defaults = value
            .as_object()
            .map(|obj| obj.keys().cloned().collect::<Vec<_>>());
        if let Some(known) = defaults {
            for key in fields.keys() {
                if !known.contains(key) {
                    log::warn!(
                        "navigation configuration: unknown field \"{}\" (expected one of: {})",
                        key,
                        known.join(", ")
                    );
                }
            }
        }
    }

    merge_json(&mut value, patch);
    Ok(serde_json::from_value(value)?)
}

/// Builds the navigation configuration: configuration file, then command line options.
/// `base`: RTK base designated by the user, with its possible position.
pub fn configure(
    args: &NavigationArgs,
    base: Option<(&str, Option<&str>)>,
    inputs: &Inputs,
) -> Result<(QcNavigationConfig, NavigationSetup), Error> {
    let mut cfg = QcNavigationConfig::default().with_method(Method::CPP);

    if let Some(path) = &args.cfg {
        let content = std::fs::read_to_string(path)?;
        cfg = partial_configuration(&cfg, &content)?;
    }

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
    let mut base_ecef_m = None;

    let base = match base {
        Some((text, position)) => {
            let identity = inputs.resolve(text, ProductKind::Observation)?;
            let position = position.map(parse_coordinates).transpose()?;
            base_ecef_m = position.or(inputs.reference_position(&identity));
            cfg.technique = QcNavigationTechnique::Rtk {
                base: identity.clone(),
                position_ecef_m: position,
            };
            Some(identity)
        },
        None => match &cfg.technique {
            QcNavigationTechnique::Rtk {
                base,
                position_ecef_m,
            } => {
                base_ecef_m = position_ecef_m.or(inputs.reference_position(base));
                Some(base.clone())
            },
            QcNavigationTechnique::Absolute => None,
        },
    };

    // RTK + dual frequency: GNSS-RTK elects a single pivot satellite (all constellations)
    // and only differences measurements on the same carriers. The satellites of the other
    // constellations (L2 versus E5a..) cannot be differenced and are dropped.
    if base.is_some() && cfg.solver.method != Method::SPP && cfg.constellations.len() > 1 {
        log::warn!(
            "RTK-{:?} with {} constellations: dual frequency double differences are only formed \
             within the constellation of the pivot satellite, the other satellites are dropped. \
             Prefer a single constellation (for example -c gps)",
            cfg.solver.method,
            cfg.constellations.len(),
        );
    }

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

    // a moving rover has no reference position, unless designated
    let moving = !args.is_static && args.profile.is_some_and(|p| p != ProfileArg::Static);

    let reference_ecef_m = match &args.reference {
        Some(text) => Some(parse_coordinates(text)?),
        None if moving => None,
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

    if let (Some(rover_ecef_m), Some(base_ecef_m)) = (reference_ecef_m, base_ecef_m) {
        let length_m = ((rover_ecef_m.0 - base_ecef_m.0).powi(2)
            + (rover_ecef_m.1 - base_ecef_m.1).powi(2)
            + (rover_ecef_m.2 - base_ecef_m.2).powi(2))
        .sqrt();
        description.push((
            "Baseline".to_string(),
            format!("{:.3} km (reference positions)", length_m / 1.0E3),
        ));
    }

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
            base_ecef_m,
            reference_ecef_m,
            moving,
            description,
        },
    ))
}

/// Signal combinations of the navigated signal sources: rover (and RTK base),
/// constellations of the navigation.
pub fn combinations(cfg: &QcNavigationConfig) -> gnss_qc::prelude::QcCombinationConfig {
    let mut combinations = gnss_qc::prelude::QcCombinationConfig::default();

    if let Some(rover) = &cfg.rover {
        combinations.receivers.push(rover.clone());
    }

    if let gnss_qc::prelude::QcNavigationTechnique::Rtk { base, .. } = &cfg.technique {
        combinations.receivers.push(base.clone());
    }

    combinations.constellations = cfg.constellations.clone();
    combinations
}

#[cfg(test)]
mod test {
    use super::*;
    use gnss_qc::prelude::TimeScale;

    #[test]
    fn partial_configurations() {
        let defaults = QcNavigationConfig::default().with_method(Method::CPP);

        // empty configuration: defaults
        let cfg = partial_configuration(&defaults, "{}").unwrap();
        assert_eq!(cfg.technique, QcNavigationTechnique::Absolute);
        assert_eq!(cfg.solver.method, Method::CPP);

        // nested field only: the rest of the solver configuration is preserved
        let cfg = partial_configuration(
            &defaults,
            r#"{ "solver": { "method": "PPP", "timescale": "GST" } }"#,
        )
        .unwrap();
        assert_eq!(cfg.solver.method, Method::PPP);
        assert_eq!(cfg.solver.timescale, TimeScale::GST);
        assert_eq!(cfg.solver.min_sv_elev, defaults.solver.min_sv_elev);
        assert_eq!(cfg.constellations, defaults.constellations);

        // RTK technique
        let cfg = partial_configuration(
            &defaults,
            r#"{ "technique": { "Rtk": { "base": { "Custom": "MOJN" }, "position_ecef_m": null } } }"#,
        )
        .unwrap();
        assert!(matches!(cfg.technique, QcNavigationTechnique::Rtk { .. }));
    }

    #[test]
    fn example_configurations() {
        let defaults = QcNavigationConfig::default();
        for entry in std::fs::read_dir("examples/CONFIG").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                let content = std::fs::read_to_string(&path).unwrap();
                partial_configuration(&defaults, &content)
                    .unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
            }
        }
    }
}
