//! Input products: file discovery, preprocessing, indexing.
//!
//! Every file is parsed, preprocessed, then loaded into the GNSS-Qc products table
//! ([QcInputProducts]) which identifies ([QcIndexing]) and merges them.
//! We keep a manifest of what was loaded, to present the indexing to the user
//! (index command, reports).
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
};

use log::{debug, info, warn};
use serde::Serialize;
use walkdir::WalkDir;

use gnss_qc::prelude::{
    Constellation, Duration, Epoch, QcIndexing, QcIndexingConfig, QcInputConfig, QcInputProducts,
    SV,
};

use gnss_qc_traits::{Filter, Merge, Preprocessing, Repair, RepairTrait};

use rinex::prelude::{Rinex, RinexType};
use sp3::prelude::SP3;

use crate::{
    cli::{InputArgs, PreprocessingArgs},
    error::Error,
};

/// Kind of input product
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ProductKind {
    Observation,
    Navigation,
    Meteo,
    Clock,
    Antex,
    SP3,
    #[cfg_attr(not(feature = "ionex"), allow(dead_code))]
    Ionex,
}

impl std::fmt::Display for ProductKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Observation => write!(f, "OBS"),
            Self::Navigation => write!(f, "NAV"),
            Self::Meteo => write!(f, "METEO"),
            Self::Clock => write!(f, "CLK"),
            Self::Antex => write!(f, "ANTEX"),
            Self::SP3 => write!(f, "SP3"),
            Self::Ionex => write!(f, "IONEX"),
        }
    }
}

impl ProductKind {
    fn from_rinex_type(rtype: RinexType) -> Self {
        match rtype {
            RinexType::ObservationData => Self::Observation,
            RinexType::NavigationData => Self::Navigation,
            RinexType::MeteoData => Self::Meteo,
            RinexType::ClockData => Self::Clock,
            RinexType::AntennaData => Self::Antex,
        }
    }
}

/// One indexed product (possibly several files merged together)
#[derive(Debug, Clone, Serialize)]
pub struct ProductEntry {
    /// Identity, as resolved by GNSS-Qc
    #[serde(serialize_with = "serialize_display")]
    pub identity: QcIndexing,

    /// Kind of product
    pub kind: ProductKind,

    /// Files merged into this product
    pub files: Vec<PathBuf>,

    /// First epoch
    pub first: Option<Epoch>,

    /// Last epoch
    pub last: Option<Epoch>,

    /// Dominant sampling interval
    pub sampling: Option<Duration>,

    /// Satellites
    #[serde(serialize_with = "serialize_satellites")]
    pub satellites: BTreeSet<SV>,

    /// Reference position (ECEF m), observations only
    pub position_ecef_m: Option<(f64, f64, f64)>,

    /// Receiver, antenna, agency.. descriptions found in the header
    pub details: Vec<(String, String)>,
}

fn serialize_display<T: std::fmt::Display, S: serde::Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

fn serialize_satellites<S: serde::Serializer>(
    satellites: &BTreeSet<SV>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(satellites.iter().map(|sv| sv.to_string()))
}

impl ProductEntry {
    /// Constellations of this product
    pub fn constellations(&self) -> BTreeSet<Constellation> {
        self.satellites.iter().map(|sv| sv.constellation).collect()
    }

    /// Duration covered by this product
    pub fn duration(&self) -> Option<Duration> {
        Some(self.last? - self.first?)
    }

    fn absorb(&mut self, rhs: ProductEntry) {
        self.files.extend(rhs.files);
        self.first = min_opt(self.first, rhs.first);
        self.last = max_opt(self.last, rhs.last);
        self.sampling = min_opt(self.sampling, rhs.sampling);
        self.satellites.extend(rhs.satellites);
        if self.position_ecef_m.is_none() {
            self.position_ecef_m = rhs.position_ecef_m;
        }
        for (key, value) in rhs.details {
            if !self.details.iter().any(|(k, v)| *k == key && *v == value) {
                self.details.push((key, value));
            }
        }
    }
}

fn min_opt<T: PartialOrd>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b < a { b } else { a }),
        (a, None) => a,
        (None, b) => b,
    }
}

fn max_opt<T: PartialOrd>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b > a { b } else { a }),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Loaded inputs: the GNSS-Qc products table and its manifest.
pub struct Inputs {
    /// GNSS-Qc products table
    pub products: QcInputProducts,

    /// Manifest, sorted by kind then identity
    pub manifest: Vec<ProductEntry>,

    /// Session name, derived from the primary file
    pub session: String,

    /// Meteo and clock RINEX, merged per identity: available to the file operations
    pub others: Vec<(ProductKind, QcIndexing, Rinex)>,

    /// IONEX products, merged per identity: header of the synthesized IONEX files
    #[cfg(feature = "ionex")]
    pub ionex: Vec<(QcIndexing, ionex::prelude::IONEX)>,
}

/// One input file, possibly aliased
#[derive(Debug, Clone, PartialEq)]
struct InputFile {
    alias: Option<String>,
    path: PathBuf,
    /// Explicitly designated (-f): errors are fatal
    explicit: bool,
}

/// Splits `[ALIAS=]PATH`. The full text is preferred when it designates
/// an existing path (paths may contain '=').
fn split_alias(arg: &str) -> Result<(Option<String>, PathBuf), Error> {
    if Path::new(arg).exists() {
        return Ok((None, PathBuf::from(arg)));
    }

    if let Some((alias, path)) = arg.split_once('=') {
        let (alias, path) = (alias.trim(), Path::new(path));
        if !alias.is_empty() && path.exists() {
            return Ok((Some(alias.to_string()), path.to_path_buf()));
        }
    }

    Err(Error::FileNotFound(arg.to_string()))
}

/// Lists the files designated by -f and -d
fn discover(args: &InputArgs) -> Result<Vec<InputFile>, Error> {
    let mut files = Vec::new();

    for arg in args.files.iter() {
        let (alias, path) = split_alias(arg)?;
        files.push(InputFile {
            alias,
            path,
            explicit: true,
        });
    }

    for arg in args.directories.iter() {
        let (alias, dir) = split_alias(arg)?;

        let mut found = WalkDir::new(&dir)
            .max_depth(args.depth)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.path().to_path_buf())
            .filter(|path| {
                // hidden files (.gitignore, .DS_Store..)
                !path
                    .file_name()
                    .map(|name| name.to_string_lossy().starts_with('.'))
                    .unwrap_or(true)
            })
            .map(|path| InputFile {
                alias: alias.clone(),
                path,
                explicit: false,
            })
            .collect::<Vec<_>>();

        files.append(&mut found);
    }

    if files.is_empty() {
        return Err(Error::NoInput);
    }

    Ok(files)
}

/// Gzip compression is determined from the content (magic bytes), not the extension
fn is_gzip(path: &Path) -> bool {
    let mut magic = [0u8; 2];
    File::open(path)
        .and_then(|mut fd| fd.read_exact(&mut magic))
        .map(|_| magic == [0x1f, 0x8b])
        .unwrap_or(false)
}

/// Parsed file
pub enum Parsed {
    Rinex(Box<Rinex>),
    SP3(Box<SP3>),
    #[cfg(feature = "ionex")]
    Ionex(Box<ionex::prelude::IONEX>),
}

pub fn parse(path: &Path) -> Option<Parsed> {
    let gzip = is_gzip(path);

    let rinex = if gzip {
        Rinex::from_gzip_file(path)
    } else {
        Rinex::from_file(path)
    };

    match rinex {
        Ok(rinex) => return Some(Parsed::Rinex(Box::new(rinex))),
        Err(e) => debug!("{}: not a RINEX ({})", path.display(), e),
    }

    #[cfg(feature = "ionex")]
    {
        let ionex = if gzip {
            ionex::prelude::IONEX::from_gzip_file(path)
        } else {
            ionex::prelude::IONEX::from_file(path)
        };

        match ionex {
            Ok(ionex) if ionex.record.iter().next().is_some() => {
                return Some(Parsed::Ionex(Box::new(ionex)))
            },
            Ok(_) => debug!("{}: not a IONEX (no map)", path.display()),
            Err(e) => debug!("{}: not a IONEX ({:?})", path.display(), e),
        }
    }

    let sp3 = if gzip {
        SP3::from_gzip_file(path)
    } else {
        SP3::from_file(path)
    };

    // the SP3 and IONEX parsers are lenient: the content must be described
    match sp3 {
        Ok(sp3) if !sp3.data.is_empty() => return Some(Parsed::SP3(Box::new(sp3))),
        Ok(_) => debug!("{}: not a SP3 (no content)", path.display()),
        Err(e) => debug!("{}: not a SP3 ({:?})", path.display(), e),
    }

    None
}

/// Preprocessing operations, designed once
pub struct Preprocessor {
    filters: Vec<Filter>,
    zero_repair: bool,
}

impl Preprocessor {
    pub fn new(args: &PreprocessingArgs) -> Result<Self, Error> {
        let mut specs = Vec::<String>::new();

        for (excluded, spec) in [
            (args.no_gps, "!=GPS"),
            (args.no_glonass, "!=GLO"),
            (args.no_galileo, "!=Gal"),
            (args.no_beidou, "!=BDS"),
            (args.no_qzss, "!=QZSS"),
            (args.no_irnss, "!=IRNSS"),
            (args.no_sbas, "!=geo"),
        ] {
            if excluded {
                specs.push(spec.to_string());
            }
        }

        if args.no_beidou_geo {
            specs.push(">C05".to_string());
            specs.push("<C55".to_string());
        }

        specs.extend(args.filters.iter().cloned());

        let filters = specs
            .iter()
            .map(|spec| {
                Filter::from_str(spec.trim())
                    .map_err(|e| Error::InvalidFilter(spec.clone(), format!("{:?}", e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            filters,
            zero_repair: args.zero_repair,
        })
    }

    pub fn rinex(&self, rinex: &mut Rinex) {
        for filter in self.filters.iter() {
            rinex.filter_mut(filter);
        }
        if self.zero_repair {
            rinex.repair_mut(Repair::Zero);
        }
    }

    pub fn sp3(&self, sp3: &mut SP3) {
        for filter in self.filters.iter() {
            sp3.filter_mut(filter);
        }
    }
}

/// Builds the input preferences: JSON file, then command line options.
fn input_config(args: &InputArgs) -> Result<QcInputConfig, Error> {
    let mut cfg = match &args.input_cfg {
        Some(path) => {
            let content = std::fs::read_to_string(path)?;
            serde_json::from_str::<QcInputConfig>(&content)?
        },
        None => QcInputConfig::default(),
    };

    if let Some(method) = &args.obs_index {
        cfg.obs_rinex_indexing = method.to_config();
    }
    if let Some(method) = &args.nav_index {
        cfg.nav_rinex_indexing = method.to_config();
    }
    if let Some(method) = &args.sp3_index {
        cfg.sp3_indexing = method.to_config();
    }
    if let Some(method) = &args.meteo_index {
        cfg.meteo_rinex_indexing = method.to_config();
    }
    if let Some(method) = &args.clk_index {
        cfg.clk_rinex_indexing = method.to_config();
    }
    if let Some(method) = &args.ionex_index {
        cfg.ionex_indexing = method.to_config();
    }

    Ok(cfg)
}

/// Preferences where every product is indexed by this alias
fn aliased_config(alias: &str) -> QcInputConfig {
    let custom = Some(QcIndexingConfig::Custom(alias.to_string()));
    QcInputConfig {
        obs_rinex_indexing: custom.clone(),
        nav_rinex_indexing: custom.clone(),
        meteo_rinex_indexing: custom.clone(),
        clk_rinex_indexing: custom.clone(),
        sp3_indexing: custom.clone(),
        ionex_indexing: custom,
    }
}

/// File name without (any) extension
pub fn file_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    name.split('.').next().unwrap_or(&name).to_string()
}

/// Describes a RINEX file
fn rinex_entry(identity: QcIndexing, path: &Path, rinex: &Rinex) -> ProductEntry {
    let kind = ProductKind::from_rinex_type(rinex.header.rinex_type);
    let mut details = Vec::new();

    if let Some(marker) = &rinex.header.geodetic_marker {
        details.push(("marker".to_string(), marker.name.clone()));
    }
    if let Some(rcvr) = &rinex.header.rcvr {
        details.push((
            "receiver".to_string(),
            format!("{} #{}", rcvr.model.trim(), rcvr.sn.trim()),
        ));
    }
    if let Some(antenna) = &rinex.header.rcvr_antenna {
        details.push(("antenna".to_string(), antenna.model.trim().to_string()));
    }
    if let Some(agency) = &rinex.header.agency {
        details.push(("agency".to_string(), agency.trim().to_string()));
    }
    if let Some(observer) = &rinex.header.observer {
        details.push(("operator".to_string(), observer.trim().to_string()));
    }

    let satellites = if kind == ProductKind::Antex {
        BTreeSet::new()
    } else {
        rinex.sv_iter().collect()
    };

    let position_ecef_m = if kind == ProductKind::Observation {
        gnss_qc::io::rinex::QcRinexObsSource::antenna_reference_point(rinex)
    } else {
        None
    };

    ProductEntry {
        identity,
        kind,
        files: vec![path.to_path_buf()],
        first: rinex.first_epoch(),
        last: rinex.last_epoch(),
        sampling: rinex
            .dominant_sampling_interval()
            .filter(|dt| *dt > Duration::ZERO && kind != ProductKind::Navigation),
        satellites,
        position_ecef_m,
        details,
    }
}

/// Describes a SP3 file
fn sp3_entry(identity: QcIndexing, path: &Path, sp3: &SP3) -> ProductEntry {
    ProductEntry {
        identity,
        kind: ProductKind::SP3,
        files: vec![path.to_path_buf()],
        first: sp3.first_epoch(),
        last: sp3.last_epoch(),
        sampling: Some(sp3.header.sampling_period),
        satellites: sp3
            .satellites_iter()
            .chain(sp3.satellites_position_km_iter().map(|item| item.1))
            .collect(),
        position_ecef_m: None,
        details: vec![
            ("agency".to_string(), sp3.header.agency.trim().to_string()),
            (
                "coordinates".to_string(),
                sp3.header.coord_system.trim().to_string(),
            ),
            ("timescale".to_string(), sp3.header.timescale.to_string()),
        ],
    }
}

/// Describes a IONEX file
#[cfg(feature = "ionex")]
fn ionex_entry(identity: QcIndexing, path: &Path, ionex: &ionex::prelude::IONEX) -> ProductEntry {
    let header = &ionex.header;
    let grid = &header.grid;

    let mut details = Vec::new();

    if let Some(agency) = &header.run_by {
        details.push(("agency".to_string(), agency.trim().to_string()));
    }
    if let Some(program) = &header.program {
        details.push(("program".to_string(), program.trim().to_string()));
    }
    details.push((
        "grid".to_string(),
        format!(
            "latitude {}° → {}° ({}°), longitude {}° → {}° ({}°), altitude {} km",
            grid.latitude.start,
            grid.latitude.end,
            grid.latitude.spacing,
            grid.longitude.start,
            grid.longitude.end,
            grid.longitude.spacing,
            grid.altitude.start,
        ),
    ));
    details.push(("maps".to_string(), ionex.epoch_iter().count().to_string()));

    let epochs = ionex.epoch_iter().collect::<Vec<_>>();

    ProductEntry {
        identity,
        kind: ProductKind::Ionex,
        files: vec![path.to_path_buf()],
        first: epochs.first().copied(),
        last: epochs.last().copied(),
        sampling: epochs.windows(2).map(|w| w[1] - w[0]).min(),
        satellites: BTreeSet::new(),
        position_ecef_m: None,
        details,
    }
}

impl Inputs {
    /// Discovers, parses, preprocesses and indexes the input products
    pub fn load(args: &InputArgs, preprocessing: &PreprocessingArgs) -> Result<Self, Error> {
        let files = discover(args)?;
        let cfg = input_config(args)?;
        let preprocessor = Preprocessor::new(preprocessing)?;

        let mut products = QcInputProducts::default();
        let mut manifest = BTreeMap::<(ProductKind, String), ProductEntry>::new();

        let mut primary: Option<(bool, PathBuf)> = None;
        let mut others = Vec::<(ProductKind, QcIndexing, Rinex)>::new();

        #[cfg(feature = "ionex")]
        let mut ionex_products = Vec::<(QcIndexing, ionex::prelude::IONEX)>::new();

        for file in files.iter() {
            let path = &file.path;

            let parsed = match parse(path) {
                Some(parsed) => parsed,
                None => {
                    if file.explicit {
                        return Err(Error::FormatNotSupported(path.display().to_string()));
                    }
                    warn!("{}: format not supported, skipped", path.display());
                    continue;
                },
            };

            let file_cfg = match &file.alias {
                Some(alias) => aliased_config(alias),
                None => cfg.clone(),
            };

            // The identity is resolved by GNSS-Qc: the file is first loaded on its own
            // (exact identity), then merged into the session table.
            let entry = match parsed {
                Parsed::Rinex(mut rinex) => {
                    preprocessor.rinex(&mut rinex);

                    let mut single = QcInputProducts::default();
                    single
                        .load_rinex(&file_cfg, path, (*rinex).clone())
                        .map_err(|e| Error::QcInput(e.to_string()))?;

                    let identity = single
                        .obs_rinex_iter()
                        .chain(single.nav_rinex_iter())
                        .chain(single.meteo_rinex_iter())
                        .chain(single.clk_rinex_iter())
                        .chain(single.antex_iter())
                        .map(|(identity, _)| identity.clone())
                        .next()
                        .unwrap_or_else(|| QcIndexing::Custom(file_stem(path)));

                    let entry = rinex_entry(identity, path, &rinex);

                    if matches!(entry.kind, ProductKind::Meteo | ProductKind::Clock) {
                        match others
                            .iter_mut()
                            .find(|(kind, id, _)| *kind == entry.kind && *id == entry.identity)
                        {
                            Some((_, _, merged)) => merged.merge_mut(&rinex).map_err(|e| {
                                Error::QcInput(format!("{}: {}", path.display(), e))
                            })?,
                            None => {
                                others.push((entry.kind, entry.identity.clone(), (*rinex).clone()))
                            },
                        }
                    }

                    products
                        .load_rinex(&file_cfg, path, *rinex)
                        .map_err(|e| Error::QcInput(format!("{}: {}", path.display(), e)))?;

                    entry
                },
                Parsed::SP3(mut sp3) => {
                    preprocessor.sp3(&mut sp3);

                    let mut single = QcInputProducts::default();
                    single
                        .load_sp3(&file_cfg, path, (*sp3).clone())
                        .map_err(|e| Error::QcInput(e.to_string()))?;

                    let identity = single
                        .sp3_iter()
                        .map(|(identity, _)| identity.clone())
                        .next()
                        .expect("internal error: sp3 indexing");

                    let entry = sp3_entry(identity, path, &sp3);

                    products
                        .load_sp3(&file_cfg, path, *sp3)
                        .map_err(|e| Error::QcInput(format!("{}: {}", path.display(), e)))?;

                    entry
                },
                #[cfg(feature = "ionex")]
                Parsed::Ionex(ionex) => {
                    let mut single = QcInputProducts::default();
                    single
                        .load_ionex(&file_cfg, path, (*ionex).clone())
                        .map_err(|e| Error::QcInput(e.to_string()))?;

                    let identity = single
                        .ionex_iter()
                        .map(|(identity, _)| identity.clone())
                        .next()
                        .expect("internal error: ionex indexing");

                    let entry = ionex_entry(identity.clone(), path, &ionex);

                    match ionex_products.iter_mut().find(|(id, _)| *id == identity) {
                        Some((_, merged)) => {
                            for (key, tec) in ionex.record.iter() {
                                merged.record.insert(*key, *tec);
                            }
                        },
                        None => ionex_products.push((identity, (*ionex).clone())),
                    }

                    products
                        .load_ionex(&file_cfg, path, *ionex)
                        .map_err(|e| Error::QcInput(format!("{}: {}", path.display(), e)))?;

                    entry
                },
            };

            info!(
                "{} loaded: {} \"{}\"",
                path.display(),
                entry.kind,
                entry.identity
            );

            // primary file: first observation, first file otherwise
            let is_obs = entry.kind == ProductKind::Observation;
            match &primary {
                None => primary = Some((is_obs, path.clone())),
                Some((false, _)) if is_obs => primary = Some((true, path.clone())),
                _ => {},
            }

            let key = (entry.kind, entry.identity.to_string());
            match manifest.get_mut(&key) {
                Some(existing) => existing.absorb(entry),
                None => {
                    manifest.insert(key, entry);
                },
            }
        }

        if manifest.is_empty() {
            return Err(Error::NoInput);
        }

        let session = primary
            .map(|(_, path)| file_stem(&path))
            .unwrap_or_else(|| "session".to_string());

        Ok(Self {
            products,
            manifest: manifest.into_values().collect(),
            session,
            others,
            #[cfg(feature = "ionex")]
            ionex: ionex_products,
        })
    }

    /// Products of this kind
    pub fn entries(&self, kind: ProductKind) -> impl Iterator<Item = &ProductEntry> {
        self.manifest.iter().filter(move |entry| entry.kind == kind)
    }

    /// Resolves a product designated by the user: textual identity
    /// ("receiver:SEPT POLARX5 #3047937", "agency:GRGS", alias), restricted to a kind.
    pub fn resolve(&self, text: &str, kind: ProductKind) -> Result<QcIndexing, Error> {
        let identity =
            QcIndexing::from_str(text).map_err(|_| Error::UnknownProduct(text.to_string()))?;

        if self.entries(kind).any(|entry| entry.identity == identity) {
            return Ok(identity);
        }

        // tolerate a missing prefix: unique match on the name
        let mut candidates = self.entries(kind).filter(|entry| match &entry.identity {
            QcIndexing::Antenna(name)
            | QcIndexing::Receiver(name)
            | QcIndexing::Operator(name)
            | QcIndexing::Agency(name)
            | QcIndexing::Custom(name) => name == text,
        });

        match (candidates.next(), candidates.next()) {
            (Some(entry), None) => Ok(entry.identity.clone()),
            _ => Err(Error::UnknownProduct(text.to_string())),
        }
    }

    /// Reference position of a signal source (ECEF m)
    #[cfg_attr(not(feature = "navigation"), allow(dead_code))]
    pub fn reference_position(&self, identity: &QcIndexing) -> Option<(f64, f64, f64)> {
        self.entries(ProductKind::Observation)
            .find(|entry| entry.identity == *identity)
            .and_then(|entry| entry.position_ecef_m)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn file_stems() {
        assert_eq!(
            file_stem(Path::new("data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz")),
            "ESBC00DNK_R_20201770000_01D_30S_MO"
        );
        assert_eq!(file_stem(Path::new("/tmp/test")), "test");
    }

    #[test]
    fn aliases() {
        assert!(split_alias("base=does/not/exist").is_err());
        let (alias, path) = split_alias("base=Cargo.toml").unwrap();
        assert_eq!(alias, Some("base".to_string()));
        assert_eq!(path, PathBuf::from("Cargo.toml"));
        let (alias, _) = split_alias("Cargo.toml").unwrap();
        assert!(alias.is_none());
    }
}
