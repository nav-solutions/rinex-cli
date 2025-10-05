use clap::ArgMatches;
use gnss_qc::prelude::ProductType;

#[cfg(feature = "csv")]
use crate::fops::csv::dump_context_csv;

use crate::{
    cli::Context,
    fops::{custom_prod_attributes, output_filename},
    Error,
};

/// Dumps current (possibly preprocessed) context
/// into either RINEX / SP3 format (maintaining consistent format) or CSV
pub fn filegen(ctx: &Context, matches: &ArgMatches, submatches: &ArgMatches) -> Result<(), Error> {
    #[cfg(feature = "csv")]
    if submatches.get_flag("csv") {
        dump_context_csv(ctx, matches, submatches)?;
        return Ok(());
    }

    #[cfg(not(feature = "csv"))]
    if submatches.get_flag("csv") {
        panic!("Not available. Requires `csv` compilation flag.");
    }

    write(ctx, matches, submatches)?;
    Ok(())
}

fn write(ctx: &Context, matches: &ArgMatches, submatches: &ArgMatches) -> Result<(), Error> {
    let ctx_data = &ctx.data;
    for (product, dir) in [
        (ProductType::Observation, "OBSERVATIONS"),
        (ProductType::MeteoObservation, "METEO"),
        (ProductType::BroadcastNavigation, "BRDC"),
        (ProductType::HighPrecisionClock, "CLOCK"),
        (ProductType::HighPrecisionOrbit, "SP3"),
        (ProductType::ANTEX, "ANTEX"),
    ] {
        if let Some(rinex) = ctx_data.rinex(product) {
            ctx.workspace.create_subdir(dir);
            let prod = custom_prod_attributes(rinex, submatches);
            let filename = output_filename(rinex, matches, submatches, prod);

            let output_path = ctx
                .workspace
                .root
                .join(dir)
                .join(filename)
                .to_string_lossy()
                .to_string();

            if submatches.get_flag("gzip") {
                rinex.to_gzip_file(&output_path).unwrap_or_else(|_| {
                    panic!("failed to generate {} RINEX \"{}\"", product, output_path)
                });
            } else {
                rinex.to_file(&output_path).unwrap_or_else(|_| {
                    panic!("failed to generate {} RINEX \"{}\"", product, output_path)
                });
            }

            info!("{} RINEX \"{}\" has been generated", product, output_path);
        }
    }
    Ok(())
}
