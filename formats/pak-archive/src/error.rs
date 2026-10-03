//! PAK decoding and encoding failures.

use std::io;
use thiserror::Error;

use crate::options::PakSourceInfo;

pub type Result<T> = std::result::Result<T, PakError>;

#[derive(Debug, Error)]
pub enum PakError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[cfg(feature = "tv")]
    #[error("TV/ZIP container error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("invalid PAK magic {found:02X?}")]
    InvalidMagic { found: [u8; 4] },

    #[error("unsupported PAK version {found}; expected 0")]
    InvalidVersion { found: u32 },

    #[error("invalid directory marker 0x{marker:02X} at byte {offset}")]
    InvalidMarker { offset: u64, marker: u8 },

    #[error("{resource} limit exceeded: requested {requested}, limit {limit}")]
    LimitExceeded {
        resource: &'static str,
        requested: u64,
        limit: u64,
    },

    #[error("integer overflow while calculating {context}")]
    IntegerOverflow { context: &'static str },

    #[error("invalid {context} at byte {offset}: {message}")]
    InvalidData {
        context: &'static str,
        offset: u64,
        message: String,
    },

    #[error("no supported PAK layout matched ({attempts_count} attempts)", attempts_count = .attempts.len())]
    NoMatchingFormat { attempts: Vec<FormatAttempt> },

    #[error("PAK layout is ambiguous: {candidates:?}")]
    AmbiguousFormat { candidates: Vec<PakSourceInfo> },

    #[error("entry index {index} is out of bounds for {entries} entries")]
    InvalidEntryIndex { index: usize, entries: usize },

    #[error("failed to allocate {requested} bytes for {context}")]
    AllocationFailed {
        context: &'static str,
        requested: u64,
    },

    #[error("entry {index} could not be read: {source}")]
    EntryIo {
        index: usize,
        #[source]
        source: io::Error,
    },

    #[error("incompatible archive metadata{entry_suffix}: {message}", entry_suffix = entry.map(|value| format!(" for entry {value}")).unwrap_or_default())]
    IncompatibleMetadata {
        entry: Option<usize>,
        message: &'static str,
    },

    #[error("entry {entry} source length mismatch: declared {declared}, consumed {consumed}")]
    SourceLengthMismatch {
        entry: usize,
        declared: u64,
        consumed: u64,
    },

    #[error("PAK writing was cancelled")]
    Cancelled,

    #[error("ZIP entry {entry} is encrypted and is not supported")]
    EncryptedZipEntry { entry: usize },

    #[error("TV/ZIP support is disabled; enable the `tv` feature")]
    TvFeatureDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatAttempt {
    pub candidate: PakSourceInfo,
    pub reason: String,
}

impl PakError {
    pub(crate) fn invalid(context: &'static str, offset: u64, message: impl Into<String>) -> Self {
        Self::InvalidData {
            context,
            offset,
            message: message.into(),
        }
    }
}
