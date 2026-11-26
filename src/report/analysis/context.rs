//! Session section: inputs, configuration, engine statistics
use itertools::Itertools;

use crate::{
    engine::SessionContext,
    index::{format_duration, format_epoch},
    report::{Block, Section, Table},
    sink::RunSummary,
};

pub fn sections(context: &SessionContext, run: &RunSummary) -> Section {
    let mut section = Section::new("session", "Session");

    if !context.setup.is_empty() {
        section.push(Block::Heading("Configuration".to_string()));
        section.push(Block::KeyValue(context.setup.clone()));
    }

    section.push(Block::Heading("Input products".to_string()));

    let mut table = Table::new(&[
        "Type",
        "Identity",
        "Period",
        "Sampling",
        "Satellites",
        "Files",
    ]);

    for entry in context.manifest.iter() {
        table.push(vec![
            entry.kind.to_string(),
            entry.identity.to_string(),
            format!(
                "{} → {}",
                format_epoch(entry.first),
                format_epoch(entry.last)
            ),
            format_duration(entry.sampling),
            if entry.satellites.is_empty() {
                "-".to_string()
            } else {
                format!(
                    "{} ({})",
                    entry.satellites.len(),
                    entry
                        .constellations()
                        .iter()
                        .map(|c| format!("{:x}", c))
                        .unique()
                        .join(", ")
                )
            },
            entry
                .files
                .iter()
                .map(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default()
                })
                .join(", "),
        ]);
    }

    section.push(Block::Table(table));

    section.push(Block::Heading("Processing".to_string()));
    section.push(Block::KeyValue(vec![
        ("Epochs".to_string(), run.stats.epochs.to_string()),
        ("Input items".to_string(), run.stats.items.to_string()),
        ("Results".to_string(), run.results.to_string()),
        (
            "Late observations (dropped)".to_string(),
            run.stats.late_items.to_string(),
        ),
        ("First result".to_string(), format_epoch(run.first)),
        ("Last result".to_string(), format_epoch(run.last)),
        (
            "Processing time".to_string(),
            format!("{:.1} s", run.processing_time_s),
        ),
        ("Command line".to_string(), context.command_line.clone()),
        (
            "Software".to_string(),
            format!(
                "rinex-cli v{}, gnss-qc v0.8",
                env!("CARGO_PKG_VERSION")
            ),
        ),
    ]));

    section
}
