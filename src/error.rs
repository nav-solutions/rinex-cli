//! Application errors
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no input product: use -f or -d")]
    NoInput,

    #[error("{0}: file not found")]
    FileNotFound(String),

    #[error("{0}: format not supported (RINEX, CRINEX, SP3, gzip compressed or not)")]
    FormatNotSupported(String),

    #[error("invalid indexing method \"{0}\": receiver, antenna, operator, agency, filename, auto or alias:NAME")]
    InvalidIndexingMethod(String),

    #[error("invalid preprocessing filter \"{0}\": {1}")]
    InvalidFilter(String, String),

    #[error("gnss-qc input: {0}")]
    QcInput(String),

    #[error("gnss-qc engine: {0}")]
    QcEngine(String),

    #[error("unknown product \"{0}\": run the index command to list the products")]
    UnknownProduct(String),

    #[cfg_attr(not(feature = "navigation"), allow(dead_code))]
    #[error("invalid coordinates \"{0}\": expecting \"x,y,z\"")]
    InvalidCoordinates(String),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Config(String),

    #[cfg(feature = "navigation")]
    #[error("navigation: {0}")]
    Navigation(String),

    #[cfg(feature = "pdf")]
    #[error("pdf report: {0}")]
    Pdf(String),

    #[error("file operation: {0}")]
    FileOperation(String),
}
