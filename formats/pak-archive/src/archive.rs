//! Owned archive, entry metadata, and validated editing operations.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::OnceLock;

use crate::error::{PakError, Result};
use crate::options::{EncodeOptions, PakFormat, PakSourceInfo, ZipCompression};
use crate::path::PakPath;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Timestamp representation associated with an entry container.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
pub enum PakTimestamp {
    #[default]
    None,
    PopCap(u64),
    ZipMsDos {
        date: u16,
        time: u16,
    },
}

/// Logical kind of an archive entry.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PakEntryKind {
    #[default]
    File,
    /// Explicit directory entry used by TV/ZIP containers.
    Directory,
    /// ZIP symbolic link whose payload stores the link target.
    Symlink,
}

/// One preserved ZIP extra-data field.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipExtraField {
    header_id: u16,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    data: Vec<u8>,
    central_only: bool,
}

impl ZipExtraField {
    pub fn new(header_id: u16, data: impl Into<Vec<u8>>, central_only: bool) -> Self {
        Self {
            header_id,
            data: data.into(),
            central_only,
        }
    }

    pub const fn header_id(&self) -> u16 {
        self.header_id
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub const fn central_only(&self) -> bool {
        self.central_only
    }
}

/// ZIP-only metadata retained for editable TV archives.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntryMetadata {
    compression: ZipCompression,
    unix_mode: Option<u32>,
    comment: String,
    extra_fields: Vec<ZipExtraField>,
}

impl Default for ZipEntryMetadata {
    fn default() -> Self {
        Self::new(ZipCompression::Deflated)
    }
}

impl ZipEntryMetadata {
    pub const fn new(compression: ZipCompression) -> Self {
        Self {
            compression,
            unix_mode: None,
            comment: String::new(),
            extra_fields: Vec::new(),
        }
    }

    pub const fn compression(&self) -> ZipCompression {
        self.compression
    }

    pub fn set_compression(&mut self, compression: ZipCompression) {
        self.compression = compression;
    }

    pub const fn unix_mode(&self) -> Option<u32> {
        self.unix_mode
    }

    pub fn set_unix_mode(&mut self, unix_mode: Option<u32>) {
        self.unix_mode = unix_mode;
    }

    pub fn comment(&self) -> &str {
        &self.comment
    }

    pub fn set_comment(&mut self, comment: impl Into<String>) {
        self.comment = comment.into();
    }

    pub fn extra_fields(&self) -> &[ZipExtraField] {
        &self.extra_fields
    }

    pub fn add_extra_field(&mut self, field: ZipExtraField) {
        self.extra_fields.push(field);
    }

    pub fn clear_extra_fields(&mut self) {
        self.extra_fields.clear();
    }
}

/// Metadata known without materializing an entry payload.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PakEntryInfo {
    pub(crate) path: PakPath,
    pub(crate) kind: PakEntryKind,
    pub(crate) timestamp: PakTimestamp,
    pub(crate) stored_size: u64,
    pub(crate) original_size: u64,
    pub(crate) zip: Option<ZipEntryMetadata>,
}

impl PakEntryInfo {
    pub fn path(&self) -> &PakPath {
        &self.path
    }

    pub const fn kind(&self) -> PakEntryKind {
        self.kind
    }

    pub const fn timestamp(&self) -> PakTimestamp {
        self.timestamp
    }

    pub const fn stored_size(&self) -> u64 {
        self.stored_size
    }

    pub const fn original_size(&self) -> u64 {
        self.original_size
    }

    pub fn zip_metadata(&self) -> Option<&ZipEntryMetadata> {
        self.zip.as_ref()
    }
}

/// One editable file stored in a PAK.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PakEntry {
    path: PakPath,
    kind: PakEntryKind,
    timestamp: PakTimestamp,
    zip: Option<ZipEntryMetadata>,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    data: Vec<u8>,
}

impl PakEntry {
    pub fn new(path: impl Into<PakPath>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            kind: PakEntryKind::File,
            timestamp: PakTimestamp::None,
            zip: None,
            data: data.into(),
        }
    }

    pub fn directory(path: impl Into<PakPath>) -> Self {
        let mut path = path.into();
        if !path.as_bytes().ends_with(b"/") {
            let mut bytes = path.as_bytes().to_vec();
            bytes.push(b'/');
            path = PakPath::from(bytes);
        }
        Self {
            path,
            kind: PakEntryKind::Directory,
            timestamp: PakTimestamp::None,
            zip: Some(ZipEntryMetadata::new(ZipCompression::Stored)),
            data: Vec::new(),
        }
    }

    pub fn symlink(path: impl Into<PakPath>, target: impl Into<Vec<u8>>) -> Self {
        let mut metadata = ZipEntryMetadata::new(ZipCompression::Stored);
        metadata.set_unix_mode(Some(0o777));
        Self {
            path: path.into(),
            kind: PakEntryKind::Symlink,
            timestamp: PakTimestamp::None,
            zip: Some(metadata),
            data: target.into(),
        }
    }

    pub fn with_popcap_time(mut self, time: u64) -> Self {
        self.timestamp = PakTimestamp::PopCap(time);
        self
    }

    pub fn with_zip_metadata(
        mut self,
        timestamp: Option<(u16, u16)>,
        metadata: ZipEntryMetadata,
    ) -> Self {
        self.timestamp = timestamp.map_or(PakTimestamp::None, |(date, time)| {
            PakTimestamp::ZipMsDos { date, time }
        });
        self.zip = Some(metadata);
        self
    }

    pub fn path(&self) -> &PakPath {
        &self.path
    }

    pub const fn kind(&self) -> PakEntryKind {
        self.kind
    }

    pub const fn timestamp(&self) -> PakTimestamp {
        self.timestamp
    }

    pub fn zip_metadata(&self) -> Option<&ZipEntryMetadata> {
        self.zip.as_ref()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub(crate) fn decoded(
        path: PakPath,
        kind: PakEntryKind,
        timestamp: PakTimestamp,
        zip: Option<ZipEntryMetadata>,
        data: Vec<u8>,
    ) -> Self {
        Self {
            path,
            kind,
            timestamp,
            zip,
            data,
        }
    }
}

/// Policy used when changing an archive to a container with different metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MetadataConversion {
    #[default]
    RejectLossy,
    Normalize,
}

/// Explicitly reported metadata changes made by [`PakArchive::set_format`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataChange {
    TimestampDropped { entry: usize },
    ZipMetadataDropped { entry: usize },
    EntryDropped { entry: usize, kind: PakEntryKind },
    ArchiveCommentDropped,
}

type PathIndex = HashMap<u64, Vec<usize>>;

/// Complete, editable PAK archive.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct PakArchive {
    options: EncodeOptions,
    /// Original wire properties. This remains provenance after edits.
    source: Option<PakSourceInfo>,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    zip_comment: Vec<u8>,
    entries: Vec<PakEntry>,
    #[cfg_attr(feature = "serde", serde(skip, default))]
    path_index: OnceLock<PathIndex>,
}

impl Clone for PakArchive {
    fn clone(&self) -> Self {
        Self {
            options: self.options,
            source: self.source,
            zip_comment: self.zip_comment.clone(),
            entries: self.entries.clone(),
            path_index: OnceLock::new(),
        }
    }
}

impl PartialEq for PakArchive {
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options
            && self.source == other.source
            && self.zip_comment == other.zip_comment
            && self.entries == other.entries
    }
}

impl Eq for PakArchive {}

impl Default for PakArchive {
    fn default() -> Self {
        Self::empty(EncodeOptions::default())
    }
}

impl PakArchive {
    pub fn empty(options: EncodeOptions) -> Self {
        Self {
            options,
            source: None,
            zip_comment: Vec::new(),
            entries: Vec::new(),
            path_index: OnceLock::new(),
        }
    }

    pub fn new(options: EncodeOptions, entries: Vec<PakEntry>) -> Result<Self> {
        let archive = Self {
            options,
            source: None,
            zip_comment: Vec::new(),
            entries,
            path_index: OnceLock::new(),
        };
        archive.validate()?;
        Ok(archive)
    }

    pub(crate) fn decoded(
        options: EncodeOptions,
        source: PakSourceInfo,
        zip_comment: Vec<u8>,
        entries: Vec<PakEntry>,
    ) -> Result<Self> {
        let archive = Self {
            options,
            source: Some(source),
            zip_comment,
            entries,
            path_index: OnceLock::new(),
        };
        archive.validate()?;
        Ok(archive)
    }

    pub const fn options(&self) -> EncodeOptions {
        self.options
    }

    pub const fn source_info(&self) -> Option<PakSourceInfo> {
        self.source
    }

    pub fn zip_comment(&self) -> &[u8] {
        &self.zip_comment
    }

    pub fn set_zip_comment(&mut self, comment: impl Into<Vec<u8>>) -> Result<()> {
        if !matches!(self.options.format, PakFormat::TvZip { .. }) {
            return Err(PakError::IncompatibleMetadata {
                entry: None,
                message: "flat PAK archives cannot store a ZIP archive comment",
            });
        }
        let comment = comment.into();
        if comment.len() > u16::MAX as usize {
            return Err(PakError::LimitExceeded {
                resource: "ZIP archive comment bytes",
                requested: comment.len() as u64,
                limit: u16::MAX as u64,
            });
        }
        self.zip_comment = comment;
        Ok(())
    }

    pub fn entries(&self) -> &[PakEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry_at(&self, index: usize) -> Option<&PakEntry> {
        self.entries.get(index)
    }

    pub fn find_entry(&self, path: impl AsRef<[u8]>) -> Option<usize> {
        let path = path.as_ref();
        self.path_index()
            .get(&path_hash(path))
            .into_iter()
            .flatten()
            .copied()
            .find(|index| self.entries[*index].path.as_bytes() == path)
    }

    pub fn entry(&self, path: impl AsRef<[u8]>) -> Option<&PakEntry> {
        self.find_entry(path)
            .and_then(|index| self.entries.get(index))
    }

    pub fn entries_by_path<'a>(
        &'a self,
        path: &'a [u8],
    ) -> impl Iterator<Item = (usize, &'a PakEntry)> + 'a {
        self.path_index()
            .get(&path_hash(path))
            .into_iter()
            .flatten()
            .copied()
            .filter(move |index| self.entries[*index].path.as_bytes() == path)
            .map(|index| (index, &self.entries[index]))
    }

    pub fn push_entry(&mut self, entry: PakEntry) -> Result<usize> {
        validate_entry(self.options.format, self.entries.len(), &entry)?;
        let index = self.entries.len();
        self.entries.push(entry);
        self.invalidate_index();
        Ok(index)
    }

    pub fn insert_entry(&mut self, index: usize, entry: PakEntry) -> Result<()> {
        if index > self.entries.len() {
            return Err(PakError::InvalidEntryIndex {
                index,
                entries: self.entries.len(),
            });
        }
        validate_entry(self.options.format, index, &entry)?;
        self.entries.insert(index, entry);
        self.invalidate_index();
        Ok(())
    }

    pub fn remove_entry(&mut self, index: usize) -> Result<PakEntry> {
        if index >= self.entries.len() {
            return Err(PakError::InvalidEntryIndex {
                index,
                entries: self.entries.len(),
            });
        }
        let entry = self.entries.remove(index);
        self.invalidate_index();
        Ok(entry)
    }

    pub fn rename_entry(&mut self, index: usize, path: impl Into<PakPath>) -> Result<()> {
        let entries = self.entries.len();
        let entry = self
            .entries
            .get_mut(index)
            .ok_or(PakError::InvalidEntryIndex { index, entries })?;
        entry.path = path.into();
        self.invalidate_index();
        Ok(())
    }

    pub fn replace_entry_data(
        &mut self,
        index: usize,
        data: impl Into<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let entries = self.entries.len();
        let entry = self
            .entries
            .get_mut(index)
            .ok_or(PakError::InvalidEntryIndex { index, entries })?;
        if entry.kind == PakEntryKind::Directory {
            return Err(PakError::IncompatibleMetadata {
                entry: Some(index),
                message: "directory entries cannot contain payload data",
            });
        }
        Ok(std::mem::replace(&mut entry.data, data.into()))
    }

    pub fn set_entry_timestamp(&mut self, index: usize, timestamp: PakTimestamp) -> Result<()> {
        let entries = self.entries.len();
        let entry = self
            .entries
            .get_mut(index)
            .ok_or(PakError::InvalidEntryIndex { index, entries })?;
        let previous = entry.timestamp;
        entry.timestamp = timestamp;
        if let Err(error) = validate_entry(self.options.format, index, entry) {
            entry.timestamp = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_entry_zip_metadata(
        &mut self,
        index: usize,
        metadata: Option<ZipEntryMetadata>,
    ) -> Result<()> {
        let entries = self.entries.len();
        let entry = self
            .entries
            .get_mut(index)
            .ok_or(PakError::InvalidEntryIndex { index, entries })?;
        let previous = std::mem::replace(&mut entry.zip, metadata);
        if let Err(error) = validate_entry(self.options.format, index, entry) {
            entry.zip = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_options(
        &mut self,
        options: EncodeOptions,
        conversion: MetadataConversion,
    ) -> Result<Vec<MetadataChange>> {
        let changes = self.set_format(options.format, conversion)?;
        self.options = options;
        Ok(changes)
    }

    pub fn set_format(
        &mut self,
        format: PakFormat,
        conversion: MetadataConversion,
    ) -> Result<Vec<MetadataChange>> {
        if format == self.options.format {
            return Ok(Vec::new());
        }
        if conversion == MetadataConversion::RejectLossy {
            for (index, entry) in self.entries.iter().enumerate() {
                validate_entry(format, index, entry)?;
            }
            if !matches!(format, PakFormat::TvZip { .. }) && !self.zip_comment.is_empty() {
                return Err(PakError::IncompatibleMetadata {
                    entry: None,
                    message: "changing to flat PAK would discard the ZIP archive comment",
                });
            }
            self.options.format = format;
            return Ok(Vec::new());
        }

        let mut changes = Vec::new();
        match format {
            PakFormat::Flat { .. } => {
                let mut retained = Vec::with_capacity(self.entries.len());
                for (index, mut entry) in self.entries.drain(..).enumerate() {
                    if entry.kind != PakEntryKind::File {
                        changes.push(MetadataChange::EntryDropped {
                            entry: index,
                            kind: entry.kind,
                        });
                        continue;
                    }
                    if matches!(entry.timestamp, PakTimestamp::ZipMsDos { .. }) {
                        entry.timestamp = PakTimestamp::None;
                        changes.push(MetadataChange::TimestampDropped { entry: index });
                    }
                    if entry.zip.take().is_some() {
                        changes.push(MetadataChange::ZipMetadataDropped { entry: index });
                    }
                    retained.push(entry);
                }
                self.entries = retained;
                if !self.zip_comment.is_empty() {
                    self.zip_comment.clear();
                    changes.push(MetadataChange::ArchiveCommentDropped);
                }
            }
            PakFormat::TvZip { .. } => {
                for (index, entry) in self.entries.iter_mut().enumerate() {
                    if matches!(entry.timestamp, PakTimestamp::PopCap(_)) {
                        entry.timestamp = PakTimestamp::None;
                        changes.push(MetadataChange::TimestampDropped { entry: index });
                    }
                }
            }
        }
        self.options.format = format;
        self.invalidate_index();
        self.validate()?;
        Ok(changes)
    }

    pub fn validate(&self) -> Result<()> {
        if !matches!(self.options.format, PakFormat::TvZip { .. }) && !self.zip_comment.is_empty() {
            return Err(PakError::IncompatibleMetadata {
                entry: None,
                message: "flat PAK archives cannot store a ZIP archive comment",
            });
        }
        for (index, entry) in self.entries.iter().enumerate() {
            validate_entry(self.options.format, index, entry)?;
        }
        Ok(())
    }

    fn path_index(&self) -> &PathIndex {
        self.path_index
            .get_or_init(|| build_path_index(&self.entries))
    }

    fn invalidate_index(&mut self) {
        self.path_index = OnceLock::new();
    }
}

fn validate_entry(format: PakFormat, index: usize, entry: &PakEntry) -> Result<()> {
    if entry.path.is_empty() {
        return Err(PakError::IncompatibleMetadata {
            entry: Some(index),
            message: "entry path cannot be empty",
        });
    }
    if entry.kind == PakEntryKind::Directory && !entry.data.is_empty() {
        return Err(PakError::IncompatibleMetadata {
            entry: Some(index),
            message: "directory entries cannot contain payload data",
        });
    }
    match format {
        PakFormat::Flat { .. } => {
            if entry.kind != PakEntryKind::File {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(index),
                    message: "flat PAK archives cannot store explicit directory entries",
                });
            }
            if matches!(entry.timestamp, PakTimestamp::ZipMsDos { .. }) {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(index),
                    message: "flat PAK archives cannot store ZIP timestamps",
                });
            }
            if entry.zip.is_some() {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(index),
                    message: "flat PAK archives cannot store ZIP metadata",
                });
            }
        }
        PakFormat::TvZip { .. } => {
            if matches!(entry.timestamp, PakTimestamp::PopCap(_)) {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(index),
                    message: "TV/ZIP archives cannot store PopCap timestamps",
                });
            }
        }
    }
    Ok(())
}

fn build_path_index(entries: &[PakEntry]) -> PathIndex {
    let mut index = HashMap::with_capacity(entries.len());
    for (entry_index, entry) in entries.iter().enumerate() {
        index
            .entry(path_hash(entry.path.as_bytes()))
            .or_insert_with(Vec::new)
            .push(entry_index);
    }
    index
}

pub(crate) fn path_hash(path: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
