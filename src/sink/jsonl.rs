//! JSON lines: the complete output stream, one result per line.
//! Each line deserializes into a gnss-qc `QcOutputItem` (serde feature).
use std::io::Write;

use log::info;

use gnss_qc::prelude::{QcOutput, QcOutputItem};

use crate::{
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

pub struct JsonlSink {
    writer: Box<dyn Write>,
    description: String,
    raw: bool,
}

impl JsonlSink {
    /// results.jsonl, within the session workspace
    pub fn file(workspace: &Workspace, raw: bool) -> Result<Self, Error> {
        let name = "data/results.jsonl";
        Ok(Self {
            writer: Box::new(workspace.create_file(name)?),
            description: workspace.path(name).display().to_string(),
            raw,
        })
    }

    /// Standard output
    pub fn stdout(raw: bool) -> Self {
        Self {
            writer: Box::new(std::io::BufWriter::new(std::io::stdout())),
            description: String::new(),
            raw,
        }
    }
}

impl Consumer for JsonlSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        let skip = match &item.payload {
            QcOutput::Signal(_) => !self.raw,
            // ephemeris frames do not round trip (serde)
            QcOutput::Ephemeris(_) => true,
            _ => false,
        };

        if !skip {
            serde_json::to_writer(&mut self.writer, item)?;
            self.writer.write_all(b"\n")?;
        }

        Ok(())
    }

    fn finish(&mut self, _: &RunSummary) -> Result<(), Error> {
        self.writer.flush()?;
        if !self.description.is_empty() {
            info!("{} generated", self.description);
        }
        Ok(())
    }
}
