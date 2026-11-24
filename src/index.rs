//! index command: presents the input products, as indexed by GNSS-Qc
use std::io::Write;

use itertools::Itertools;

use crate::{cli::IndexArgs, error::Error, input::Inputs};

/// Human readable duration
pub fn format_duration(duration: Option<gnss_qc::prelude::Duration>) -> String {
    match duration {
        Some(duration) => duration.to_string(),
        None => "-".to_string(),
    }
}

/// Human readable epoch
pub fn format_epoch(epoch: Option<gnss_qc::prelude::Epoch>) -> String {
    match epoch {
        Some(epoch) => epoch.to_string(),
        None => "-".to_string(),
    }
}

pub fn run(inputs: &Inputs, args: &IndexArgs) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    if args.json {
        serde_json::to_writer_pretty(&mut out, &inputs.manifest)?;
        writeln!(out)?;
        return Ok(());
    }

    for entry in inputs.manifest.iter() {
        writeln!(out, "[{}] {}", entry.kind, entry.identity)?;

        writeln!(
            out,
            "    period      {} → {} ({})",
            format_epoch(entry.first),
            format_epoch(entry.last),
            format_duration(entry.duration()),
        )?;

        writeln!(out, "    sampling    {}", format_duration(entry.sampling))?;

        if !entry.satellites.is_empty() {
            writeln!(
                out,
                "    {} satellites ({})",
                entry.satellites.len(),
                entry
                    .constellations()
                    .iter()
                    .map(|c| format!("{:x}", c))
                    .unique()
                    .join(", ")
            )?;
        }

        if let Some((x, y, z)) = entry.position_ecef_m {
            writeln!(out, "    position    ECEF {:.3} {:.3} {:.3} m", x, y, z)?;
        }

        for (key, value) in entry.details.iter() {
            writeln!(out, "    {:<11} {}", key, value)?;
        }

        for file in entry.files.iter() {
            writeln!(out, "    · {}", file.display())?;
        }

        writeln!(out)?;
    }

    writeln!(
        out,
        "Designate a product by its identity, for example: --rover \"{}\"",
        inputs
            .manifest
            .first()
            .map(|entry| entry.identity.to_string())
            .unwrap_or_default()
    )?;

    Ok(())
}
