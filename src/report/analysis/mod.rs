//! Accumulators: analyze the output stream, build the report sections.
mod combinations;
pub mod context;
mod meteo;
mod residuals;
mod signals;

#[cfg(feature = "navigation")]
mod navigation;

#[cfg(feature = "cggtts")]
mod cggtts;

#[cfg(feature = "ionex")]
mod ipp;

#[cfg(feature = "ionex")]
mod tec;

#[cfg(feature = "ionex")]
mod obs2tec;

use gnss_qc::prelude::QcOutputItem;

use crate::{engine::SessionContext, report::Section, sink::RunSummary};

/// Analysis of the output stream
pub trait Analysis {
    /// Consumes one result
    fn consume(&mut self, item: &QcOutputItem);

    /// Report sections, at the end of the stream
    fn sections(&mut self, context: &SessionContext, run: &RunSummary) -> Vec<Section>;
}

/// Analyses that apply to this command
pub fn for_command(context: &SessionContext) -> Vec<Box<dyn Analysis>> {
    let mut analyses: Vec<Box<dyn Analysis>> = Vec::new();

    match context.command.as_str() {
        "qc" => analyses.push(Box::new(signals::SignalAnalysis::new(context))),
        "residuals obs" => analyses.push(Box::new(residuals::ObservationResiduals::default())),
        "residuals orbit" => analyses.push(Box::new(residuals::OrbitResiduals::default())),
        "residuals clock" => analyses.push(Box::new(residuals::ClockResiduals::default())),
        "meteo" => analyses.push(Box::new(meteo::MeteoAnalysis::default())),
        "combinations" => analyses.push(Box::new(combinations::CombinationAnalysis::new(context))),
        #[cfg(feature = "ionex")]
        "tec" => analyses.push(Box::new(tec::TecAnalysis::default())),
        #[cfg(feature = "navigation")]
        "nav" => analyses.push(Box::new(navigation::NavigationAnalysis::new(context))),
        #[cfg(feature = "cggtts")]
        "cggtts" => {
            analyses.push(Box::new(cggtts::CggttsAnalysis::default()));
            analyses.push(Box::new(navigation::NavigationAnalysis::new(context)));
        },
        #[cfg(feature = "ionex")]
        "ipp" => analyses.push(Box::new(ipp::IppAnalysis::new(context))),
        #[cfg(feature = "ionex")]
        "obs2tec" => analyses.push(Box::new(obs2tec::Obs2TecAnalysis::new(context))),
        _ => {},
    }

    // navigation: signal combinations of the rover (and base)
    if context.combinations && context.command != "combinations" {
        analyses.push(Box::new(combinations::CombinationAnalysis::new(context)));
    }

    // processing commands: the quality control of the observations follows
    if context.qc && context.command != "qc" {
        analyses.push(Box::new(signals::SignalAnalysis::new(context)));
    }

    analyses
}
