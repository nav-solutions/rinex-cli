//! Command line tool to parse and analyze `RINEX` files.    
//! Refer to README for command line arguments.    
//! Homepage: <https://github.com/georust/rinex-cli>

mod cli; // command line interface
mod fops; // file operations
mod preprocessing; // preprocessing
mod report; // custom reports

#[cfg(feature = "ppp")]
mod positioning; // post processed positioning

use report::Report;

use preprocessing::context_preprocessing;

use gnss_qc::prelude::QcContext;
use rinex::prelude::{FormattingError as RinexFormattingError, ParsingError as RinexParsingError};

use std::path::Path;
use walkdir::WalkDir;

extern crate gnss_rs as gnss;

use rinex::prelude::qc::MergeError;

use cli::{Cli, Context, Workspace};

#[cfg(feature = "csv")]
use csv::Error as CsvError;

#[cfg(feature = "ppp")]
use gnss_qc::prelude::QcExtraPage;

use env_logger::{Builder, Target};

#[macro_use]
extern crate log;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o error")]
    StdioError(#[from] std::io::Error),
    #[error("missing OBS RINEX")]
    MissingObservationRinex,
    #[error("RINEX parsing error: {0}")]
    RinexParsing(#[from] RinexParsingError),
    #[error("RINEX formatting error: {0}")]
    RinexFormatting(#[from] RinexFormattingError),
    #[error("Qc merge error: {0}")]
    Merge(#[from] MergeError),
    #[error("missing (BRDC) NAV RINEX")]
    MissingNavigationRinex,
    #[error("missing IONEX")]
    MissingIONEX,
    #[error("missing Meteo RINEX")]
    MissingMeteoRinex,
    #[error("missing Clock RINEX")]
    MissingClockRinex,
    #[cfg(feature = "csv")]
    #[error("csv export error")]
    CsvError(#[from] CsvError),
    #[cfg(feature = "ppp")]
    #[error("positioning solver error")]
    PositioningSolverError(#[from] positioning::Error),
}

/// Parses and preprepocess all files passed by User
fn user_data_parsing(
    cli: &Cli,
    single_files: Vec<&String>,
    directories: Vec<&String>,
    max_depth: usize,
) -> QcContext {
    let mut ctx = QcContext::new();

    if cli.jpl_bpc_update() {
        #[cfg(not(feature = "ppp"))]
        error!("--jpl-bpc only applies along PPP/PVT solver options");

        #[cfg(feature = "ppp")]
        ctx.with_jpl_bpc()
            .unwrap_or_else(|e| panic!("Upgrade to high precision context failed: {}", e));
    }

    // recursive dir loader
    for dir in directories.iter() {
        let walkdir = WalkDir::new(dir).max_depth(max_depth);
        for entry in walkdir.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            if !path.is_dir() {
                let extension = path
                    .extension()
                    .unwrap_or_else(|| {
                        panic!("failed to determine file extension: \"{}\"", path.display())
                    })
                    .to_string_lossy()
                    .to_string();

                if extension == "gz" {
                    match ctx.load_gzip_rinex_file(path) {
                        Ok(_) => {
                            info!("RINEX file loaded: \"{}\"", path.display());
                        },
                        Err(_) => match ctx.load_gzip_sp3_file(path) {
                            Ok(_) => {
                                info!("SP3 file loaded: \"{}\"", path.display());
                            },
                            Err(_) => {
                                panic!("File format not recognized!");
                            },
                        },
                    }
                } else {
                    match ctx.load_rinex_file(path) {
                        Ok(_) => {
                            info!("RINEX file loaded: \"{}\"", path.display());
                        },
                        Err(_) => match ctx.load_sp3_file(path) {
                            Ok(_) => {
                                info!("SP3 file loaded: \"{}\"", path.display());
                            },
                            Err(_) => {
                                panic!("File format not recognized!");
                            },
                        },
                    }
                }
            }
        }
    }

    // load individual files
    for fp in single_files.iter() {
        let path = Path::new(fp);

        let extension = path
            .extension()
            .unwrap_or_else(|| panic!("failed to determine file extension: \"{}\"", path.display()))
            .to_string_lossy()
            .to_string();

        if extension == "gz" {
            match ctx.load_gzip_rinex_file(path) {
                Ok(_) => {
                    info!("RINEX file loaded: \"{}\"", path.display());
                },
                Err(_) => match ctx.load_gzip_sp3_file(path) {
                    Ok(_) => {
                        info!("SP3 file loaded: \"{}\"", path.display());
                    },
                    Err(_) => {
                        panic!("File format not recognized!");
                    },
                },
            }
        } else {
            match ctx.load_rinex_file(path) {
                Ok(_) => {
                    info!("RINEX file loaded: \"{}\"", path.display());
                },
                Err(_) => match ctx.load_sp3_file(path) {
                    Ok(_) => {
                        info!("SP3 file loaded: \"{}\"", path.display());
                    },
                    Err(_) => {
                        panic!("File format not recognized!");
                    },
                },
            }
        }
    }

    // Preprocessing
    context_preprocessing(&mut ctx, cli);

    debug!("{:?}", ctx);

    ctx
}

pub fn main() -> Result<(), Error> {
    let mut builder = Builder::from_default_env();

    builder
        .target(Target::Stdout)
        .format_timestamp_secs()
        .format_module_path(false)
        .init();

    /*
     * Build context defined by user
     *   Parse all data, determine other useful information
     */
    let cli = Cli::new();
    let max_recursive_depth = cli.recursive_depth();

    // User (ROVER) Data parsing
    let mut data_ctx = user_data_parsing(
        &cli,
        cli.rover_files(),
        cli.rover_directories(),
        max_recursive_depth,
    );

    let ctx_stem = Context::context_stem(&mut data_ctx);

    // Input context
    let mut ctx = Context {
        name: ctx_stem.clone(),

        #[cfg(feature = "ppp")]
        rx_orbit: {
            // possible reference point
            if let Some(rx_orbit) = data_ctx.reference_rx_orbit() {
                let (lat_ddeg, long_ddeg, alt_km) = rx_orbit
                    .latlongalt()
                    .unwrap_or_else(|e| panic!("latlongalt - physical error: {}", e));

                info!(
                    "reference point identified: latitude={:.5}°, longitude={:.5}° altitude={:.5}m",
                    lat_ddeg,
                    long_ddeg,
                    alt_km * 1.0E3
                );
                Some(rx_orbit)
            } else {
                warn!("no reference point identified");
                None
            }
        },

        data: data_ctx,
        quiet: cli.matches.get_flag("quiet"),
        workspace: Workspace::new(&ctx_stem, &cli),
    };

    // ground reference point
    #[cfg(feature = "ppp")]
    match ctx.rx_orbit {
        Some(_) => {
            if let Some(obs_rinex) = ctx.data.observation() {
                if let Some(t0) = obs_rinex.first_epoch() {
                    if let Some(rx_orbit) = cli.manual_rx_orbit(t0, ctx.data.earth_cef) {
                        let (lat_ddeg, long_ddeg, alt_km) = rx_orbit
                            .latlongalt()
                            .unwrap_or_else(|e| panic!("latlongalt - physical error: {}", e));

                        info!("reference point manually overwritten: latitude={:.5}°, longitude={:.5}°, altitude={:.5}m", lat_ddeg, long_ddeg, alt_km * 1.0E3);
                        ctx.rx_orbit = Some(rx_orbit);
                    }
                }
            } else {
                panic!("manual definition of a reference point requires OBS RINEX");
            }
        },
        None => {
            if let Some(obs_rinex) = ctx.data.observation() {
                if let Some(t0) = obs_rinex.first_epoch() {
                    if let Some(rx_orbit) = cli.manual_rx_orbit(t0, ctx.data.earth_cef) {
                        let (lat_ddeg, long_ddeg, alt_km) = rx_orbit
                            .latlongalt()
                            .unwrap_or_else(|e| panic!("latlongalt - physical error: {}", e));

                        info!("reference point manually defined: latitude={:.5}°, longitude={:.5}°, altitude={:.5}m", lat_ddeg, long_ddeg, alt_km * 1.0E3);
                        ctx.rx_orbit = Some(rx_orbit);
                    }
                }
            }
        },
    }

    // Prepare for file operation (output products)
    if cli.is_file_operation_run() {
        // possible seamless CRINEX/RINEX compression
        if cli.rnx2crnx() {
            if let Some(observation) = ctx.data.observation_mut() {
                info!("internal RNX2CRX compression");
                observation.rnx2crnx_mut();
            }
        }

        if cli.crnx2rnx() {
            if let Some(observation) = ctx.data.observation_mut() {
                info!("internal CRX2RNX decompression");
                observation.crnx2rnx_mut();
            }
        }
    }

    // Exclusive opmodes to follow
    #[cfg(feature = "ppp")]
    let mut extra_pages = Vec::<QcExtraPage>::new();

    match cli.matches.subcommand() {
        // File operations abort here and do not continue to analysis opmode (special case).
        // Users need to re-run (re execute) on previously generated data
        // to perform their analysis.
        Some(("filegen", submatches)) => {
            fops::filegen(&ctx, &cli.matches, submatches)?;
            return Ok(());
        },

        Some(("merge", submatches)) => {
            fops::merge(&ctx, &cli, submatches)?;
            return Ok(());
        },

        Some(("split", submatches)) => {
            fops::split(&ctx, submatches)?;
            return Ok(());
        },

        Some(("tbin", submatches)) => {
            fops::time_binning(&ctx, &cli.matches, submatches)?;
            return Ok(());
        },

        Some(("cbin", submatches)) => {
            fops::constell_timescale_binning(&ctx, submatches)?;
            return Ok(());
        },

        Some(("diff", submatches)) => {
            fops::diff(&ctx, &cli, submatches)?;
            return Ok(());
        },

        #[cfg(feature = "ppp")]
        Some(("ppp", submatches)) => {
            let chapter = positioning::precise_positioning(&ctx, false, submatches)?;
            extra_pages.push(chapter);
        },

        #[cfg(feature = "ppp")]
        Some(("rtk", submatches)) => {
            let chapter = positioning::precise_positioning(&ctx, true, submatches)?;
            extra_pages.push(chapter);
        },
        _ => {},
    }

    // report
    let cfg = cli.qc_config();

    let mut report = Report::new(&cli, &ctx, cfg);

    #[cfg(feature = "ppp")]
    for extra in extra_pages {
        // customization
        report.customize(extra);
    }

    // synthesis
    report.generate(&cli, &ctx)?;

    if !ctx.quiet {
        ctx.workspace.open_with_web_browser();
    }

    Ok(())
} // main
