use clap::ArgMatches;
use std::path::{Path, PathBuf};

use rinex::prelude::{qc::Merge, RinexType};

use crate::{
    cli::{Cli, Context},
    fops::{dump_rinex_auto_generated_name, parse_rinex},
    preprocessing::rinex_preprocessing,
    Error,
};

/// Merge single file into [Context], dump into workspace.
pub fn merge(ctx: &Context, cli: &Cli, submatches: &ArgMatches) -> Result<(), Error> {
    let ctx_data = &ctx.data;

    // options
    let gzip = submatches.get_flag("gzip");
    let short_v2_name = submatches.get_flag("short");
    let merge_path = submatches.get_one::<PathBuf>("file").unwrap();

    let forced_rinex = cli.matches.get_flag("crx2rnx");
    let forced_crinex = cli.matches.get_flag("rnx2crx");

    let mut rinex_b = parse_rinex(&merge_path);

    rinex_preprocessing(&mut rinex_b, cli);

    if forced_rinex {
        rinex_b.crnx2rnx_mut();
    }

    if forced_crinex {
        rinex_b.rnx2crnx_mut();
    }

    // perform merge
    let (origin_name, rinex_c) = match rinex_b.header.rinex_type {
        RinexType::ObservationData => {
            let rinex_a = ctx_data
                .observation()
                .ok_or(Error::MissingObservationRinex)?;

            (
                rinex_a.standard_filename(short_v2_name, None, None),
                rinex_a.merge(&rinex_b)?,
            )
        },
        RinexType::NavigationData => {
            let rinex_a = ctx_data
                .brdc_navigation()
                .ok_or(Error::MissingNavigationRinex)?;

            (
                rinex_a.standard_filename(short_v2_name, None, None),
                rinex_a.merge(&rinex_b)?,
            )
        },
        RinexType::MeteoData => {
            let rinex_a = ctx_data.meteo().ok_or(Error::MissingMeteoRinex)?;

            (
                rinex_a.standard_filename(short_v2_name, None, None),
                rinex_a.merge(&rinex_b)?,
            )
        },
        RinexType::ClockData => {
            let rinex_a = ctx_data.clock().ok_or(Error::MissingClockRinex)?;

            (
                rinex_a.standard_filename(short_v2_name, None, None),
                rinex_a.merge(&rinex_b)?,
            )
        },
        rinex_format => panic!("merge is not available for {}", rinex_format),
    };

    let input_path = Path::new(&origin_name);
    dump_rinex_auto_generated_name(&ctx, &input_path, &rinex_c, gzip, None);

    Ok(())
}
