//! File operations: they apply to the (preprocessed) input products and write files,
//! the engine is not deployed.
use std::{path::Path, str::FromStr};

use log::{info, warn};

use gnss_qc::prelude::{Constellation, Epoch, QcIndexing};
use gnss_qc_traits::{Filter, FilterItem, MaskOperand, Merge, Preprocessing, Timeshift};

use rinex::{
    prelude::{ProductionAttributes, Rinex, RinexType},
    production::{DataSource, DetailedProductionAttributes, FFU, PPU},
};
use sp3::prelude::SP3;

use crate::{
    cli::{Cli, Command, ProductionArgs},
    error::Error,
    input::{parse, Inputs, Parsed, Preprocessor, ProductKind},
    workspace::Workspace,
};

/// Products available to the file operations
struct Products<'a> {
    rinex: Vec<(ProductKind, &'a QcIndexing, &'a Rinex)>,
    sp3: Vec<(&'a QcIndexing, &'a SP3)>,
}

fn products(inputs: &Inputs) -> Products<'_> {
    let mut rinex = Vec::new();

    for (id, r) in inputs.products.obs_rinex_iter() {
        rinex.push((ProductKind::Observation, id, r));
    }
    for (id, r) in inputs.products.nav_rinex_iter() {
        rinex.push((ProductKind::Navigation, id, r));
    }
    for (kind, id, r) in inputs.others.iter() {
        rinex.push((*kind, id, r));
    }

    Products {
        rinex,
        sp3: inputs.products.sp3_iter().collect(),
    }
}

/// Production attributes: guessed from the content, customized by the user
fn production_attributes(
    rinex: &Rinex,
    args: &ProductionArgs,
) -> Result<ProductionAttributes, Error> {
    let mut attributes = rinex.guess_production_attributes();

    if let Some(agency) = &args.agency {
        attributes.name = agency.clone();
    }

    let customized = args.country.is_some()
        || args.batch.is_some()
        || args.src.is_some()
        || args.ppu.is_some()
        || args.ffu.is_some();

    if customized {
        let details = attributes
            .v3_details
            .get_or_insert_with(DetailedProductionAttributes::default);

        if let Some(country) = &args.country {
            details.country = country.chars().take(3).collect::<String>().to_uppercase();
        }
        if let Some(batch) = args.batch {
            details.batch = batch;
        }
        if let Some(src) = &args.src {
            details.data_src = DataSource::from_str(src)
                .map_err(|e| Error::Config(format!("--src \"{}\": {:?}", src, e)))?;
        }
        if let Some(ppu) = &args.ppu {
            details.ppu = PPU::from_str(ppu)
                .map_err(|e| Error::Config(format!("--ppu \"{}\": {:?}", ppu, e)))?;
        }
        if let Some(ffu) = &args.ffu {
            details.ffu = Some(
                FFU::from_str(ffu)
                    .map_err(|e| Error::Config(format!("--ffu \"{}\": {:?}", ffu, e)))?,
            );
        }
    }

    Ok(attributes)
}

/// (De)compression of observations
fn hatanaka(rinex: &mut Rinex, args: &ProductionArgs) {
    if rinex.header.rinex_type == RinexType::ObservationData {
        if args.rnx2crx {
            rinex.rnx2crnx_mut();
        }
        if args.crx2rnx {
            rinex.crnx2rnx_mut();
        }
    }
}

/// Writes a RINEX into the workspace, with a standard file name
fn write_rinex(
    workspace: &Workspace,
    subdir: &str,
    rinex: &Rinex,
    attributes: Option<ProductionAttributes>,
    args: &ProductionArgs,
) -> Result<(), Error> {
    let suffix = if args.gzip { Some(".gz") } else { None };
    let name = rinex.standard_filename(args.short, suffix, attributes);

    let dir = workspace.subdir(subdir)?;
    let path = dir.join(&name);

    let result = if args.gzip {
        rinex.to_gzip_file(&path)
    } else {
        rinex.to_file(&path)
    };

    result.map_err(|e| Error::FileOperation(format!("{}: {}", path.display(), e)))?;
    info!("{} generated", path.display());
    Ok(())
}

fn write_sp3(workspace: &Workspace, sp3: &SP3, args: &ProductionArgs) -> Result<(), Error> {
    let mut name = sp3.standardized_filename();
    if args.gzip && !name.ends_with(".gz") {
        name.push_str(".gz");
    }

    let path = workspace.subdir("SP3")?.join(&name);

    let result = if args.gzip {
        sp3.to_gzip_file(&path)
    } else {
        sp3.to_file(&path)
    };

    result.map_err(|e| Error::FileOperation(format!("{}: {:?}", path.display(), e)))?;
    info!("{} generated", path.display());
    Ok(())
}

fn subdir(kind: ProductKind) -> &'static str {
    match kind {
        ProductKind::Observation => "OBS",
        ProductKind::Navigation => "NAV",
        ProductKind::Meteo => "METEO",
        ProductKind::Clock => "CLK",
        ProductKind::Antex => "ANTEX",
        ProductKind::SP3 => "SP3",
    }
}

fn time_window(rinex: &Rinex, start: Epoch, end: Option<Epoch>) -> Result<Rinex, Error> {
    let lower = Filter::greater_equals(&start.to_string())
        .map_err(|e| Error::FileOperation(format!("{:?}", e)))?;
    let mut window = rinex.filter(&lower);
    if let Some(end) = end {
        let upper = Filter::lower_than(&end.to_string())
            .map_err(|e| Error::FileOperation(format!("{:?}", e)))?;
        window.filter_mut(&upper);
    }
    Ok(window)
}

/// Parses and preprocesses a file designated by a file operation
fn parse_rinex(path: &Path, preprocessor: &Preprocessor) -> Result<Rinex, Error> {
    match parse(path) {
        Some(Parsed::Rinex(mut rinex)) => {
            preprocessor.rinex(&mut rinex);
            Ok(*rinex)
        },
        _ => Err(Error::FormatNotSupported(path.display().to_string())),
    }
}

pub fn run(cli: &Cli, inputs: &Inputs, workspace: &Workspace) -> Result<(), Error> {
    let products = products(inputs);
    let preprocessor = Preprocessor::new(&cli.preprocessing)?;

    match &cli.command {
        Command::Filegen(args) => {
            let args = &args.production;
            for (kind, _, rinex) in products.rinex.iter() {
                let mut rinex = (*rinex).clone();
                hatanaka(&mut rinex, args);
                let attributes = production_attributes(&rinex, args)?;
                write_rinex(workspace, subdir(*kind), &rinex, Some(attributes), args)?;
            }
            for (_, sp3) in products.sp3.iter() {
                write_sp3(workspace, sp3, args)?;
            }
            if products.rinex.is_empty() && products.sp3.is_empty() {
                warn!("nothing to generate");
            }
        },

        Command::Merge(args) => {
            let rhs = parse_rinex(&args.file, &preprocessor)?;
            let rinex_type = rhs.header.rinex_type;

            let (kind, _, lhs) = products
                .rinex
                .iter()
                .find(|(_, _, rinex)| rinex.header.rinex_type == rinex_type)
                .ok_or(Error::FileOperation(format!(
                    "no {:?} product to merge {} into",
                    rinex_type,
                    args.file.display()
                )))?;

            let mut merged = lhs
                .merge(&rhs)
                .map_err(|e| Error::FileOperation(format!("merge: {}", e)))?;

            hatanaka(&mut merged, &args.production);
            let attributes = production_attributes(&merged, &args.production)?;
            write_rinex(
                workspace,
                subdir(*kind),
                &merged,
                Some(attributes),
                &args.production,
            )?;
        },

        Command::Split(args) => {
            for (kind, _, rinex) in products.rinex.iter() {
                if *kind == ProductKind::Antex {
                    continue;
                }
                let (Some(first), Some(last)) = (rinex.first_epoch(), rinex.last_epoch()) else {
                    continue;
                };
                if args.epoch <= first || args.epoch > last {
                    warn!(
                        "{} {}: split epoch out of time frame",
                        kind,
                        rinex.standard_filename(false, None, None)
                    );
                    continue;
                }
                for (start, end) in [(first, Some(args.epoch)), (args.epoch, None)] {
                    let mut part = time_window(rinex, start, end)?;
                    hatanaka(&mut part, &args.production);
                    let attributes = production_attributes(&part, &args.production)?;
                    write_rinex(
                        workspace,
                        subdir(*kind),
                        &part,
                        Some(attributes),
                        &args.production,
                    )?;
                }
            }
        },

        Command::Tbin(args) => {
            if args.duration <= gnss_qc::prelude::Duration::ZERO {
                return Err(Error::Config("invalid (null) duration".to_string()));
            }

            for (kind, _, rinex) in products.rinex.iter() {
                if *kind == ProductKind::Antex {
                    continue;
                }
                let (Some(first), Some(last)) = (rinex.first_epoch(), rinex.last_epoch()) else {
                    continue;
                };

                let mut batch = args.production.batch.unwrap_or(0);
                let mut start = first;

                while start <= last {
                    let end = start + args.duration;
                    let mut part = time_window(rinex, start, Some(end))?;
                    hatanaka(&mut part, &args.production);

                    let mut attributes = production_attributes(&part, &args.production)?;
                    attributes
                        .v3_details
                        .get_or_insert_with(DetailedProductionAttributes::default)
                        .batch = batch;

                    write_rinex(
                        workspace,
                        &format!("BATCH/{}", subdir(*kind)),
                        &part,
                        Some(attributes),
                        &args.production,
                    )?;

                    start = end;
                    batch = batch.wrapping_add(1);
                }
            }
        },

        Command::Cbin(args) => {
            for (kind, _, rinex) in products.rinex.iter() {
                if matches!(kind, ProductKind::Antex | ProductKind::Meteo) {
                    continue;
                }

                let constellations = rinex
                    .sv_iter()
                    .map(|sv| sv.constellation)
                    .collect::<std::collections::BTreeSet<Constellation>>();

                for constellation in constellations {
                    let mask = Filter::mask(
                        MaskOperand::Equals,
                        FilterItem::ConstellationItem(vec![constellation]),
                    );

                    let mut part = rinex.filter(&mask);
                    part.header.constellation = Some(constellation);

                    if args.timescale {
                        match constellation.timescale() {
                            Some(ts) => part.timeshift_mut(ts),
                            None => {
                                warn!("{} timescale is not supported: not shifted", constellation)
                            },
                        }
                    }

                    hatanaka(&mut part, &args.production);
                    let attributes = production_attributes(&part, &args.production)?;
                    write_rinex(
                        workspace,
                        &format!("{}/{:x}", subdir(*kind), constellation),
                        &part,
                        Some(attributes),
                        &args.production,
                    )?;
                }
            }
        },

        Command::Diff(args) => {
            let rhs = parse_rinex(&args.file, &preprocessor)?;
            if rhs.header.rinex_type != RinexType::ObservationData {
                return Err(Error::FileOperation(
                    "diff only applies to observation RINEX".to_string(),
                ));
            }

            let (_, _, lhs) = products
                .rinex
                .iter()
                .find(|(kind, _, _)| *kind == ProductKind::Observation)
                .ok_or(Error::FileOperation(
                    "diff requires an observation RINEX (A)".to_string(),
                ))?;

            let mut diff = lhs
                .observations_substract(&rhs)
                .map_err(|e| Error::FileOperation(format!("diff: {:?}", e)))?;

            hatanaka(&mut diff, &args.production);
            let attributes = production_attributes(&diff, &args.production)?;
            write_rinex(workspace, "DIFF", &diff, Some(attributes), &args.production)?;
        },

        _ => unreachable!("not a file operation"),
    }

    Ok(())
}
