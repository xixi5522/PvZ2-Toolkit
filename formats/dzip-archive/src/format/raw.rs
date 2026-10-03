//! Lossless archive metadata.
//!
//! Dzip stores names as nul-terminated byte strings.  Keeping those bytes
//! separate from their host-filesystem representation is important for format
//! inspection and for faithfully rewriting metadata produced on another
//! platform.

use super::{ArchiveSettings, Chunk, ChunkSettings, RangeSettings};
use crate::{DzipError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ArchiveString(Vec<u8>);

impl ArchiveString {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.contains(&0) {
            return Err(DzipError::InvalidArchive(
                "archive strings cannot contain embedded nul bytes".to_string(),
            ));
        }
        Ok(Self(bytes))
    }

    pub(crate) fn from_terminated_field(bytes: Vec<u8>) -> Self {
        debug_assert!(!bytes.contains(&0));
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn as_str(&self) -> Result<&str> {
        std::str::from_utf8(&self.0).map_err(|error| {
            DzipError::InvalidArchive(format!("archive string is not UTF-8: {error}"))
        })
    }

    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    pub(crate) fn encoded_len(&self) -> usize {
        self.0.len() + 1
    }
}

impl AsRef<[u8]> for ArchiveString {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<String> for ArchiveString {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

impl From<&str> for ArchiveString {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ArchivePath(ArchiveString);

impl ArchivePath {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self> {
        ArchiveString::new(bytes).map(Self)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_str(&self) -> Result<&str> {
        self.0.as_str()
    }

    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        self.0.to_string_lossy()
    }
}

impl AsRef<[u8]> for ArchivePath {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFileRecord {
    pub directory_id: u16,
    pub chunk_ids: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawArchive {
    pub settings: ArchiveSettings,
    /// User-file names followed by the stored directory table (the implicit
    /// root directory is not present in this list).
    pub strings: Vec<ArchiveString>,
    pub files: Vec<RawFileRecord>,
    pub chunk_settings: ChunkSettings,
    /// Exact chunk records as stored in the archive header.
    pub chunks: Vec<Chunk>,
    /// Names of auxiliary volumes. Volume zero is the main archive and is not
    /// stored in this list.
    pub volume_files: Vec<ArchiveString>,
    pub range_settings: Option<RangeSettings>,
}
