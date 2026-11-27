//! Consumers of the GNSS-Qc output stream: data files.
//!
//! The engine produces a single stream of time tagged results ([QcOutputItem]).
//! Every [Consumer] receives every result: data files (CSV, JSON lines),
//! dedicated formats (CGGTTS, GPX, KML) and reports.
mod csv;
mod jsonl;

#[cfg(feature = "cggtts")]
mod cggtts;

#[cfg(feature = "navigation")]
mod tracks;

#[cfg(feature = "ionex")]
mod ionex;

#[cfg(feature = "ionex")]
mod obs2tec;

pub use csv::CsvSink;
pub use jsonl::JsonlSink;

#[cfg(feature = "cggtts")]
pub use cggtts::{CggttsHeader, CggttsSink};

#[cfg(feature = "navigation")]
pub use tracks::{TrackFormat, TrackSink};

#[cfg(feature = "ionex")]
pub use ionex::{period_code, IonexSink};

#[cfg(feature = "ionex")]
pub use obs2tec::Obs2TecIonexSink;

use gnss_qc::prelude::{Epoch, QcEngineStats, QcOutputItem};

use crate::error::Error;

/// Summary of a run, presented to the consumers once the stream ended
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RunSummary {
    /// Engine statistics
    pub stats: QcEngineStats,

    /// Number of results
    pub results: usize,

    /// First result epoch
    pub first: Option<Epoch>,

    /// Last result epoch
    pub last: Option<Epoch>,

    /// Processing duration (s)
    pub processing_time_s: f64,
}

/// Consumer of the output stream
pub trait Consumer {
    /// Consumes one result
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error>;

    /// End of stream
    fn finish(&mut self, _summary: &RunSummary) -> Result<(), Error> {
        Ok(())
    }
}
