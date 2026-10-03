//! Indexed, lazy PAK reader.

use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

use flate2::read::ZlibDecoder;

use crate::archive::{PakArchive, PakEntry, PakEntryInfo, path_hash};
#[cfg(feature = "tv")]
use crate::archive::{PakEntryKind, PakTimestamp, ZipEntryMetadata, ZipExtraField};
use crate::error::{PakError, Result};
use crate::flat::{self, FlatEntry, StoredReader};
use crate::limits::DecodeLimits;
#[cfg(feature = "tv")]
use crate::options::ZipCompression;
use crate::options::{
    CompressionLevel, DecodeOptions, EncodeOptions, FlatProfile, PakCompression, PakFormat,
    PakFormatHint, PakSourceInfo,
};
use crate::wire::{PAK_MAGIC, PC_MAGIC};

enum ReaderBackend<R> {
    Flat(R),
    #[cfg(feature = "tv")]
    Tv(zip::ZipArchive<R>),
}

#[derive(Debug, Clone, Copy)]
enum EntryLocation {
    Flat {
        payload_offset: u64,
        profile: FlatProfile,
        compression: PakCompression,
    },
    #[cfg(feature = "tv")]
    Tv { zip_index: usize },
}

#[derive(Debug, Clone)]
struct IndexedEntry {
    info: PakEntryInfo,
    location: EntryLocation,
}

/// Detached directory index that can be attached to independent handles of
/// the same archive for concurrent entry reads without reparsing metadata.
#[derive(Debug, Clone)]
pub struct PakIndex {
    archive_len: u64,
    source: PakSourceInfo,
    zip_comment: Vec<u8>,
    entries: Vec<IndexedEntry>,
    path_index: HashMap<u64, Vec<usize>>,
    options: DecodeOptions,
}

/// Directory index over a seekable PAK source.
pub struct PakReader<R> {
    backend: ReaderBackend<R>,
    archive_len: u64,
    source: PakSourceInfo,
    zip_comment: Vec<u8>,
    entries: Vec<IndexedEntry>,
    path_index: HashMap<u64, Vec<usize>>,
    options: DecodeOptions,
}

impl<R: Read + Seek> PakReader<R> {
    pub fn new(reader: R) -> Result<Self> {
        Self::with_options(reader, DecodeOptions::default())
    }

    pub fn with_options(mut reader: R, options: DecodeOptions) -> Result<Self> {
        let archive_len = reader.seek(SeekFrom::End(0))?;
        flat::enforce_limit(
            "archive bytes",
            archive_len,
            options.limits.max_archive_bytes,
        )?;
        reader.seek(SeekFrom::Start(0))?;
        let raw_magic = read_magic(&mut reader)?;
        reader.seek(SeekFrom::Start(0))?;

        if is_zip_magic(raw_magic.to_le_bytes()) {
            if matches!(
                options.format_hint,
                PakFormatHint::Exact(PakFormat::Flat { .. })
            ) {
                return Err(PakError::InvalidMagic {
                    found: raw_magic.to_le_bytes(),
                });
            }
            preflight_zip_directory(&mut reader, archive_len, options.limits)?;
            return Self::from_tv(reader, archive_len, options);
        }
        if matches!(
            options.format_hint,
            PakFormatHint::Exact(PakFormat::TvZip { .. })
        ) {
            return Err(PakError::InvalidMagic {
                found: raw_magic.to_le_bytes(),
            });
        }
        if raw_magic != PAK_MAGIC && raw_magic != PC_MAGIC {
            return Err(PakError::InvalidMagic {
                found: raw_magic.to_le_bytes(),
            });
        }

        let index = flat::index(&mut reader, archive_len, raw_magic, options)?;
        let (profile, compression) = match index.source.format {
            PakFormat::Flat {
                profile,
                compression,
            } => (profile, compression),
            PakFormat::TvZip { .. } => unreachable!("flat index returned ZIP format"),
        };
        let entries: Vec<IndexedEntry> = index
            .entries
            .into_iter()
            .map(|entry| flat_indexed_entry(entry, profile, compression))
            .collect();
        let path_index = build_path_index(&entries)?;
        Ok(Self {
            backend: ReaderBackend::Flat(reader),
            archive_len,
            source: index.source,
            zip_comment: Vec::new(),
            entries,
            path_index,
            options,
        })
    }

    pub fn source_info(&self) -> PakSourceInfo {
        self.source
    }

    pub fn index_snapshot(&self) -> PakIndex {
        PakIndex {
            archive_len: self.archive_len,
            source: self.source,
            zip_comment: self.zip_comment.clone(),
            entries: self.entries.clone(),
            path_index: self.path_index.clone(),
            options: self.options,
        }
    }

    pub fn zip_comment(&self) -> &[u8] {
        &self.zip_comment
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &PakEntryInfo> {
        self.entries.iter().map(|entry| &entry.info)
    }

    pub fn entry_info(&self, index: usize) -> Option<&PakEntryInfo> {
        self.entries.get(index).map(|entry| &entry.info)
    }

    pub fn find_entry(&self, path: impl AsRef<[u8]>) -> Option<usize> {
        let path = path.as_ref();
        self.path_index
            .get(&path_hash(path))
            .into_iter()
            .flatten()
            .copied()
            .find(|index| self.entries[*index].info.path.as_bytes() == path)
    }

    pub fn entries_by_path<'a>(
        &'a self,
        path: &'a [u8],
    ) -> impl Iterator<Item = (usize, &'a PakEntryInfo)> + 'a {
        self.path_index
            .get(&path_hash(path))
            .into_iter()
            .flatten()
            .copied()
            .filter(move |index| self.entries[*index].info.path.as_bytes() == path)
            .map(|index| (index, &self.entries[index].info))
    }

    /// Open one payload without materializing other archive entries.
    ///
    /// Reading through EOF validates the declared uncompressed length. Use
    /// [`Self::read_entry`] when exact compressed-input consumption must also
    /// be reported as a [`PakError`].
    pub fn open_entry(&mut self, index: usize) -> Result<PakEntryReader<'_, R>> {
        let entry = self.entries.get(index).ok_or(PakError::InvalidEntryIndex {
            index,
            entries: self.entries.len(),
        })?;
        let expected_size = entry.info.original_size;
        let stored_size = entry.info.stored_size;
        let backend = match (&mut self.backend, entry.location) {
            (
                ReaderBackend::Flat(reader),
                EntryLocation::Flat {
                    payload_offset,
                    profile,
                    compression,
                },
            ) => {
                let stored =
                    StoredReader::new(reader, payload_offset, stored_size, flat::xor_key(profile))?;
                match compression {
                    PakCompression::None => EntryReaderBackend::Stored(stored),
                    PakCompression::Zlib => EntryReaderBackend::Zlib(ZlibDecoder::new(stored)),
                }
            }
            #[cfg(feature = "tv")]
            (ReaderBackend::Tv(archive), EntryLocation::Tv { zip_index }) => {
                EntryReaderBackend::Zip(archive.by_index(zip_index)?)
            }
            #[cfg(feature = "tv")]
            (ReaderBackend::Flat(_), EntryLocation::Tv { .. })
            | (ReaderBackend::Tv(_), EntryLocation::Flat { .. }) => {
                unreachable!("reader backend and entry location diverged")
            }
        };
        Ok(PakEntryReader {
            index,
            backend,
            remaining_output: expected_size,
            stored_size,
            validated: false,
        })
    }

    pub fn read_entry(&mut self, index: usize) -> Result<Vec<u8>> {
        let expected = self
            .entry_info(index)
            .ok_or(PakError::InvalidEntryIndex {
                index,
                entries: self.entries.len(),
            })?
            .original_size;
        let expected_usize = usize::try_from(expected).map_err(|_| PakError::AllocationFailed {
            context: "entry output",
            requested: expected,
        })?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(expected_usize)
            .map_err(|_| PakError::AllocationFailed {
                context: "entry output",
                requested: expected,
            })?;
        self.open_entry(index)?
            .read_to_end(&mut output)
            .map_err(|source| PakError::EntryIo { index, source })?;
        if output.len() != expected_usize {
            return Err(PakError::invalid(
                "entry payload",
                0,
                format!(
                    "entry {index} decoded to {} bytes; expected {expected_usize}",
                    output.len()
                ),
            ));
        }
        Ok(output)
    }

    pub fn read_archive(&mut self) -> Result<PakArchive> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(self.entries.len())
            .map_err(|_| PakError::AllocationFailed {
                context: "archive entries",
                requested: self.entries.len() as u64,
            })?;
        for index in 0..self.entries.len() {
            let info = self.entries[index].info.clone();
            let data = self.read_entry(index)?;
            entries.push(PakEntry::decoded(
                info.path,
                info.kind,
                info.timestamp,
                info.zip,
                data,
            ));
        }
        PakArchive::decoded(
            EncodeOptions {
                format: self.source.format,
                path_separator: self.source.path_separator,
                compression_level: CompressionLevel::Default,
                preserve_zip_metadata: true,
            },
            self.source,
            self.zip_comment.clone(),
            entries,
        )
    }

    pub fn into_archive(mut self) -> Result<PakArchive> {
        self.read_archive()
    }

    pub fn into_inner(self) -> R {
        match self.backend {
            ReaderBackend::Flat(reader) => reader,
            #[cfg(feature = "tv")]
            ReaderBackend::Tv(archive) => archive.into_inner(),
        }
    }

    pub fn options(&self) -> DecodeOptions {
        self.options
    }

    #[cfg(feature = "tv")]
    fn from_tv(reader: R, archive_len: u64, options: DecodeOptions) -> Result<Self> {
        use crate::path::PakPath;

        let mut archive = zip::ZipArchive::new(reader)?;
        let zip_comment = archive.comment().to_vec();
        if archive.len() > options.limits.max_entries {
            return Err(PakError::LimitExceeded {
                resource: "ZIP entry count",
                requested: archive.len() as u64,
                limit: options.limits.max_entries as u64,
            });
        }
        let mut entries = Vec::new();
        let mut total_uncompressed = 0_u64;
        let mut total_path_bytes = 0_u64;
        let mut has_deflated = false;
        for zip_index in 0..archive.len() {
            let file = archive.by_index(zip_index)?;
            if file.encrypted() {
                return Err(PakError::EncryptedZipEntry { entry: zip_index });
            }
            flat::enforce_limit(
                "ZIP stored entry bytes",
                file.compressed_size(),
                options.limits.max_entry_stored_bytes,
            )?;
            flat::enforce_limit(
                "ZIP uncompressed entry bytes",
                file.size(),
                options.limits.max_entry_uncompressed_bytes,
            )?;
            total_uncompressed =
                total_uncompressed
                    .checked_add(file.size())
                    .ok_or(PakError::IntegerOverflow {
                        context: "ZIP total uncompressed bytes",
                    })?;
            flat::enforce_limit(
                "ZIP total uncompressed bytes",
                total_uncompressed,
                options.limits.max_total_uncompressed_bytes,
            )?;
            total_path_bytes = total_path_bytes
                .checked_add(file.name_raw().len() as u64)
                .ok_or(PakError::IntegerOverflow {
                    context: "ZIP total path bytes",
                })?;
            flat::enforce_limit(
                "ZIP total path bytes",
                total_path_bytes,
                options.limits.max_total_path_bytes,
            )?;
            entries
                .try_reserve(1)
                .map_err(|_| PakError::AllocationFailed {
                    context: "ZIP entry metadata",
                    requested: (entries.len() + 1) as u64,
                })?;
            let timestamp =
                file.last_modified()
                    .map_or(PakTimestamp::None, |time| PakTimestamp::ZipMsDos {
                        date: time.datepart(),
                        time: time.timepart(),
                    });
            let compression = if file.compression() == zip::CompressionMethod::Stored {
                ZipCompression::Stored
            } else if file.compression() == zip::CompressionMethod::Deflated {
                ZipCompression::Deflated
            } else {
                return Err(PakError::invalid(
                    "ZIP compression method",
                    zip_index as u64,
                    format!("unsupported method {:?}", file.compression()),
                ));
            };
            has_deflated |= compression == ZipCompression::Deflated;
            let mut zip_metadata = ZipEntryMetadata::new(compression);
            zip_metadata.set_unix_mode(file.unix_mode());
            zip_metadata.set_comment(file.comment());
            for field in parse_zip_extra_fields(file.extra_data(), zip_index)? {
                zip_metadata.add_extra_field(field);
            }
            entries.push(IndexedEntry {
                info: PakEntryInfo {
                    path: PakPath::from(file.name_raw().to_vec()),
                    kind: if file.is_dir() {
                        PakEntryKind::Directory
                    } else if file.is_symlink() {
                        PakEntryKind::Symlink
                    } else {
                        PakEntryKind::File
                    },
                    timestamp,
                    stored_size: file.compressed_size(),
                    original_size: file.size(),
                    zip: Some(zip_metadata),
                },
                location: EntryLocation::Tv { zip_index },
            });
        }
        let separator = flat::detect_path_separator(entries.iter().map(|entry| &entry.info.path));
        let source = PakSourceInfo {
            format: PakFormat::TvZip {
                compression: if has_deflated {
                    ZipCompression::Deflated
                } else {
                    ZipCompression::Stored
                },
            },
            path_separator: separator,
            directory_layout: None,
        };
        let path_index = build_path_index(&entries)?;
        Ok(Self {
            backend: ReaderBackend::Tv(archive),
            archive_len,
            source,
            zip_comment,
            entries,
            path_index,
            options,
        })
    }

    #[cfg(not(feature = "tv"))]
    fn from_tv(_reader: R, _archive_len: u64, _options: DecodeOptions) -> Result<Self> {
        Err(PakError::TvFeatureDisabled)
    }
}

impl PakIndex {
    pub const fn source_info(&self) -> PakSourceInfo {
        self.source
    }

    pub fn zip_comment(&self) -> &[u8] {
        &self.zip_comment
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &PakEntryInfo> {
        self.entries.iter().map(|entry| &entry.info)
    }

    pub fn entry_info(&self, index: usize) -> Option<&PakEntryInfo> {
        self.entries.get(index).map(|entry| &entry.info)
    }

    pub fn find_entry(&self, path: impl AsRef<[u8]>) -> Option<usize> {
        let path = path.as_ref();
        self.path_index
            .get(&path_hash(path))
            .into_iter()
            .flatten()
            .copied()
            .find(|index| self.entries[*index].info.path.as_bytes() == path)
    }

    /// Attach this index to another handle of the same archive. Length and
    /// container magic are checked before the cached offsets are trusted.
    pub fn attach<R: Read + Seek>(&self, mut reader: R) -> Result<PakReader<R>> {
        let length = reader.seek(SeekFrom::End(0))?;
        if length != self.archive_len {
            return Err(PakError::invalid(
                "indexed archive length",
                length,
                format!("expected {} bytes", self.archive_len),
            ));
        }
        reader.seek(SeekFrom::Start(0))?;
        let raw_magic = read_magic(&mut reader)?;
        reader.seek(SeekFrom::Start(0))?;
        let backend = match self.source.format {
            PakFormat::Flat { profile, .. } => {
                let expected = if profile == FlatProfile::PcXor {
                    PC_MAGIC
                } else {
                    PAK_MAGIC
                };
                if raw_magic != expected {
                    return Err(PakError::InvalidMagic {
                        found: raw_magic.to_le_bytes(),
                    });
                }
                ReaderBackend::Flat(reader)
            }
            #[cfg(feature = "tv")]
            PakFormat::TvZip { .. } => {
                if !is_zip_magic(raw_magic.to_le_bytes()) {
                    return Err(PakError::InvalidMagic {
                        found: raw_magic.to_le_bytes(),
                    });
                }
                ReaderBackend::Tv(zip::ZipArchive::new(reader)?)
            }
            #[cfg(not(feature = "tv"))]
            PakFormat::TvZip { .. } => return Err(PakError::TvFeatureDisabled),
        };
        Ok(PakReader {
            backend,
            archive_len: self.archive_len,
            source: self.source,
            zip_comment: self.zip_comment.clone(),
            entries: self.entries.clone(),
            path_index: self.path_index.clone(),
            options: self.options,
        })
    }
}

/// Seekable view over one bounded region of a larger source.
pub struct PakRange<R> {
    inner: R,
    base: u64,
    length: u64,
    position: u64,
}

impl<R: Read + Seek> PakRange<R> {
    pub fn new(mut inner: R, base: u64, length: u64) -> Result<Self> {
        let end = base.checked_add(length).ok_or(PakError::IntegerOverflow {
            context: "PAK source range end",
        })?;
        let source_len = inner.seek(SeekFrom::End(0))?;
        if end > source_len {
            return Err(PakError::invalid(
                "PAK source range",
                base,
                format!("range ends at {end}, source contains {source_len} bytes"),
            ));
        }
        inner.seek(SeekFrom::Start(base))?;
        Ok(Self {
            inner,
            base,
            length,
            position: 0,
        })
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub const fn base(&self) -> u64 {
        self.base
    }

    pub const fn len(&self) -> u64 {
        self.length
    }

    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }
}

impl<R: Read + Seek> Read for PakRange<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.length || buffer.is_empty() {
            return Ok(0);
        }
        let available = usize::try_from(self.length - self.position).unwrap_or(usize::MAX);
        let maximum = available.min(buffer.len());
        self.inner
            .seek(SeekFrom::Start(self.base + self.position))?;
        let read = self.inner.read(&mut buffer[..maximum])?;
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Read + Seek> Seek for PakRange<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let requested = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::End(delta) => i128::from(self.length) + i128::from(delta),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
        };
        if requested < 0 || requested > i128::from(self.length) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek is outside the bounded PAK source range",
            ));
        }
        self.position = requested as u64;
        self.inner
            .seek(SeekFrom::Start(self.base + self.position))?;
        Ok(self.position)
    }
}

impl<R: Read + Seek> PakReader<PakRange<R>> {
    pub fn from_range(reader: R, base: u64, length: u64) -> Result<Self> {
        Self::from_range_with_options(reader, base, length, DecodeOptions::default())
    }

    pub fn from_range_with_options(
        reader: R,
        base: u64,
        length: u64,
        options: DecodeOptions,
    ) -> Result<Self> {
        Self::with_options(PakRange::new(reader, base, length)?, options)
    }
}

fn build_path_index(entries: &[IndexedEntry]) -> Result<HashMap<u64, Vec<usize>>> {
    let mut index = HashMap::new();
    index
        .try_reserve(entries.len())
        .map_err(|_| PakError::AllocationFailed {
            context: "path index",
            requested: entries.len() as u64,
        })?;
    for (entry_index, entry) in entries.iter().enumerate() {
        let indices = index
            .entry(path_hash(entry.info.path.as_bytes()))
            .or_insert_with(Vec::new);
        indices
            .try_reserve(1)
            .map_err(|_| PakError::AllocationFailed {
                context: "duplicate path index",
                requested: indices.len() as u64 + 1,
            })?;
        indices.push(entry_index);
    }
    Ok(index)
}

#[cfg(feature = "tv")]
fn parse_zip_extra_fields(raw: Option<&[u8]>, entry: usize) -> Result<Vec<ZipExtraField>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let mut fields = Vec::new();
    let mut offset = 0_usize;
    while offset < raw.len() {
        if raw.len() - offset < 4 {
            return Err(PakError::invalid(
                "ZIP extra data",
                entry as u64,
                "field header is truncated",
            ));
        }
        let header_id = u16::from_le_bytes([raw[offset], raw[offset + 1]]);
        let length = u16::from_le_bytes([raw[offset + 2], raw[offset + 3]]) as usize;
        offset += 4;
        let end = offset
            .checked_add(length)
            .ok_or(PakError::IntegerOverflow {
                context: "ZIP extra field end",
            })?;
        if end > raw.len() {
            return Err(PakError::invalid(
                "ZIP extra data",
                entry as u64,
                "field payload is truncated",
            ));
        }
        // ZIP64 sizes and offsets are regenerated by the writer.
        if header_id != 0x0001 {
            fields.push(ZipExtraField::new(
                header_id,
                raw[offset..end].to_vec(),
                false,
            ));
        }
        offset = end;
    }
    Ok(fields)
}

fn flat_indexed_entry(
    entry: FlatEntry,
    profile: FlatProfile,
    compression: PakCompression,
) -> IndexedEntry {
    IndexedEntry {
        info: entry.info,
        location: EntryLocation::Flat {
            payload_offset: entry.payload_offset,
            profile,
            compression,
        },
    }
}

enum EntryReaderBackend<'a, R: Read + Seek> {
    Stored(StoredReader<'a, R>),
    Zlib(ZlibDecoder<StoredReader<'a, R>>),
    #[cfg(feature = "tv")]
    Zip(zip::read::ZipFile<'a, R>),
}

/// Streaming reader returned by [`PakReader::open_entry`].
pub struct PakEntryReader<'a, R: Read + Seek> {
    index: usize,
    backend: EntryReaderBackend<'a, R>,
    remaining_output: u64,
    stored_size: u64,
    validated: bool,
}

impl<R: Read + Seek> PakEntryReader<'_, R> {
    /// Drain the remaining payload and complete all length, zlib, and CRC
    /// validation even when the caller stopped reading early.
    pub fn finish(mut self) -> Result<()> {
        let mut buffer = [0_u8; 32 * 1024];
        loop {
            let read = self.read(&mut buffer).map_err(|source| PakError::EntryIo {
                index: self.index,
                source,
            })?;
            if read == 0 {
                return Ok(());
            }
        }
    }

    pub const fn remaining_output(&self) -> u64 {
        self.remaining_output
    }

    pub const fn is_verified(&self) -> bool {
        self.validated
    }

    fn read_backend(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.backend {
            EntryReaderBackend::Stored(reader) => reader.read(buffer),
            EntryReaderBackend::Zlib(reader) => reader.read(buffer),
            #[cfg(feature = "tv")]
            EntryReaderBackend::Zip(reader) => reader.read(buffer),
        }
    }

    fn validate_end(&mut self) -> io::Result<()> {
        if self.validated {
            return Ok(());
        }
        let mut probe = [0_u8; 1];
        if self.read_backend(&mut probe)? != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PAK entry expands beyond its declared size",
            ));
        }
        match &self.backend {
            EntryReaderBackend::Stored(reader) if reader.remaining() != 0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "PAK entry did not consume its declared payload",
                ));
            }
            EntryReaderBackend::Zlib(reader)
                if reader.total_in() != self.stored_size || reader.get_ref().remaining() != 0 =>
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "zlib stream consumed {} of {} stored bytes",
                        reader.total_in(),
                        self.stored_size
                    ),
                ));
            }
            _ => {}
        }
        self.validated = true;
        Ok(())
    }
}

impl<R: Read + Seek> Read for PakEntryReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining_output == 0 {
            self.validate_end()?;
            return Ok(0);
        }
        let maximum = usize::try_from(self.remaining_output)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = self.read_backend(&mut buffer[..maximum])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "PAK entry ended with {} decoded bytes still expected",
                    self.remaining_output
                ),
            ));
        }
        self.remaining_output -= read as u64;
        Ok(read)
    }
}

fn read_magic(reader: &mut impl Read) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    let mut read = 0;
    while read < bytes.len() {
        let count = reader.read(&mut bytes[read..])?;
        if count == 0 {
            break;
        }
        read += count;
    }
    if read < bytes.len() {
        return Err(PakError::InvalidMagic { found: bytes });
    }
    Ok(u32::from_le_bytes(bytes))
}

fn is_zip_magic(bytes: [u8; 4]) -> bool {
    matches!(
        bytes,
        [b'P', b'K', 3, 4] | [b'P', b'K', 5, 6] | [b'P', b'K', 7, 8]
    )
}

fn preflight_zip_directory<R: Read + Seek>(
    reader: &mut R,
    archive_len: u64,
    limits: DecodeLimits,
) -> Result<()> {
    const EOCD_MIN: u64 = 22;
    const MAX_COMMENT: u64 = u16::MAX as u64;
    if archive_len < EOCD_MIN {
        return Err(PakError::invalid(
            "ZIP end record",
            archive_len,
            "archive is shorter than the end-of-central-directory record",
        ));
    }
    let tail_len = archive_len.min(EOCD_MIN + MAX_COMMENT) as usize;
    let tail_start = archive_len - tail_len as u64;
    reader.seek(SeekFrom::Start(tail_start))?;
    let mut tail = vec![0_u8; tail_len];
    reader.read_exact(&mut tail)?;
    let eocd_index = (0..=tail.len() - EOCD_MIN as usize)
        .rev()
        .find(|index| {
            tail[*index..].starts_with(b"PK\x05\x06")
                && tail.len() - *index
                    == EOCD_MIN as usize + read_u16_at(&tail, *index + 20) as usize
        })
        .ok_or_else(|| {
            PakError::invalid(
                "ZIP end record",
                tail_start,
                "end-of-central-directory signature was not found",
            )
        })?;
    let eocd_offset = tail_start + eocd_index as u64;
    let entries16 = read_u16_at(&tail, eocd_index + 10);
    let directory_size32 = read_u32_at(&tail, eocd_index + 12);
    let directory_offset32 = read_u32_at(&tail, eocd_index + 16);
    let (entries, directory_size, directory_offset) = if entries16 == u16::MAX
        || directory_size32 == u32::MAX
        || directory_offset32 == u32::MAX
    {
        if eocd_offset < 20 {
            return Err(PakError::invalid(
                "ZIP64 locator",
                eocd_offset,
                "ZIP64 locator is missing",
            ));
        }
        reader.seek(SeekFrom::Start(eocd_offset - 20))?;
        let mut locator = [0_u8; 20];
        reader.read_exact(&mut locator)?;
        if !locator.starts_with(b"PK\x06\x07") {
            return Err(PakError::invalid(
                "ZIP64 locator",
                eocd_offset - 20,
                "invalid ZIP64 locator signature",
            ));
        }
        let zip64_offset = u64::from_le_bytes(locator[8..16].try_into().unwrap());
        reader.seek(SeekFrom::Start(zip64_offset))?;
        let mut record = [0_u8; 56];
        reader.read_exact(&mut record)?;
        if !record.starts_with(b"PK\x06\x06") {
            return Err(PakError::invalid(
                "ZIP64 end record",
                zip64_offset,
                "invalid ZIP64 end-of-central-directory signature",
            ));
        }
        (
            u64::from_le_bytes(record[32..40].try_into().unwrap()),
            u64::from_le_bytes(record[40..48].try_into().unwrap()),
            u64::from_le_bytes(record[48..56].try_into().unwrap()),
        )
    } else {
        (
            u64::from(entries16),
            u64::from(directory_size32),
            u64::from(directory_offset32),
        )
    };
    flat::enforce_limit("ZIP entry count", entries, limits.max_entries as u64)?;
    flat::enforce_limit(
        "ZIP directory bytes",
        directory_size,
        limits.max_directory_bytes,
    )?;
    let directory_end =
        directory_offset
            .checked_add(directory_size)
            .ok_or(PakError::IntegerOverflow {
                context: "ZIP directory end",
            })?;
    if directory_end > archive_len {
        return Err(PakError::invalid(
            "ZIP directory",
            directory_offset,
            format!("directory ends at {directory_end}, archive contains {archive_len} bytes"),
        ));
    }
    reader.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn read_u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
