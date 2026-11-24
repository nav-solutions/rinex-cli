//! summary.json: session description, engine statistics and end of session reports
//! (navigation, CGGTTS), for automation.
use std::io::Write;

use log::info;
use serde::Serialize;

use gnss_qc::prelude::QcOutputItem;

#[cfg(feature = "navigation")]
use gnss_qc::prelude::QcOutput;

use crate::{
    engine::SessionContext,
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

#[derive(Serialize)]
struct Summary<'a> {
    rinex_cli: &'static str,
    context: &'a SessionContext,
    run: &'a RunSummary,
    reports: &'a [QcOutputItem],
}

pub struct SummarySink {
    workspace: Workspace,
    context: SessionContext,
    reports: Vec<QcOutputItem>,
}

impl SummarySink {
    pub fn new(workspace: &Workspace, context: &SessionContext) -> Self {
        Self {
            workspace: workspace.clone(),
            context: context.clone(),
            reports: Vec::new(),
        }
    }
}

impl Consumer for SummarySink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        #[cfg(feature = "navigation")]
        if matches!(item.payload, QcOutput::NavigationReport(_)) {
            self.reports.push(item.clone());
        }

        #[cfg(feature = "cggtts")]
        if matches!(item.payload, QcOutput::CggttsReport(_)) {
            self.reports.push(item.clone());
        }

        let _ = item;
        Ok(())
    }

    fn finish(&mut self, run: &RunSummary) -> Result<(), Error> {
        let summary = Summary {
            rinex_cli: env!("CARGO_PKG_VERSION"),
            context: &self.context,
            run,
            reports: &self.reports,
        };

        let mut fd = self.workspace.create_file("summary.json")?;
        serde_json::to_writer_pretty(&mut fd, &summary)?;
        fd.flush()?;

        info!(
            "{} generated",
            self.workspace.path("summary.json").display()
        );
        Ok(())
    }
}
