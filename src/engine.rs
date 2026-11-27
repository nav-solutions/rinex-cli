//! Deploys the GNSS-Qc engine for the selected command, and dispatches the
//! output stream to every consumer (data files, dedicated formats, reports).
use std::{str::FromStr, time::Instant};

use log::{info, warn};

use gnss_qc::prelude::{
    Duration, Epoch, QcClockAlignment, QcClockProduct, QcClockResidualConfig,
    QcClockResidualProcessor, QcEngine, QcEngineBuilder, QcEphemerisCoverageProcessor,
    QcEphemerisProcessor, QcMeteoConfig, QcMeteoProcessor, QcObservationResidualConfig,
    QcObservationResidualProcessor, QcOrbitProduct, QcOrbitResidualConfig,
    QcOrbitResidualProcessor, QcOutput, QcOutputItem, QcPoll, QcSignalProcessor,
};

use crate::{
    cli::{parse_constellations, AlignmentArg, Cli, Command, ResidualsKind},
    error::Error,
    input::{Inputs, ProductEntry, ProductKind},
    report::{Formats, ReportSink},
    sink::{Consumer, CsvSink, JsonlSink, RunSummary},
    summary::SummarySink,
    workspace::Workspace,
};

/// Session description, shared by the consumers (summary, reports)
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionContext {
    /// Title
    pub title: String,

    /// Session name
    pub session: String,

    /// Command (opmode)
    pub command: String,

    /// Command line
    pub command_line: String,

    /// Configuration, as presented to the user
    pub setup: Vec<(String, String)>,

    /// Input products
    pub manifest: Vec<ProductEntry>,

    /// Navigation context
    #[cfg(feature = "navigation")]
    pub navigation: Option<crate::nav::NavigationSetup>,

    /// Quality control of the observations deployed (signal sources, sky view)
    pub qc: bool,

    /// Signal combinations deployed next to a processing command (navigation)
    pub combinations: bool,

    /// TEC maps of the obs2tec command
    #[cfg(feature = "ionex")]
    #[serde(skip)]
    pub obs2tec: Option<crate::obs2tec::Obs2TecSetup>,
}

/// Engine and consumers, ready to run
pub struct Plan {
    pub engine: QcEngine<QcOutput>,
    pub consumers: Vec<Box<dyn Consumer>>,
}

/// Signal combinations (GF, IF, code multipath) of the navigated signal sources
#[cfg(feature = "navigation")]
fn with_combinations(
    builder: QcEngineBuilder<QcOutput>,
    cfg: gnss_qc::prelude::QcCombinationConfig,
    setup: &mut Vec<(String, String)>,
) -> QcEngineBuilder<QcOutput> {
    setup.push((
        "Signal combinations".to_string(),
        format!(
            "GF, IF, MP: {}",
            cfg.receivers
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ));
    builder.with_processor(gnss_qc::prelude::QcCombinationProcessor::new(cfg))
}

/// Quality control of the observations: signal sources, and with navigation RINEX,
/// ephemeris coverage and broadcast ephemerides (sky view).
/// Returns false when there is no observation to analyze.
fn with_qc(
    builder: QcEngineBuilder<QcOutput>,
    inputs: &Inputs,
    coverage: bool,
    setup: &mut Vec<(String, String)>,
) -> (QcEngineBuilder<QcOutput>, bool) {
    if inputs.entries(ProductKind::Observation).next().is_none() {
        return (builder, false);
    }

    let mut builder = builder.with_processor(QcSignalProcessor);

    if coverage {
        if inputs.entries(ProductKind::Navigation).next().is_some() {
            builder = builder
                .with_processor(QcEphemerisCoverageProcessor::default())
                // broadcast ephemerides, for the sky plots
                .with_processor(QcEphemerisProcessor::default());
            setup.push(("Ephemeris coverage".to_string(), "yes".to_string()));
        } else {
            warn!("no navigation RINEX: ephemeris coverage and sky view are not analyzed");
        }
    }

    (builder, true)
}

fn builder(cli: &Cli, inputs: &mut Inputs) -> QcEngineBuilder<QcOutput> {
    let products = std::mem::take(&mut inputs.products);
    QcEngine::<QcOutput>::builder()
        .with_products(products)
        .with_time_window(cli.inputs.start, cli.inputs.end)
}

fn engine_error<E: std::fmt::Display>(e: E) -> Error {
    Error::QcEngine(e.to_string())
}

#[cfg(not(feature = "navigation"))]
fn missing_feature(feature: &str) -> Error {
    Error::Config(format!(
        "rinex-cli was built without the \"{}\" feature: cargo install rinex-cli --features {}",
        feature, feature
    ))
}

/// Parses an orbital product designation: sp3, sp3:PRODUCT, nav, nav:PRODUCT
fn orbit_product(text: &str, inputs: &Inputs) -> Result<QcOrbitProduct, Error> {
    let (kind, product) = match text.split_once(':') {
        Some((kind, product)) => (kind, Some(product)),
        None => (text, None),
    };

    match kind.to_lowercase().as_str() {
        "sp3" | "precise" => Ok(QcOrbitProduct::Precise(
            product
                .map(|p| inputs.resolve(p, ProductKind::SP3))
                .transpose()?,
        )),
        "nav" | "brdc" | "broadcast" => Ok(QcOrbitProduct::Broadcast(
            product
                .map(|p| inputs.resolve(p, ProductKind::Navigation))
                .transpose()?,
        )),
        _ => Err(Error::Config(format!(
            "invalid orbital product \"{}\": sp3, sp3:PRODUCT, nav or nav:PRODUCT",
            text
        ))),
    }
}

/// Parses a clock product designation: clk, clk:PRODUCT, sp3, sp3:PRODUCT, nav, nav:PRODUCT
fn clock_product(text: &str, inputs: &Inputs) -> Result<QcClockProduct, Error> {
    let (kind, product) = match text.split_once(':') {
        Some((kind, product)) => (kind, Some(product)),
        None => (text, None),
    };

    match kind.to_lowercase().as_str() {
        "clk" | "clock" => Ok(QcClockProduct::Clock(
            product
                .map(|p| inputs.resolve(p, ProductKind::Clock))
                .transpose()?,
        )),
        "sp3" | "precise" => Ok(QcClockProduct::PreciseState(
            product
                .map(|p| inputs.resolve(p, ProductKind::SP3))
                .transpose()?,
        )),
        "nav" | "brdc" | "broadcast" => Ok(QcClockProduct::Broadcast(
            product
                .map(|p| inputs.resolve(p, ProductKind::Navigation))
                .transpose()?,
        )),
        _ => Err(Error::Config(format!(
            "invalid clock product \"{}\": clk, clk:PRODUCT, sp3, sp3:PRODUCT, nav or nav:PRODUCT",
            text
        ))),
    }
}

fn describe_clock_product(product: &QcClockProduct) -> String {
    let kind = match product {
        QcClockProduct::Clock(_) => "Clock RINEX",
        QcClockProduct::PreciseState(_) => "SP3",
        QcClockProduct::Broadcast(_) => "broadcast",
    };
    match product.source() {
        Some(id) => format!("{} ({})", kind, id),
        None => format!("{} (any)", kind),
    }
}

fn describe_orbit_product(product: &QcOrbitProduct) -> String {
    match product {
        QcOrbitProduct::Precise(Some(id)) => format!("precise ({})", id),
        QcOrbitProduct::Precise(None) => "precise (any)".to_string(),
        QcOrbitProduct::Broadcast(Some(id)) => format!("broadcast ({})", id),
        QcOrbitProduct::Broadcast(None) => "broadcast (any)".to_string(),
    }
}

/// Deploys the engine and the consumers for this command
pub fn plan(cli: &Cli, mut inputs: Inputs, workspace: &Workspace) -> Result<Plan, Error> {
    let mut consumers = Vec::<Box<dyn Consumer>>::new();
    let mut setup = Vec::<(String, String)>::new();

    #[cfg(feature = "navigation")]
    let mut navigation = None;

    #[cfg(feature = "ionex")]
    let mut obs2tec = None;

    // quality control of the observations, in the reports of the processing commands
    let qc = !cli.outputs.no_qc;
    let mut qc_deployed = false;
    #[allow(unused_mut)]
    let mut combinations_deployed = false;

    let (command, engine) = match &cli.command {
        Command::Index(_)
        | Command::Filegen(_)
        | Command::Merge(_)
        | Command::Split(_)
        | Command::Tbin(_)
        | Command::Cbin(_)
        | Command::Diff(_) => unreachable!("this command does not deploy the engine"),

        Command::Qc(args) => {
            let builder = builder(cli, &mut inputs);
            let (builder, _) = with_qc(builder, &inputs, !args.no_coverage, &mut setup);
            qc_deployed = true;

            let engine = builder.build().map_err(engine_error)?;

            ("qc", engine)
        },

        #[cfg(not(feature = "navigation"))]
        Command::Nav(_) => return Err(missing_feature("navigation")),

        #[cfg(feature = "navigation")]
        Command::Nav(args) => {
            use crate::sink::{TrackFormat, TrackSink};
            use gnss_qc::prelude::QcNavigationProcessor;

            let base = args
                .base
                .as_deref()
                .map(|base| (base, args.base_position.as_deref()));

            let (cfg, nav_setup) = crate::nav::configure(&args.navigation, base, &inputs)?;
            setup.extend(nav_setup.description.iter().cloned());

            let almanac = crate::nav::almanac(args.navigation.anise_dir.as_deref())?;

            let combination_cfg = crate::nav::combinations(&cfg);

            let processor = QcNavigationProcessor::new(almanac, cfg)
                .map_err(|e| Error::Navigation(e.to_string()))?;

            let mut builder = builder(cli, &mut inputs).with_processor(processor);

            // signal combinations of the navigated signal sources
            if !args.no_combinations {
                builder = with_combinations(builder, combination_cfg, &mut setup);
                combinations_deployed = true;
            }

            if qc {
                (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
            }
            let engine = builder.build().map_err(engine_error)?;

            if args.gpx {
                consumers.push(Box::new(TrackSink::new(workspace, TrackFormat::Gpx)));
            }
            if args.kml {
                consumers.push(Box::new(TrackSink::new(workspace, TrackFormat::Kml)));
            }

            navigation = Some(nav_setup);
            ("nav", engine)
        },

        #[cfg(not(feature = "cggtts"))]
        Command::Cggtts(_) => {
            #[cfg(feature = "navigation")]
            return Err(Error::Config(
                "rinex-cli was built without the \"cggtts\" feature: cargo install rinex-cli --features cggtts".to_string(),
            ));
            #[cfg(not(feature = "navigation"))]
            return Err(missing_feature("cggtts"));
        },

        #[cfg(feature = "cggtts")]
        Command::Cggtts(args) => {
            use crate::sink::{CggttsHeader, CggttsSink};
            use gnss_qc::prelude::{QcCggttsConfig, QcCggttsProcessor};

            let (cfg, nav_setup) = crate::nav::configure(&args.navigation, None, &inputs)?;
            setup.extend(nav_setup.description.iter().cloned());

            let cggtts_cfg = QcCggttsConfig {
                min_track_duration: Duration::from_seconds(args.min_track),
                pvt_solutions: args.solutions,
                ..Default::default()
            };

            setup.push((
                "Minimal track duration".to_string(),
                cggtts_cfg.min_track_duration.to_string(),
            ));

            let almanac = crate::nav::almanac(args.navigation.anise_dir.as_deref())?;

            let combination_cfg = crate::nav::combinations(&cfg);

            let processor = QcCggttsProcessor::new(almanac, cfg, cggtts_cfg)
                .map_err(|e| Error::Navigation(e.to_string()))?;

            let mut builder = builder(cli, &mut inputs).with_processor(processor);

            // signal combinations of the receiver
            if !args.no_combinations {
                builder = with_combinations(builder, combination_cfg, &mut setup);
                combinations_deployed = true;
            }

            if qc {
                (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
            }
            let engine = builder.build().map_err(engine_error)?;

            // station: user, marker name, receiver identity
            let station = args.station.clone().unwrap_or_else(|| {
                inputs
                    .entries(ProductKind::Observation)
                    .find(|entry| entry.identity.to_string() == nav_setup.rover)
                    .and_then(|entry| {
                        entry
                            .details
                            .iter()
                            .find(|(key, _)| key == "marker")
                            .map(|(_, marker)| marker.clone())
                    })
                    .unwrap_or_else(|| inputs.session.chars().take(4).collect())
            });

            consumers.push(Box::new(CggttsSink::new(
                workspace,
                CggttsHeader {
                    station,
                    frame: args.frame.clone(),
                    apc_ecef_m: nav_setup.reference_ecef_m,
                    comment: format!(
                        "rinex-cli v{} (gnss-qc) {}",
                        env!("CARGO_PKG_VERSION"),
                        nav_setup
                            .description
                            .first()
                            .map(|(_, method)| method.clone())
                            .unwrap_or_default()
                    ),
                },
            )));

            navigation = Some(nav_setup);
            ("cggtts", engine)
        },

        Command::Residuals(args) => match &args.kind {
            ResidualsKind::Obs(args) => {
                let mut cfg = QcObservationResidualConfig {
                    lhs: args
                        .lhs
                        .as_deref()
                        .map(|p| inputs.resolve(p, ProductKind::Observation))
                        .transpose()?,
                    rhs: args
                        .rhs
                        .as_deref()
                        .map(|p| inputs.resolve(p, ProductKind::Observation))
                        .transpose()?,
                    tolerance: args.tolerance.map(Duration::from_seconds),
                    ..Default::default()
                };

                if inputs.entries(ProductKind::Observation).count() < 2 {
                    return Err(Error::Config(
                        "observation residuals require two signal sources".to_string(),
                    ));
                }

                if args.all {
                    cfg = cfg.with_all_observables();
                }

                setup.push((
                    "A".to_string(),
                    cfg.lhs
                        .as_ref()
                        .map(|id| id.to_string())
                        .unwrap_or("first signal source".to_string()),
                ));
                setup.push((
                    "B".to_string(),
                    cfg.rhs
                        .as_ref()
                        .map(|id| id.to_string())
                        .unwrap_or("other signal source".to_string()),
                ));

                let mut builder = builder(cli, &mut inputs)
                    .with_processor(QcObservationResidualProcessor::new(cfg));
                if qc {
                    (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
                }
                let engine = builder.build().map_err(engine_error)?;

                ("residuals obs", engine)
            },
            ResidualsKind::Orbit(args) => {
                let lhs = orbit_product(&args.lhs, &inputs)?;
                let rhs = orbit_product(&args.rhs, &inputs)?;

                setup.push(("A".to_string(), describe_orbit_product(&lhs)));
                setup.push(("B".to_string(), describe_orbit_product(&rhs)));

                let mut cfg = QcOrbitResidualConfig::broadcast_precise();
                cfg.lhs = lhs;
                cfg.rhs = rhs;
                cfg.sampling = args.sampling.map(Duration::from_seconds);
                cfg.max_broadcast_age = args.max_age.map(Duration::from_seconds);

                if !args.constellations.is_empty() {
                    cfg = cfg.with_constellations(&parse_constellations(&args.constellations)?);
                }

                let engine = builder(cli, &mut inputs)
                    .with_processor(QcOrbitResidualProcessor::new(cfg))
                    .build()
                    .map_err(engine_error)?;

                ("residuals orbit", engine)
            },
            ResidualsKind::Clock(args) => {
                let lhs = clock_product(&args.lhs, &inputs)?;
                let rhs = clock_product(&args.rhs, &inputs)?;

                setup.push(("A".to_string(), describe_clock_product(&lhs)));
                setup.push(("B".to_string(), describe_clock_product(&rhs)));

                let mut cfg = QcClockResidualConfig::new(lhs, rhs);
                cfg.sampling = args.sampling.map(Duration::from_seconds);
                cfg.alignment = match args.alignment {
                    AlignmentArg::None => QcClockAlignment::None,
                    AlignmentArg::Epoch => QcClockAlignment::Epoch,
                    AlignmentArg::Constellation => QcClockAlignment::Constellation,
                };

                if args.satellites_only {
                    cfg = cfg.satellites_only();
                }
                if args.stations_only {
                    cfg = cfg.stations_only();
                }

                if !args.constellations.is_empty() {
                    cfg = cfg.with_constellations(&parse_constellations(&args.constellations)?);
                }

                setup.push((
                    "Clock datum".to_string(),
                    format!("{:?}", cfg.alignment),
                ));

                if let Some(dt) = cfg.sampling {
                    setup.push(("Sampling".to_string(), dt.to_string()));
                }

                let engine = builder(cli, &mut inputs)
                    .with_processor(QcClockResidualProcessor::new(cfg))
                    .build()
                    .map_err(engine_error)?;

                ("residuals clock", engine)
            },
        },

        Command::Combinations(args) => {
            use gnss_qc::prelude::{QcCombinationConfig, QcCombinationKind, QcCombinationProcessor};

            if inputs.entries(ProductKind::Observation).next().is_none() {
                return Err(Error::Config(
                    "signal combinations require observations".to_string(),
                ));
            }

            let all = !args.gf && !args.iono_free && !args.mp;

            let mut cfg = QcCombinationConfig {
                geometry_free: all || args.gf,
                ionosphere_free: all || args.iono_free,
                multipath: all || args.mp,
                code: !args.phase_only,
                phase: !args.code_only,
                max_gap: Duration::from_seconds(args.max_gap),
                jump_threshold_m: args.jump,
                ..Default::default()
            };

            if !args.constellations.is_empty() {
                cfg.constellations = parse_constellations(&args.constellations)?;
            }

            for receiver in args.receivers.iter() {
                cfg.receivers
                    .push(inputs.resolve(receiver, ProductKind::Observation)?);
            }

            let kinds = [
                (cfg.geometry_free, QcCombinationKind::GeometryFree),
                (cfg.ionosphere_free, QcCombinationKind::IonosphereFree),
                (cfg.multipath, QcCombinationKind::Multipath),
            ]
            .iter()
            .filter(|(selected, _)| *selected)
            .map(|(_, kind)| kind.to_string())
            .collect::<Vec<_>>();

            setup.push(("Combinations".to_string(), kinds.join(", ")));
            setup.push((
                "Measurements (GF, IF)".to_string(),
                match (cfg.code, cfg.phase) {
                    (true, true) => "code and phase",
                    (true, false) => "code",
                    _ => "phase",
                }
                .to_string(),
            ));
            if cfg.multipath {
                setup.push((
                    "Multipath arcs".to_string(),
                    format!(
                        "loss of lock, gaps > {} s, jumps > {} m",
                        args.max_gap, args.jump
                    ),
                ));
            }
            if !cfg.constellations.is_empty() {
                setup.push((
                    "Constellations".to_string(),
                    cfg.constellations
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
            if !cfg.receivers.is_empty() {
                setup.push((
                    "Signal sources".to_string(),
                    cfg.receivers
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }

            let mut builder =
                builder(cli, &mut inputs).with_processor(QcCombinationProcessor::new(cfg));
            if qc {
                (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
            }
            let engine = builder.build().map_err(engine_error)?;

            ("combinations", engine)
        },

        Command::Meteo(args) => {
            if inputs.entries(ProductKind::Meteo).next().is_none() {
                return Err(Error::Config(
                    "the meteo command requires Meteo RINEX".to_string(),
                ));
            }

            let mut cfg = QcMeteoConfig::default();

            if !args.observables.is_empty() {
                let observables = args
                    .observables
                    .iter()
                    .map(|code| {
                        rinex::prelude::Observable::from_str(code.trim()).map_err(|_| {
                            Error::Config(format!("unknown meteo observable \"{}\"", code))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                setup.push((
                    "Observables".to_string(),
                    observables
                        .iter()
                        .map(|o| o.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));

                cfg = cfg.with_observables(&observables);
            }

            let engine = builder(cli, &mut inputs)
                .with_processor(QcMeteoProcessor::new(cfg))
                .build()
                .map_err(engine_error)?;

            ("meteo", engine)
        },

        #[cfg(not(feature = "ionex"))]
        Command::Tec(_) => {
            return Err(Error::Config(
                "rinex-cli was built without the \"ionex\" feature: cargo install rinex-cli --features ionex".to_string(),
            ))
        },

        #[cfg(feature = "ionex")]
        Command::Tec(args) => {
            use crate::cli::InterpolationArg;
            use gnss_qc::prelude::{QcTecConfig, QcTecInterpolation, QcTecProcessor, QcTecSampling};

            if inputs.entries(ProductKind::Ionex).next().is_none() {
                return Err(Error::Config(
                    "the tec command requires IONEX files".to_string(),
                ));
            }

            let mut cfg = QcTecConfig::default().with_points();

            cfg.interpolation = match args.interpolation {
                InterpolationArg::Nearest => QcTecInterpolation::NearestMap,
                InterpolationArg::Linear => QcTecInterpolation::Linear,
                InterpolationArg::Rotated => QcTecInterpolation::RotatedMaps,
            };

            if let Some(factor) = args.upsample {
                if !(factor.is_finite() && factor > 0.0) {
                    return Err(Error::Config(format!("invalid upsampling factor {}", factor)));
                }
                cfg.sampling = QcTecSampling::Factor(factor);
            }

            if let Some(period) = args.resample {
                cfg.sampling = QcTecSampling::Period(period);
            }

            for region in args.region.iter() {
                cfg.regions.push(crate::cli::parse_region(region)?);
            }

            setup.push((
                "Maps".to_string(),
                match cfg.sampling {
                    QcTecSampling::Native => "native".to_string(),
                    QcTecSampling::Factor(k) => format!("native sampling / {}", k),
                    QcTecSampling::Period(dt) => format!("one map every {}", dt),
                },
            ));
            setup.push((
                "Temporal interpolation".to_string(),
                format!("{:?}", cfg.interpolation),
            ));
            setup.push((
                "Regions".to_string(),
                cfg.regions
                    .iter()
                    .map(|r| r.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));

            if cfg.sampling != QcTecSampling::Native && !args.no_ionex {
                consumers.push(Box::new(crate::sink::IonexSink::new(
                    workspace,
                    &inputs.ionex,
                    cfg.interpolation,
                    args.gzip,
                )));
            }

            let engine = builder(cli, &mut inputs)
                .with_processor(QcTecProcessor::new(cfg))
                .build()
                .map_err(engine_error)?;

            ("tec", engine)
        },

        #[cfg(not(feature = "ionex"))]
        Command::Ipp(_) => {
            return Err(Error::Config(
                "rinex-cli was built without the \"ionex\" feature: cargo install rinex-cli --features ionex".to_string(),
            ))
        },

        #[cfg(feature = "ionex")]
        Command::Ipp(args) => {
            use gnss_qc::prelude::{QcIppConfig, QcIppProcessor};

            let cfg = QcIppConfig {
                orbits: args.orbits.to_preference(),
                elevation_mask_deg: args.elevation_mask,
                shell_altitude_km: args.shell_altitude,
                constellations: if args.constellations.is_empty() {
                    None
                } else {
                    Some(parse_constellations(&args.constellations)?)
                },
                ..Default::default()
            };

            setup.push((
                "Elevation mask".to_string(),
                format!("{}°", cfg.elevation_mask_deg),
            ));
            setup.push((
                "Shell altitude".to_string(),
                format!("{} km", cfg.shell_altitude_km),
            ));

            let mut builder = builder(cli, &mut inputs).with_processor(QcIppProcessor::new(cfg));
            if qc {
                (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
            }
            let engine = builder.build().map_err(engine_error)?;

            ("ipp", engine)
        },

        #[cfg(not(feature = "ionex"))]
        Command::Obs2tec(_) => {
            return Err(Error::Config(
                "rinex-cli was built without the \"ionex\" feature: cargo install rinex-cli --features ionex".to_string(),
            ))
        },

        #[cfg(feature = "ionex")]
        Command::Obs2tec(args) => {
            use crate::obs2tec::{parse_point, GridExtent, Obs2TecSetup};
            use gnss_qc::prelude::{QcObsTecConfig, QcObsTecProcessor};
            use std::str::FromStr;

            if inputs.entries(ProductKind::Observation).next().is_none() {
                return Err(Error::Config(
                    "obs2tec requires observations (one or more receivers)".to_string(),
                ));
            }

            let mut cfg = QcObsTecConfig {
                orbits: args.orbits.to_preference(),
                elevation_mask_deg: args.elevation_mask,
                shell_altitude_km: args.shell_altitude,
                max_gap: args.max_gap,
                slip_threshold_m: args.slip_threshold,
                ..Default::default()
            };

            if !args.constellations.is_empty() {
                cfg = cfg.with_constellations(&parse_constellations(&args.constellations)?);
            }

            for receiver in args.receivers.iter() {
                cfg = cfg.with_receiver(inputs.resolve(receiver, ProductKind::Observation)?);
            }

            for (name, value) in [("latitude", args.dlat), ("longitude", args.dlon)] {
                if !(value.is_finite() && value > 0.0) {
                    return Err(Error::Config(format!("invalid {} spacing {}", name, value)));
                }
            }

            let mut grid = Obs2TecSetup {
                extent: GridExtent::from_str(&args.grid).map_err(Error::Config)?,
                dlat_deg: args.dlat,
                dlon_deg: args.dlon,
                interval: args.interval,
                radius_deg: args.radius.unwrap_or(args.dlat.max(args.dlon)),
                min_samples: args.min_samples,
                rotation: !args.no_rotation,
                shell_altitude_km: cfg.shell_altitude_km,
                earth_radius_km: cfg.earth_radius_km,
                elevation_mask_deg: cfg.elevation_mask_deg,
                ..Default::default()
            };

            if grid.interval.to_seconds() <= 0.0 {
                return Err(Error::Config("invalid map interval".to_string()));
            }

            for region in args.region.iter() {
                grid.regions.push(crate::cli::parse_region(region)?);
            }

            for point in args.point.iter() {
                grid.points.push(parse_point(point).map_err(Error::Config)?);
            }

            setup.push((
                "Receivers".to_string(),
                if cfg.receivers.is_empty() {
                    "all signal sources".to_string()
                } else {
                    cfg.receivers
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            ));
            setup.push((
                "Elevation mask".to_string(),
                format!("{}°", cfg.elevation_mask_deg),
            ));
            setup.push((
                "Single layer".to_string(),
                format!("{} km, mapping function 1/cos(z')", cfg.shell_altitude_km),
            ));
            setup.push((
                "Phase arcs".to_string(),
                format!(
                    "end on loss of lock, gaps > {}, geometry free phase jumps > {} m",
                    cfg.max_gap, cfg.slip_threshold_m
                ),
            ));
            setup.push((
                "Differential code biases".to_string(),
                "not corrected (uncalibrated TEC)".to_string(),
            ));
            setup.push((
                "Maps".to_string(),
                format!(
                    "one every {}, {} grid, {}° × {}°, {} frame",
                    grid.interval,
                    match grid.extent {
                        GridExtent::Network => "network".to_string(),
                        GridExtent::World => "world".to_string(),
                        GridExtent::Bounds {
                            latitude_deg,
                            longitude_deg,
                        } => format!(
                            "[{}°, {}°] × [{}°, {}°]",
                            latitude_deg.0, latitude_deg.1, longitude_deg.0, longitude_deg.1
                        ),
                    },
                    grid.dlat_deg,
                    grid.dlon_deg,
                    if grid.rotation {
                        "Sun-fixed"
                    } else {
                        "Earth-fixed"
                    },
                ),
            ));
            setup.push((
                "Gridding".to_string(),
                format!(
                    "inverse distance weighted, radius {}°, at least {} pierce points per node",
                    grid.radius_deg, grid.min_samples
                ),
            ));

            if args.ionex {
                let name = cli
                    .outputs
                    .session
                    .clone()
                    .unwrap_or_else(|| inputs.session.clone());
                consumers.push(Box::new(crate::sink::Obs2TecIonexSink::new(
                    workspace, &name, &grid, args.gzip,
                )));
            }

            obs2tec = Some(grid);

            let mut builder =
                builder(cli, &mut inputs).with_processor(QcObsTecProcessor::new(cfg));
            if qc {
                (builder, qc_deployed) = with_qc(builder, &inputs, true, &mut setup);
            }
            let engine = builder.build().map_err(engine_error)?;

            ("obs2tec", engine)
        },
    };

    // generic consumers
    if cli.outputs.csv() {
        consumers.push(Box::new(CsvSink::new(workspace, cli.outputs.raw)));
    }
    if cli.outputs.jsonl() {
        consumers.push(Box::new(JsonlSink::file(workspace, cli.outputs.raw)?));
    }
    if cli.outputs.stdout {
        consumers.push(Box::new(JsonlSink::stdout(cli.outputs.raw)));
    }

    let context = SessionContext {
        title: cli
            .outputs
            .title
            .clone()
            .unwrap_or_else(|| inputs.session.clone()),
        session: inputs.session.clone(),
        command: command.to_string(),
        command_line: std::env::args().collect::<Vec<_>>().join(" "),
        setup,
        manifest: inputs.manifest.clone(),
        #[cfg(feature = "navigation")]
        navigation,
        qc: qc_deployed,
        combinations: combinations_deployed,
        #[cfg(feature = "ionex")]
        obs2tec,
    };

    consumers.push(Box::new(SummarySink::new(workspace, &context)));

    let formats = Formats {
        html: cli.outputs.html(),
        pdf: cli.outputs.pdf(),
    };

    if formats.html || formats.pdf {
        consumers.push(Box::new(ReportSink::new(workspace, &context, formats)));
    }

    Ok(Plan { engine, consumers })
}

/// Runs the engine until the end of the stream, dispatching every result
pub fn run(mut plan: Plan) -> Result<RunSummary, Error> {
    let start = Instant::now();
    let mut summary = RunSummary::default();
    let mut last_log: Option<Epoch> = None;

    loop {
        match plan.engine.poll_next().map_err(engine_error)? {
            QcPoll::Ready(item) => {
                dispatch(&mut plan.consumers, &item)?;

                summary.results += 1;
                if summary.first.is_none() {
                    summary.first = Some(item.epoch);
                }
                summary.last = Some(item.epoch);

                // progress, every 6 hours of data
                match last_log {
                    Some(t) if item.epoch - t < Duration::from_hours(6.0) => {},
                    _ => {
                        info!("processing {}", item.epoch);
                        last_log = Some(item.epoch);
                    },
                }
            },
            QcPoll::Pending => std::thread::sleep(std::time::Duration::from_millis(1)),
            QcPoll::EndOfStream => break,
        }
    }

    summary.stats = plan.engine.stats();
    summary.processing_time_s = start.elapsed().as_secs_f64();

    info!(
        "{} epochs processed, {} results ({:.1} s)",
        summary.stats.epochs, summary.results, summary.processing_time_s
    );

    if summary.stats.late_items > 0 {
        warn!(
            "{} late observations dropped (not in chronological order)",
            summary.stats.late_items
        );
    }

    for consumer in plan.consumers.iter_mut() {
        consumer.finish(&summary)?;
    }

    Ok(summary)
}

fn dispatch(consumers: &mut [Box<dyn Consumer>], item: &QcOutputItem) -> Result<(), Error> {
    for consumer in consumers.iter_mut() {
        consumer.consume(item)?;
    }
    Ok(())
}
