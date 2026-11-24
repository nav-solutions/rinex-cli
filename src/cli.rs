//! Command line interface
use std::{path::PathBuf, str::FromStr};

use clap::{Args, Parser, Subcommand, ValueEnum};

use gnss_qc::prelude::{Epoch, QcIndexingConfig};

use crate::error::Error;

#[derive(Debug, Parser)]
#[command(
    name = "rinex-cli",
    author,
    version,
    about = "GNSS post processing and quality control, powered by GNSS-Qc",
    long_about = "RINEX-Cli deploys the GNSS-Qc engine on your data:
input products are indexed (receivers, agencies, aliases), synchronized,
processed (quality control, navigation, CGGTTS, residuals..) and the results
are written into a session workspace: data files (CSV, JSON lines, CGGTTS..)
and reports (HTML, PDF).

Start with the index command, to verify how your files are identified:

  rinex-cli -d DATA/ index"
)]
pub struct Cli {
    #[command(flatten)]
    pub inputs: InputArgs,

    #[command(flatten)]
    pub preprocessing: PreprocessingArgs,

    #[command(flatten)]
    pub outputs: OutputArgs,

    /// Opmode
    #[command(subcommand)]
    pub command: Command,
}

/// Input products definition and indexing
#[derive(Debug, Args)]
#[command(next_help_heading = "Inputs")]
pub struct InputArgs {
    /// Load one file: RINEX, CRINEX, SP3, gzip compressed or not. Repeat as needed.
    /// Prefix with ALIAS= to index this file (and merge it) under a custom alias,
    /// for example: -f base=MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz
    #[arg(short = 'f', long = "file", value_name = "[ALIAS=]FILE", global = true)]
    pub files: Vec<String>,

    /// Load every file of a directory (recursively). Repeat as needed.
    /// Prefix with ALIAS= to index every file of this directory under a custom alias.
    #[arg(short = 'd', long = "dir", value_name = "[ALIAS=]DIR", global = true)]
    pub directories: Vec<String>,

    /// Maximal recursion depth of -d
    #[arg(long, default_value_t = 5, global = true)]
    pub depth: usize,

    /// Indexing method of observation RINEX (signal sources):
    /// receiver (default), antenna, operator, filename, auto, alias:NAME
    #[arg(long, value_name = "METHOD", global = true)]
    pub obs_index: Option<IndexingMethod>,

    /// Indexing method of navigation RINEX: agency (default), filename, auto, alias:NAME
    #[arg(long, value_name = "METHOD", global = true)]
    pub nav_index: Option<IndexingMethod>,

    /// Indexing method of SP3 files: agency (default), filename, auto, alias:NAME
    #[arg(long, value_name = "METHOD", global = true)]
    pub sp3_index: Option<IndexingMethod>,

    /// Indexing method of meteo RINEX: agency (default), filename, auto, alias:NAME
    #[arg(long, value_name = "METHOD", global = true)]
    pub meteo_index: Option<IndexingMethod>,

    /// Indexing method of clock RINEX: agency (default), filename, auto, alias:NAME
    #[arg(long, value_name = "METHOD", global = true)]
    pub clk_index: Option<IndexingMethod>,

    /// Input preferences (QcInputConfig) as JSON file.
    /// The --*-index options take precedence.
    #[arg(long, value_name = "JSON", global = true)]
    pub input_cfg: Option<PathBuf>,

    /// Processing window: first epoch (for example "2020-06-25T00:00:00 GPST")
    #[arg(long, value_name = "EPOCH", global = true)]
    pub start: Option<Epoch>,

    /// Processing window: last epoch
    #[arg(long, value_name = "EPOCH", global = true)]
    pub end: Option<Epoch>,
}

/// Preprocessing, applied to each file before indexing
#[derive(Debug, Default, Args)]
#[command(next_help_heading = "Preprocessing")]
pub struct PreprocessingArgs {
    /// Filter out GPS
    #[arg(short = 'G', global = true)]
    pub no_gps: bool,

    /// Filter out Glonass
    #[arg(short = 'R', global = true)]
    pub no_glonass: bool,

    /// Filter out Galileo
    #[arg(short = 'E', global = true)]
    pub no_galileo: bool,

    /// Filter out BeiDou
    #[arg(short = 'C', global = true)]
    pub no_beidou: bool,

    /// Filter out BeiDou GEO satellites
    #[arg(long = "CG", global = true)]
    pub no_beidou_geo: bool,

    /// Filter out QZSS
    #[arg(short = 'J', global = true)]
    pub no_qzss: bool,

    /// Filter out IRNSS
    #[arg(short = 'I', global = true)]
    pub no_irnss: bool,

    /// Filter out SBAS
    #[arg(short = 'S', global = true)]
    pub no_sbas: bool,

    /// Preprocessing filter (GNSS-Qc filter designer), ';' separated.
    /// For example: -P "GPS;>E05" or -P "decim:30s"
    #[arg(short = 'P', value_delimiter = ';', global = true)]
    pub filters: Vec<String>,

    /// Remove null observations (zero repair)
    #[arg(short = 'z', long = "zero-repair", global = true)]
    pub zero_repair: bool,
}

/// Report format
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    /// Interactive, self-contained HTML page
    Html,
    /// Static PDF document (requires the pdf feature)
    Pdf,
    /// No report
    None,
}

/// Data format
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DataFormat {
    /// One CSV file per kind of result
    Csv,
    /// The complete stream of results, one JSON object per line
    Jsonl,
    /// No data file
    None,
}

/// Outputs definition
#[derive(Debug, Args)]
#[command(next_help_heading = "Outputs")]
pub struct OutputArgs {
    /// Output folder. Each session is written in its own subfolder.
    #[arg(
        short = 'o',
        long = "output",
        value_name = "DIR",
        env = "RINEX_WORKSPACE",
        default_value = "WORKSPACE",
        global = true
    )]
    pub output: PathBuf,

    /// Session name (subfolder). Default: name of the primary input file.
    #[arg(long, value_name = "NAME", global = true)]
    pub session: Option<String>,

    /// Report formats, comma separated: html (default), pdf, none
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "html",
        global = true
    )]
    pub report: Vec<ReportFormat>,

    /// Data formats, comma separated: csv (default), jsonl, none
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "csv",
        global = true
    )]
    pub data: Vec<DataFormat>,

    /// Also export the raw signal observations (large)
    #[arg(long, global = true)]
    pub raw: bool,

    /// Stream the results on stdout (JSON lines). Logs go to stderr.
    #[arg(long, global = true)]
    pub stdout: bool,

    /// Report title. Default: session name.
    #[arg(long, global = true)]
    pub title: Option<String>,

    /// Open the report once generated
    #[arg(long, global = true)]
    pub open: bool,

    /// Only report warnings and errors
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,
}

impl OutputArgs {
    pub fn html(&self) -> bool {
        self.report.contains(&ReportFormat::Html)
    }

    pub fn pdf(&self) -> bool {
        self.report.contains(&ReportFormat::Pdf)
    }

    pub fn csv(&self) -> bool {
        self.data.contains(&DataFormat::Csv)
    }

    pub fn jsonl(&self) -> bool {
        self.data.contains(&DataFormat::Jsonl)
    }
}

/// Indexing method, as selected on the command line
#[derive(Debug, Clone, PartialEq)]
pub enum IndexingMethod {
    /// Automatic indexing
    Auto,
    /// One of the GNSS-Qc presets
    Preset(QcIndexingConfig),
}

impl FromStr for IndexingMethod {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let method = s.trim();
        match method.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "receiver" => Ok(Self::Preset(QcIndexingConfig::Receiver)),
            "antenna" => Ok(Self::Preset(QcIndexingConfig::Antenna)),
            "operator" => Ok(Self::Preset(QcIndexingConfig::Operator)),
            "agency" => Ok(Self::Preset(QcIndexingConfig::Agency)),
            "filename" | "file" => Ok(Self::Preset(QcIndexingConfig::FileName)),
            _ => {
                let alias = method
                    .strip_prefix("alias:")
                    .or_else(|| method.strip_prefix("custom:"))
                    .filter(|alias| !alias.is_empty())
                    .ok_or(Error::InvalidIndexingMethod(method.to_string()))?;
                Ok(Self::Preset(QcIndexingConfig::Custom(alias.to_string())))
            },
        }
    }
}

impl IndexingMethod {
    /// Converts to a GNSS-Qc preference
    pub fn to_config(&self) -> Option<QcIndexingConfig> {
        match self {
            Self::Auto => None,
            Self::Preset(cfg) => Some(cfg.clone()),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lists the input products, as indexed (identity, type, files, time frame).
    /// Nothing is processed.
    Index(IndexArgs),

    /// Quality control of the signal sources: observations, sampling, gaps,
    /// cycle slips, signal strength, ephemeris coverage.
    Qc(QcArgs),

    /// P.V.T solutions (SPP, CPP, PPP, RTK). Requires the navigation feature.
    Nav(NavArgs),

    /// CGGTTS solutions (common view time transfer). Requires the cggtts feature.
    Cggtts(CggttsArgs),

    /// Residual analysis: A − B between two receivers or two orbital products.
    Residuals(ResidualsArgs),

    /// Ionospheric Pierce Points of every line of sight. Requires the ionex feature.
    Ipp(IppArgs),

    /// File operation: writes the (preprocessed) input products as RINEX / SP3
    Filegen(FilegenArgs),

    /// File operation: merges a file into the input product of the same kind
    Merge(MergeArgs),

    /// File operation: splits the input products at this epoch
    Split(SplitArgs),

    /// File operation: time binning, a batch of files of this duration
    Tbin(TbinArgs),

    /// File operation: constellation binning, one file per constellation
    Cbin(CbinArgs),

    /// File operation: observation RINEX (A) − FILE (B)
    Diff(DiffArgs),
}

impl Command {
    /// True for the file operations, which do not deploy the engine
    pub fn is_file_operation(&self) -> bool {
        matches!(
            self,
            Self::Filegen(_)
                | Self::Merge(_)
                | Self::Split(_)
                | Self::Tbin(_)
                | Self::Cbin(_)
                | Self::Diff(_)
        )
    }
}

/// File production options
#[derive(Debug, Default, Args)]
#[command(next_help_heading = "File production")]
pub struct ProductionArgs {
    /// Gzip compressed output
    #[arg(long)]
    pub gzip: bool,

    /// Prefer (deprecated) V2 short file names
    #[arg(short, long)]
    pub short: bool,

    /// Agency (file name), overriding the input
    #[arg(short, long)]
    pub agency: Option<String>,

    /// Country code (3 letters, V3 file name)
    #[arg(long)]
    pub country: Option<String>,

    /// Batch number (V3 file name)
    #[arg(short, long)]
    pub batch: Option<u8>,

    /// Data source (V3 file name): RCVR, STREAM
    #[arg(long, value_name = "RCVR|STREAM")]
    pub src: Option<String>,

    /// Production periodicity (V3 file name): 15M, 01H, 01D, 01Y
    #[arg(long)]
    pub ppu: Option<String>,

    /// Sampling interval (V3 file name), for example 30S
    #[arg(long)]
    pub ffu: Option<String>,

    /// Compress observations to CRINEX
    #[arg(long)]
    pub rnx2crx: bool,

    /// Decompress CRINEX observations to readable RINEX
    #[arg(long)]
    pub crx2rnx: bool,
}

#[derive(Debug, Args)]
pub struct FilegenArgs {
    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// File to merge
    pub file: PathBuf,

    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct SplitArgs {
    /// Split epoch, for example "2020-06-25T12:00:00 GPST"
    pub epoch: Epoch,

    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct TbinArgs {
    /// Batch duration, for example "1 hour" or "15 min"
    pub duration: gnss_qc::prelude::Duration,

    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct CbinArgs {
    /// Express each file in the timescale of its constellation
    #[arg(long)]
    pub timescale: bool,

    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Observation RINEX (B)
    pub file: PathBuf,

    #[command(flatten)]
    pub production: ProductionArgs,
}

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Prints the index as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct QcArgs {
    /// Do not analyze the ephemeris coverage of the signal sources
    #[arg(long)]
    pub no_coverage: bool,
}

/// Navigation method
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MethodArg {
    /// Single frequency code
    Spp,
    /// Dual frequency code (iono free)
    Cpp,
    /// Dual frequency code and phase (iono free)
    Ppp,
}

/// Orbital products preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrbitsArg {
    /// Precise products (SP3) whenever they cover the satellite, broadcast otherwise
    Auto,
    /// Precise products only
    Precise,
    /// Broadcast ephemerides only
    Broadcast,
}

impl OrbitsArg {
    /// GNSS-Qc preference
    #[cfg_attr(not(any(feature = "navigation", feature = "ionex")), allow(dead_code))]
    pub fn to_preference(self) -> gnss_qc::prelude::QcOrbitPreference {
        use gnss_qc::prelude::QcOrbitPreference;
        match self {
            Self::Auto => QcOrbitPreference::Auto,
            Self::Precise => QcOrbitPreference::PreciseOnly,
            Self::Broadcast => QcOrbitPreference::BroadcastOnly,
        }
    }
}

/// User (rover) profile
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProfileArg {
    Static,
    Pedestrian,
    Car,
    Airplane,
    Rocket,
}

/// Receiver clock profile
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClockArg {
    Quartz,
    Oscillator,
    Atomic,
    HydrogenMaser,
}

/// Options shared by the navigation commands
#[derive(Debug, Args)]
#[command(next_help_heading = "Navigation")]
pub struct NavigationArgs {
    /// Navigation method. Default: cpp (or the configuration file).
    #[arg(short, long, value_enum)]
    pub method: Option<MethodArg>,

    /// Signal source to navigate (identity, as listed by the index command).
    /// Default: the first signal source.
    #[arg(long, value_name = "PRODUCT")]
    pub rover: Option<String>,

    /// Constellations, comma separated (gps,gal,bds,qzss). Default: GPS, Galileo.
    #[arg(short, long, value_delimiter = ',')]
    pub constellations: Vec<String>,

    /// Orbital products preference
    #[arg(long, value_enum)]
    pub orbits: Option<OrbitsArg>,

    /// User profile. Default: pedestrian (or the configuration file).
    #[arg(long, value_enum)]
    pub profile: Option<ProfileArg>,

    /// Static receiver (shortcut for --profile static)
    #[arg(long = "static")]
    pub is_static: bool,

    /// Receiver clock profile. Default: quartz (or the configuration file).
    #[arg(long, value_enum)]
    pub clock: Option<ClockArg>,

    /// A priori position: "x,y,z" ECEF (m), or "header" (RINEX header of the rover).
    /// Default: none, the solver surveys from scratch.
    #[arg(long, value_name = "x,y,z|header")]
    pub apriori: Option<String>,

    /// Reference position to express the errors: "x,y,z" ECEF (m).
    /// Default: the RINEX header of the rover.
    #[arg(long, value_name = "x,y,z")]
    pub reference: Option<String>,

    /// Navigation configuration (QcNavigationConfig) as JSON file.
    /// Command line options take precedence.
    #[arg(long = "cfg", value_name = "JSON")]
    pub cfg: Option<PathBuf>,

    /// Folder of the ANISE dataset (de440s.bsp and pck11.pca).
    /// Default: embedded dataset (embed_ephem feature), downloaded otherwise.
    #[arg(long, value_name = "DIR", env = "GNSS_QC_ANISE_DIR")]
    pub anise_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct NavArgs {
    #[command(flatten)]
    pub navigation: NavigationArgs,

    /// Differential navigation (RTK) relative to this signal source
    #[arg(long, value_name = "PRODUCT")]
    pub base: Option<String>,

    /// Position of the RTK base, "x,y,z" ECEF (m). Default: its RINEX header.
    #[arg(long, value_name = "x,y,z")]
    pub base_position: Option<String>,

    /// Also format the solutions as a GPX track
    #[arg(long)]
    pub gpx: bool,

    /// Also format the solutions as a KML track
    #[arg(long)]
    pub kml: bool,
}

#[derive(Debug, Args)]
pub struct CggttsArgs {
    #[command(flatten)]
    pub navigation: NavigationArgs,

    /// Station name, written in the CGGTTS header. Default: marker name.
    #[arg(long)]
    pub station: Option<String>,

    /// Reference frame of the antenna coordinates, written in the CGGTTS header
    #[arg(long, default_value = "ITRF")]
    pub frame: String,

    /// Minimal track duration (s): shorter tracks are discarded
    #[arg(long, default_value_t = 720.0)]
    pub min_track: f64,

    /// Also stream (and report) the P.V.T solutions
    #[arg(long)]
    pub solutions: bool,
}

#[derive(Debug, Args)]
pub struct ResidualsArgs {
    #[command(subcommand)]
    pub kind: ResidualsKind,
}

#[derive(Debug, Subcommand)]
pub enum ResidualsKind {
    /// Observation residuals: receiver A − receiver B (same satellite, same observable)
    Obs(ObsResidualsArgs),

    /// Orbit residuals: product A − product B (radial, along track, cross track, clock)
    Orbit(OrbitResidualsArgs),
}

#[derive(Debug, Args)]
pub struct ObsResidualsArgs {
    /// Receiver A (minuend). Default: the first signal source.
    #[arg(long, value_name = "PRODUCT")]
    pub lhs: Option<String>,

    /// Receiver B (subtrahend). Default: the other signal source.
    #[arg(long, value_name = "PRODUCT")]
    pub rhs: Option<String>,

    /// Differentiate Doppler shifts and signal strengths too
    #[arg(long)]
    pub all: bool,

    /// Maximal time difference between the epochs of both receivers (s)
    #[arg(long)]
    pub tolerance: Option<f64>,
}

#[derive(Debug, Args)]
pub struct OrbitResidualsArgs {
    /// Product A: sp3, sp3:PRODUCT, nav, nav:PRODUCT
    #[arg(long, default_value = "nav")]
    pub lhs: String,

    /// Product B: sp3, sp3:PRODUCT, nav, nav:PRODUCT
    #[arg(long, default_value = "sp3")]
    pub rhs: String,

    /// Minimal interval between two evaluations (s)
    #[arg(long)]
    pub sampling: Option<f64>,

    /// Broadcast ephemerides: maximal distance to their time of clock (s)
    #[arg(long)]
    pub max_age: Option<f64>,

    /// Constellations, comma separated. Default: all.
    #[arg(short, long, value_delimiter = ',')]
    pub constellations: Vec<String>,
}

#[derive(Debug, Args)]
pub struct IppArgs {
    /// Elevation mask (degrees)
    #[arg(long, default_value_t = 10.0)]
    pub elevation_mask: f64,

    /// Altitude of the ionospheric shell (km)
    #[arg(long, default_value_t = 450.0)]
    pub shell_altitude: f64,

    /// Orbital products preference
    #[arg(long, value_enum, default_value = "auto")]
    pub orbits: OrbitsArg,

    /// Constellations, comma separated. Default: all.
    #[arg(short, long, value_delimiter = ',')]
    pub constellations: Vec<String>,
}

/// Parses "x,y,z" coordinates
#[cfg_attr(not(feature = "navigation"), allow(dead_code))]
pub fn parse_coordinates(text: &str) -> Result<(f64, f64, f64), Error> {
    let values = text
        .split(',')
        .map(|v| f64::from_str(v.trim()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::InvalidCoordinates(text.to_string()))?;

    match values.as_slice() {
        [x, y, z] => Ok((*x, *y, *z)),
        _ => Err(Error::InvalidCoordinates(text.to_string())),
    }
}

/// Parses a list of constellations
pub fn parse_constellations(
    list: &[String],
) -> Result<Vec<gnss_qc::prelude::Constellation>, Error> {
    list.iter()
        .map(|c| {
            gnss_qc::prelude::Constellation::from_str(c.trim())
                .map_err(|_| Error::Config(format!("unknown constellation \"{}\"", c)))
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn indexing_methods() {
        for (text, expected) in [
            ("auto", IndexingMethod::Auto),
            (
                "receiver",
                IndexingMethod::Preset(QcIndexingConfig::Receiver),
            ),
            ("Agency", IndexingMethod::Preset(QcIndexingConfig::Agency)),
            (
                "filename",
                IndexingMethod::Preset(QcIndexingConfig::FileName),
            ),
            (
                "alias:BRDC",
                IndexingMethod::Preset(QcIndexingConfig::Custom("BRDC".to_string())),
            ),
        ] {
            assert_eq!(IndexingMethod::from_str(text).unwrap(), expected);
        }

        assert!(IndexingMethod::from_str("alias:").is_err());
        assert_eq!(parse_coordinates("1.0, -2.5,3").unwrap(), (1.0, -2.5, 3.0));
        assert!(parse_coordinates("1,2").is_err());
        assert_eq!(
            parse_constellations(&["gps".to_string(), "Gal".to_string()])
                .unwrap()
                .len(),
            2
        );
        assert!(IndexingMethod::from_str("unknown").is_err());
    }

    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
