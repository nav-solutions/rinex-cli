use clap::ArgMatches;
use gnss_qc::prelude::ProductType;
use rinex::prelude::{processing::Split, Epoch};
use std::path::Path;

use crate::{cli::Context, fops::dump_rinex_auto_generated_name, Error};

/// Splits input files at specified Time Instant
pub fn split(ctx: &Context, submatches: &ArgMatches) -> Result<(), Error> {
    let ctx_data = &ctx.data;

    let gzip = submatches.get_flag("gzip");
    let forced_short_v2 = submatches.get_flag("short");

    let split_instant = submatches
        .get_one::<Epoch>("split")
        .expect("split epoch is required");

    for product in [
        ProductType::Observation,
        ProductType::MeteoObservation,
        ProductType::BroadcastNavigation,
        ProductType::HighPrecisionClock,
    ] {
        if let Some(rinex) = ctx_data.rinex(product) {
            let (rinex_a, rinex_b) = rinex.split(*split_instant);

            let input_name = rinex_a.standard_filename(forced_short_v2, None, None);
            let input_path = Path::new(&input_name);
            dump_rinex_auto_generated_name(&ctx, &input_path, &rinex_a, gzip, None);

            let input_name = rinex_b.standard_filename(forced_short_v2, None, None);
            let input_path = Path::new(&input_name);
            dump_rinex_auto_generated_name(&ctx, &input_path, &rinex_b, gzip, None);
        }
    }
    Ok(())
}
