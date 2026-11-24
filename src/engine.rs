//! Deploys the GNSS-Qc engine for the selected command, and dispatches the
//! output stream to every consumer (data files, dedicated formats, reports).
use std::time::Instant;

use log::{info, warn};

use gnss_qc::prelude::{
    Duration, Epoch, QcEngine, QcEngineBuilder, QcEphemerisCoverageProcessor,
    QcObservationResidualConfig, QcObservationResidualProcessor, QcOrbitProduct,
    QcOrbitResidualConfig, QcOrbitResidualProcessor, QcOutput, QcOutputItem, QcPoll,
    QcSignalProcessor,
};

use crate::{
    cli::{parse_constellations, Cli, Command, ResidualsKind},
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
}

/// Engine and consumers, ready to run
pub struct Plan {
    pub engine: QcEngine<QcOutput>,
    pub consumers: Vec<Box<dyn Consumer>>,
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

    let (command, engine) = match &cli.command {
        Command::Index(_)
        | Command::Filegen(_)
        | Command::Merge(_)
        | Command::Split(_)
        | Command::Tbin(_)
        | Command::Cbin(_)
        | Command::Diff(_) => unreachable!("this command does not deploy the engine"),

        Command::Qc(args) => {
            let mut builder = builder(cli, &mut inputs);

            let has_nav = inputs.entries(ProductKind::Navigation).next().is_some();

            if !args.no_coverage {
                if has_nav {
                    builder = builder.with_processor(QcEphemerisCoverageProcessor::default());
                    setup.push(("Ephemeris coverage".to_string(), "yes".to_string()));
                } else {
                    warn!("no navigation RINEX: ephemeris coverage is not analyzed");
                }
            }

            let engine = builder
                .with_processor(QcSignalProcessor)
                .build()
                .map_err(engine_error)?;

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

            let processor = QcNavigationProcessor::new(almanac, cfg)
                .map_err(|e| Error::Navigation(e.to_string()))?;

            let engine = builder(cli, &mut inputs)
                .with_processor(processor)
                .build()
                .map_err(engine_error)?;

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

            let processor = QcCggttsProcessor::new(almanac, cfg, cggtts_cfg)
                .map_err(|e| Error::Navigation(e.to_string()))?;

            let engine = builder(cli, &mut inputs)
                .with_processor(processor)
                .build()
                .map_err(engine_error)?;

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

                let engine = builder(cli, &mut inputs)
                    .with_processor(QcObservationResidualProcessor::new(cfg))
                    .build()
                    .map_err(engine_error)?;

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

            let engine = builder(cli, &mut inputs)
                .with_processor(QcIppProcessor::new(cfg))
                .build()
                .map_err(engine_error)?;

            ("ipp", engine)
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
