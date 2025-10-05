use crate::cli::Context;
use crate::fops::custom_prod_attributes;
use crate::fops::output_filename;
use crate::Error;
use clap::ArgMatches;
use gnss_qc::prelude::{Filter, Preprocessing, ProductType};
use rinex::prelude::Duration;
use rinex::prod::DetailedProductionAttributes;

/// Time binning (batch design)
pub fn time_binning(
    ctx: &Context,
    matches: &ArgMatches,
    submatches: &ArgMatches,
) -> Result<(), Error> {
    let ctx_data = &ctx.data;

    let gzip = submatches.get_flag("gzip");

    let duration = submatches
        .get_one::<Duration>("interval")
        .expect("duration is required");

    if *duration == Duration::ZERO {
        panic!("invalid (null) duration");
    }

    ctx.workspace.create_subdir("BATCH");

    for product in [
        ProductType::Observation,
        ProductType::MeteoObservation,
        ProductType::BroadcastNavigation,
        ProductType::HighPrecisionClock,
    ] {
        // input data determination
        if let Some(rinex) = ctx_data.rinex(product) {
            // time frame determination
            let (mut first, end) = (
                rinex
                    .first_epoch()
                    .expect("failed to determine first epoch"),
                rinex.last_epoch().expect("failed to determine last epoch"),
            );

            let mut last = first + *duration;

            // production attributes: initialize Batch counter
            let mut prod = custom_prod_attributes(rinex, submatches);

            if let Some(ref mut details) = prod.v3_details {
                details.batch = 0_u8;
            } else {
                let mut details = DetailedProductionAttributes::default();
                details.batch = 0_u8;
                prod.v3_details = Some(details);
            };

            // run time binning algorithm
            while last <= end {
                let lower = Filter::lower_than(&last.to_string()).unwrap();
                let greater = Filter::greater_equals(&first.to_string()).unwrap();
                let batched = rinex.filter(&lower).filter(&greater);

                // generate standardized name
                let filename = output_filename(&batched, matches, submatches, prod.clone());

                let output = ctx
                    .workspace
                    .root
                    .join("BATCH")
                    .join(&filename)
                    .to_string_lossy()
                    .to_string();

                if gzip {
                    batched
                        .to_gzip_file(&output)
                        .unwrap_or_else(|e| panic!("Failed to format RINEX {}: {}", output, e));
                } else {
                    batched
                        .to_file(&output)
                        .unwrap_or_else(|e| panic!("Failed to format RINEX {}: {}", output, e));
                }

                first += *duration;
                last += *duration;

                if let Some(ref mut details) = prod.v3_details {
                    details.batch += 1;
                }
            }
        }
    }
    Ok(())
}
