//! Reports.
//!
//! The output stream is analyzed by accumulators ([analysis]), which build a
//! format agnostic [Report] (sections made of text, tables and charts) at the
//! end of the session. Renderers then format the report: HTML (interactive),
//! PDF (static, pdf feature).
pub mod analysis;
mod html;
mod stats;

#[cfg(feature = "pdf")]
mod pdf;
#[cfg(feature = "pdf")]
mod svg;

pub use stats::Stats;

use log::info;

use gnss_qc::prelude::{Duration, Epoch, QcOutputItem, TimeScale};

use crate::{
    engine::SessionContext,
    error::Error,
    sink::{Consumer, RunSummary},
    workspace::Workspace,
};

use analysis::Analysis;

/// Report: a title and sections
#[derive(Debug, Clone)]
pub struct Report {
    pub title: String,
    pub subtitle: String,
    pub generated: String,
    pub sections: Vec<Section>,
}

/// Report section
#[derive(Debug, Clone)]
pub struct Section {
    /// Unique identifier (anchors)
    pub id: String,
    pub title: String,
    pub blocks: Vec<Block>,
}

impl Section {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect(),
            title: title.to_string(),
            blocks: Vec::new(),
        }
    }

    pub fn push(&mut self, block: Block) {
        self.blocks.push(block);
    }
}

/// Report content
#[derive(Debug, Clone)]
pub enum Block {
    /// Sub title
    Heading(String),
    /// Paragraph
    Text(String),
    /// Highlighted message
    Notice(String),
    /// Key / value pairs
    KeyValue(Vec<(String, String)>),
    /// Table
    Table(Table),
    /// Chart
    Chart(Chart),
    /// Color map over a regular grid (one frame per epoch)
    #[cfg_attr(not(feature = "ionex"), allow(dead_code))]
    Heatmap(Heatmap),
}

/// One frame of a [Heatmap]
#[derive(Debug, Clone)]
pub struct HeatmapFrame {
    /// Frame label (epoch)
    pub label: String,
    /// Values: one row per y value, one column per x value (NaN: no value)
    pub z: Vec<Vec<f64>>,
}

/// Color map over a regular grid, with possibly several frames sharing the same scale
#[derive(Debug, Clone)]
pub struct Heatmap {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub z_label: String,
    /// Grid x values (increasing)
    pub x: Vec<f64>,
    /// Grid y values (increasing)
    pub y: Vec<f64>,
    pub frames: Vec<HeatmapFrame>,
}

impl Heatmap {
    /// Range of the values, over all frames
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub fn range(&self) -> (f64, f64) {
        let values = self
            .frames
            .iter()
            .flat_map(|f| f.z.iter().flatten())
            .copied()
            .filter(|v| v.is_finite());
        let (min, max) = values.fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));
        if min > max {
            (0.0, 1.0)
        } else {
            (min, max)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }
}

/// X axis of a chart
#[derive(Debug, Clone, Copy)]
pub enum XAxis {
    /// Time axis: x values are seconds elapsed since this origin
    Time(Epoch),
    /// Linear axis
    #[cfg_attr(not(any(feature = "navigation", feature = "ionex")), allow(dead_code))]
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChartKind {
    /// Lines
    Lines,
    /// Markers
    Markers,
    /// Geographic map: x is longitude, y is latitude (degrees)
    Map,
}

#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Points connected by a line, whatever the kind of chart (segments, baselines..)
    pub connected: bool,
}

impl Series {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            x: Vec::new(),
            y: Vec::new(),
            connected: false,
        }
    }

    /// Points connected by a line, whatever the kind of chart
    #[allow(dead_code)] // only used by the navigation reports
    pub fn connected(mut self) -> Self {
        self.connected = true;
        self
    }

    pub fn push(&mut self, x: f64, y: f64) {
        self.x.push(x);
        self.y.push(y);
    }

    /// Uniform decimation down to (at most) `max` points
    pub fn decimated(&self, max: usize) -> Series {
        if self.x.len() <= max || max == 0 {
            return self.clone();
        }
        let step = self.x.len().div_ceil(max);
        Series {
            name: self.name.clone(),
            x: self.x.iter().step_by(step).copied().collect(),
            y: self.y.iter().step_by(step).copied().collect(),
            connected: self.connected,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chart {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub x_axis: XAxis,
    pub kind: ChartKind,
    /// Same scale on both axes (horizontal scatter plots)
    pub equal_axes: bool,
    pub series: Vec<Series>,
}

impl Chart {
    /// Time series chart
    pub fn time(title: &str, y_label: &str, origin: Epoch) -> Self {
        Self {
            title: title.to_string(),
            x_label: format!("{}", origin.time_scale),
            y_label: y_label.to_string(),
            x_axis: XAxis::Time(origin),
            kind: ChartKind::Lines,
            equal_axes: false,
            series: Vec::new(),
        }
    }

    /// X/Y chart
    #[cfg_attr(not(any(feature = "navigation", feature = "ionex")), allow(dead_code))]
    pub fn xy(title: &str, x_label: &str, y_label: &str) -> Self {
        Self {
            title: title.to_string(),
            x_label: x_label.to_string(),
            y_label: y_label.to_string(),
            x_axis: XAxis::Linear,
            kind: ChartKind::Markers,
            equal_axes: false,
            series: Vec::new(),
        }
    }

    pub fn with_kind(mut self, kind: ChartKind) -> Self {
        self.kind = kind;
        self
    }

    #[cfg_attr(not(feature = "navigation"), allow(dead_code))]
    pub fn with_equal_axes(mut self) -> Self {
        self.equal_axes = true;
        self
    }

    pub fn push(&mut self, series: Series) {
        if !series.x.is_empty() {
            self.series.push(series);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }
}

/// Seconds elapsed since origin
pub fn elapsed_s(origin: Epoch, t: Epoch) -> f64 {
    (t - origin).to_seconds()
}

/// Time tag of a time axis value, in the timescale of the origin
pub fn time_tag(origin: Epoch, x_s: f64) -> String {
    let t = origin + Duration::from_seconds(x_s);
    let ts = if origin.time_scale == TimeScale::TAI {
        TimeScale::UTC
    } else {
        origin.time_scale
    };
    let (y, m, d, hh, mm, ss, ns) = t.to_gregorian(ts);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        y,
        m,
        d,
        hh,
        mm,
        ss,
        ns / 1_000_000
    )
}

/// Report formats to generate
#[derive(Debug, Clone, Copy, Default)]
pub struct Formats {
    pub html: bool,
    pub pdf: bool,
}

/// Consumer that analyzes the stream and generates the reports
pub struct ReportSink {
    workspace: Workspace,
    context: SessionContext,
    formats: Formats,
    analyses: Vec<Box<dyn Analysis>>,
}

impl ReportSink {
    pub fn new(workspace: &Workspace, context: &SessionContext, formats: Formats) -> Self {
        Self {
            workspace: workspace.clone(),
            context: context.clone(),
            formats,
            analyses: analysis::for_command(context),
        }
    }

    /// Builds the report
    fn build(&mut self, run: &RunSummary) -> Report {
        let mut sections = Vec::new();

        for analysis in self.analyses.iter_mut() {
            sections.extend(analysis.sections(&self.context, run));
        }

        sections.push(analysis::context::sections(&self.context, run));

        let generated = Epoch::now()
            .map(|now| {
                let (y, m, d, hh, mm, _, _) = now.to_gregorian_utc();
                format!("{:04}-{:02}-{:02} {:02}:{:02} UTC", y, m, d, hh, mm)
            })
            .unwrap_or_default();

        Report {
            title: self.context.title.clone(),
            subtitle: format!("rinex-cli {}", self.context.command),
            generated,
            sections,
        }
    }
}

impl Consumer for ReportSink {
    fn consume(&mut self, item: &QcOutputItem) -> Result<(), Error> {
        for analysis in self.analyses.iter_mut() {
            analysis.consume(item);
        }
        Ok(())
    }

    fn finish(&mut self, run: &RunSummary) -> Result<(), Error> {
        let report = self.build(run);

        if self.formats.html {
            let name = "report.html";
            let html = html::render(&report);
            std::fs::write(self.workspace.path(name), html)?;
            info!("{} generated", self.workspace.path(name).display());
        }

        if self.formats.pdf {
            #[cfg(feature = "pdf")]
            {
                let name = "report.pdf";
                let pdf = pdf::render(&report)?;
                std::fs::write(self.workspace.path(name), pdf)?;
                info!("{} generated", self.workspace.path(name).display());
            }

            #[cfg(not(feature = "pdf"))]
            log::error!(
                "PDF report not available: rinex-cli was built without the \"pdf\" feature"
            );
        }

        Ok(())
    }
}
