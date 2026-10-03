//! Validated, source-streaming PAK writer.

use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression;
#[cfg(feature = "tv")]
use flate2::write::DeflateEncoder;
use flate2::write::ZlibEncoder;

use crate::archive::{PakArchive, PakEntry, PakEntryKind, PakTimestamp, ZipEntryMetadata};
use crate::error::{PakError, Result};
use crate::options::{
    EncodeOptions, FlatProfile, PakCompression, PakDirectoryLayout, PakFormat, ZipCompression,
};
use crate::path::PakPath;
use crate::wire::{PAK_MAGIC, PAK_VERSION, PC_XOR_KEY};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Stage reported by a source-streaming write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePhase {
    Preparing,
    Entry,
    Finishing,
}

/// Monotonic write progress. Returning [`ControlFlow::Break`] from the
/// observer cancels the operation at the next chunk boundary.
#[derive(Debug, Clone, Copy)]
pub struct WriteProgress<'a> {
    pub phase: WritePhase,
    pub entry_index: Option<usize>,
    pub entries_total: usize,
    pub path: Option<&'a PakPath>,
    pub entry_bytes: u64,
    pub entry_total: u64,
    pub total_input_bytes: u64,
    pub total_input_size: u64,
}

/// Non-fatal normalization performed by a canonical writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteWarning {
    LegacyDirectoryLayoutNormalized,
}

/// Summary of a completed write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReport {
    pub bytes_written: u64,
    pub entries_written: usize,
    pub input_bytes: u64,
    pub warnings: Vec<WriteWarning>,
}

/// One entry whose payload is supplied by a reader rather than an owned
/// archive buffer.
pub struct PakWriteEntry<'a> {
    path: PakPath,
    kind: PakEntryKind,
    timestamp: PakTimestamp,
    zip: Option<ZipEntryMetadata>,
    original_size: u64,
    source: Box<dyn Read + 'a>,
}

impl<'a> PakWriteEntry<'a> {
    pub fn new(path: impl Into<PakPath>, original_size: u64, source: impl Read + 'a) -> Self {
        Self {
            path: path.into(),
            kind: PakEntryKind::File,
            timestamp: PakTimestamp::None,
            zip: None,
            original_size,
            source: Box::new(source),
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
            original_size: 0,
            source: Box::new(Cursor::new(&[])),
        }
    }

    pub fn symlink(path: impl Into<PakPath>, target: impl Into<Vec<u8>>) -> Self {
        let target = target.into();
        let mut metadata = ZipEntryMetadata::new(ZipCompression::Stored);
        metadata.set_unix_mode(Some(0o777));
        Self {
            path: path.into(),
            kind: PakEntryKind::Symlink,
            timestamp: PakTimestamp::None,
            zip: Some(metadata),
            original_size: target.len() as u64,
            source: Box::new(Cursor::new(target)),
        }
    }

    pub fn from_entry(entry: &'a PakEntry) -> Self {
        Self {
            path: entry.path().clone(),
            kind: entry.kind(),
            timestamp: entry.timestamp(),
            zip: entry.zip_metadata().cloned(),
            original_size: entry.data().len() as u64,
            source: Box::new(Cursor::new(entry.data())),
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

    pub const fn original_size(&self) -> u64 {
        self.original_size
    }
}

/// Streaming archive plan. Entry sources are consumed exactly once.
pub struct PakWriter<'a> {
    options: EncodeOptions,
    zip_comment: Vec<u8>,
    entries: Vec<PakWriteEntry<'a>>,
    warnings: Vec<WriteWarning>,
}

impl<'a> PakWriter<'a> {
    pub fn new(options: EncodeOptions) -> Self {
        Self {
            options,
            zip_comment: Vec::new(),
            entries: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn from_archive(archive: &'a PakArchive) -> Self {
        let mut warnings = Vec::new();
        if archive.source_info().is_some_and(|source| {
            source.directory_layout == Some(PakDirectoryLayout::LegacyToolkitZlib)
        }) {
            warnings.push(WriteWarning::LegacyDirectoryLayoutNormalized);
        }
        Self {
            options: archive.options(),
            zip_comment: archive.zip_comment().to_vec(),
            entries: archive
                .entries()
                .iter()
                .map(PakWriteEntry::from_entry)
                .collect(),
            warnings,
        }
    }

    pub fn options(&self) -> EncodeOptions {
        self.options
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

    pub fn push_entry(&mut self, entry: PakWriteEntry<'a>) -> Result<usize> {
        validate_write_entry(self.options.format, self.entries.len(), &entry)?;
        let index = self.entries.len();
        self.entries.push(entry);
        Ok(index)
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &PakWriteEntry<'a>> {
        self.entries.iter()
    }

    /// Validate all metadata and wire-size constraints without mutating output.
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.options.format, PakFormat::TvZip { .. }) && !self.zip_comment.is_empty() {
            return Err(PakError::IncompatibleMetadata {
                entry: None,
                message: "flat PAK archives cannot store a ZIP archive comment",
            });
        }
        for (index, entry) in self.entries.iter().enumerate() {
            validate_write_entry(self.options.format, index, entry)?;
            validate_wire_entry(self.options, index, entry)?;
        }
        Ok(())
    }

    pub fn write_seekable<W: Write + Seek>(self, writer: &mut W) -> Result<WriteReport> {
        self.write_seekable_with_progress(writer, |_| ControlFlow::Continue(()))
    }

    pub fn write_seekable_with_progress<W, F>(
        mut self,
        writer: &mut W,
        mut observer: F,
    ) -> Result<WriteReport>
    where
        W: Write + Seek,
        F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
    {
        self.validate()?;
        let total_input_size = self
            .entries
            .iter()
            .try_fold(0_u64, |total, entry| total.checked_add(entry.original_size))
            .ok_or(PakError::IntegerOverflow {
                context: "total writer input size",
            })?;
        emit_progress(
            &mut observer,
            WriteProgress {
                phase: WritePhase::Preparing,
                entry_index: None,
                entries_total: self.entries.len(),
                path: None,
                entry_bytes: 0,
                entry_total: 0,
                total_input_bytes: 0,
                total_input_size,
            },
        )?;
        let start = writer.stream_position()?;
        let input_bytes = match self.options.format {
            PakFormat::Flat { .. } => {
                write_flat(&mut self, writer, total_input_size, &mut observer)?
            }
            PakFormat::TvZip { .. } => {
                write_tv(&mut self, writer, total_input_size, &mut observer)?
            }
        };
        let end = writer.stream_position()?;
        emit_progress(
            &mut observer,
            WriteProgress {
                phase: WritePhase::Finishing,
                entry_index: None,
                entries_total: self.entries.len(),
                path: None,
                entry_bytes: 0,
                entry_total: 0,
                total_input_bytes: input_bytes,
                total_input_size,
            },
        )?;
        Ok(WriteReport {
            bytes_written: end.checked_sub(start).ok_or(PakError::IntegerOverflow {
                context: "written archive length",
            })?,
            entries_written: self.entries.len(),
            input_bytes,
            warnings: self.warnings,
        })
    }
}

/// Encode into a newly allocated byte vector.
pub fn to_bytes(archive: &PakArchive) -> Result<Vec<u8>> {
    Ok(to_bytes_with_report(archive)?.0)
}

pub fn to_bytes_with_report(archive: &PakArchive) -> Result<(Vec<u8>, WriteReport)> {
    archive.validate()?;
    let mut cursor = Cursor::new(Vec::new());
    let report = PakWriter::from_archive(archive).write_seekable(&mut cursor)?;
    Ok((cursor.into_inner(), report))
}

/// Encode to a non-seekable writer through one final archive buffer.
/// Prefer [`to_seekable_writer`] for large archives.
pub fn to_writer(archive: &PakArchive, mut writer: impl Write) -> Result<WriteReport> {
    let (bytes, report) = to_bytes_with_report(archive)?;
    writer.write_all(&bytes)?;
    Ok(report)
}

/// Encode directly at the destination's current position. The caller remains
/// responsible for truncating an existing destination after the returned byte
/// count when necessary.
pub fn to_seekable_writer<W: Write + Seek>(
    archive: &PakArchive,
    writer: &mut W,
) -> Result<WriteReport> {
    archive.validate()?;
    PakWriter::from_archive(archive).write_seekable(writer)
}

/// Transactionally replace a filesystem path using a temporary file in the
/// same directory. A failed or cancelled write leaves the destination intact.
pub fn to_path_atomic(archive: &PakArchive, path: impl AsRef<Path>) -> Result<WriteReport> {
    to_path_atomic_with_progress(archive, path, |_| ControlFlow::Continue(()))
}

pub fn to_path_atomic_with_progress<F>(
    archive: &PakArchive,
    path: impl AsRef<Path>,
    observer: F,
) -> Result<WriteReport>
where
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    archive.validate()?;
    let path = path.as_ref();
    let (temporary_path, mut temporary) = create_temporary_sibling(path)?;
    let result = PakWriter::from_archive(archive)
        .write_seekable_with_progress(&mut temporary, observer)
        .and_then(|report| {
            temporary.flush()?;
            temporary.sync_all()?;
            Ok(report)
        });
    drop(temporary);
    match result {
        Ok(report) => match std::fs::rename(&temporary_path, path) {
            Ok(()) => Ok(report),
            Err(error) => {
                let _ = std::fs::remove_file(&temporary_path);
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = std::fs::remove_file(&temporary_path);
            Err(error)
        }
    }
}

fn write_flat<W, F>(
    archive: &mut PakWriter<'_>,
    writer: &mut W,
    total_input_size: u64,
    observer: &mut F,
) -> Result<u64>
where
    W: Write + Seek,
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    let PakFormat::Flat {
        profile,
        compression,
    } = archive.options.format
    else {
        unreachable!("flat writer called for ZIP format")
    };
    let base = writer.stream_position()?;
    let mut output = WireWriter::new(writer, base, profile == FlatProfile::PcXor);
    output.write_u32(PAK_MAGIC)?;
    output.write_u32(PAK_VERSION)?;

    let mut size_patches = Vec::new();
    size_patches
        .try_reserve_exact(archive.entries.len())
        .map_err(|_| PakError::AllocationFailed {
            context: "directory size patches",
            requested: archive.entries.len() as u64,
        })?;
    for entry in &archive.entries {
        let path = entry
            .path
            .normalized(archive.options.path_separator)
            .into_owned();
        output.write_all(&[0, path.len() as u8])?;
        output.write_all(&path)?;
        let size_offset = output.logical_position()?;
        let original_size = entry.original_size as u32;
        output.write_u32(if compression == PakCompression::None {
            original_size
        } else {
            0
        })?;
        if compression == PakCompression::Zlib {
            output.write_u32(original_size)?;
        }
        let timestamp = match entry.timestamp {
            PakTimestamp::PopCap(value) => value,
            PakTimestamp::None => 0,
            PakTimestamp::ZipMsDos { .. } => unreachable!("metadata was validated"),
        };
        output.write_u64(timestamp)?;
        size_patches.push(size_offset);
    }
    output.write_all(&[0x80])?;

    let mut stored_sizes = Vec::with_capacity(archive.entries.len());
    let mut total_input_bytes = 0_u64;
    let entries_total = archive.entries.len();
    for (index, entry) in archive.entries.iter_mut().enumerate() {
        if profile == FlatProfile::Xbox360 {
            write_xbox_alignment(&mut output)?;
        }
        let payload_start = output.logical_position()?;
        match compression {
            PakCompression::None => {
                copy_entry_source(
                    index,
                    entry,
                    &mut output,
                    &mut total_input_bytes,
                    total_input_size,
                    entries_total,
                    observer,
                )?;
            }
            PakCompression::Zlib => {
                let level = Compression::new(archive.options.compression_level.zlib_level());
                let mut encoder = ZlibEncoder::new(&mut output, level);
                copy_entry_source(
                    index,
                    entry,
                    &mut encoder,
                    &mut total_input_bytes,
                    total_input_size,
                    entries_total,
                    observer,
                )?;
                encoder.finish()?;
            }
        }
        let stored_size = output
            .logical_position()?
            .checked_sub(payload_start)
            .ok_or(PakError::IntegerOverflow {
                context: "stored entry size",
            })?;
        stored_sizes.push(
            u32::try_from(stored_size).map_err(|_| PakError::LimitExceeded {
                resource: "stored entry bytes",
                requested: stored_size,
                limit: u32::MAX as u64,
            })?,
        );
    }

    let archive_end = output.absolute_position()?;
    for (offset, size) in size_patches.into_iter().zip(stored_sizes) {
        output.seek_logical(offset)?;
        output.write_u32(size)?;
    }
    output.seek_absolute(archive_end)?;
    output.flush()?;
    Ok(total_input_bytes)
}

#[cfg(feature = "tv")]
fn write_tv<W, F>(
    archive: &mut PakWriter<'_>,
    writer: &mut W,
    total_input_size: u64,
    observer: &mut F,
) -> Result<u64>
where
    W: Write + Seek,
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    let PakFormat::TvZip {
        compression: default_compression,
    } = archive.options.format
    else {
        unreachable!("ZIP writer called for flat format")
    };
    let base = writer.stream_position()?;
    let mut total_input_bytes = 0_u64;
    let entries_total = archive.entries.len();
    let mut central_entries = Vec::with_capacity(entries_total);
    let mut needs_zip64 = entries_total > u16::MAX as usize;
    for (index, entry) in archive.entries.iter_mut().enumerate() {
        let path = entry
            .path
            .normalized(archive.options.path_separator)
            .into_owned();
        let compression = if archive.options.preserve_zip_metadata {
            entry
                .zip
                .as_ref()
                .map_or(default_compression, ZipEntryMetadata::compression)
        } else {
            default_compression
        };
        let method = zip_method_code(compression);
        let (date, time) = match entry.timestamp {
            PakTimestamp::ZipMsDos { date, time } => (date, time),
            PakTimestamp::None => (0x0021, 0),
            PakTimestamp::PopCap(_) => unreachable!("metadata was validated"),
        };
        let metadata = archive
            .options
            .preserve_zip_metadata
            .then(|| entry.zip.clone())
            .flatten();
        let local_offset =
            writer
                .stream_position()?
                .checked_sub(base)
                .ok_or(PakError::IntegerOverflow {
                    context: "ZIP local header offset",
                })?;
        let large = entry.original_size > u32::MAX as u64 || local_offset > u32::MAX as u64;
        needs_zip64 |= large;
        let mut local_extra = zip_extra_bytes(metadata.as_ref(), false, index)?;
        let zip64_extra_offset = if large {
            let offset = local_extra.len();
            append_zip_extra(&mut local_extra, 0x0001, &[0_u8; 16], index)?;
            Some(offset + 4)
        } else {
            None
        };
        let flags = if std::str::from_utf8(&path).is_ok() {
            1 << 11
        } else {
            0
        };

        write_u32(writer, 0x0403_4B50)?;
        write_u16(writer, if large { 45 } else { 20 })?;
        write_u16(writer, flags)?;
        write_u16(writer, method)?;
        write_u16(writer, time)?;
        write_u16(writer, date)?;
        write_u32(writer, 0)?;
        write_u32(writer, if large { u32::MAX } else { 0 })?;
        write_u32(writer, if large { u32::MAX } else { 0 })?;
        write_u16(writer, path.len() as u16)?;
        write_u16(writer, local_extra.len() as u16)?;
        writer.write_all(&path)?;
        let local_extra_start = writer.stream_position()?;
        writer.write_all(&local_extra)?;
        let payload_start = writer.stream_position()?;

        let crc32;
        if entry.kind == PakEntryKind::Directory {
            verify_empty_source(index, entry)?;
            crc32 = crc32fast::hash(&[]);
            emit_entry_progress(
                observer,
                index,
                (entries_total, total_input_size),
                &entry.path,
                0,
                0,
                total_input_bytes,
            )?;
        } else {
            match compression {
                ZipCompression::Stored => {
                    let mut crc_writer = CrcWriter::new(&mut *writer);
                    copy_entry_source(
                        index,
                        entry,
                        &mut crc_writer,
                        &mut total_input_bytes,
                        total_input_size,
                        entries_total,
                        observer,
                    )?;
                    crc32 = crc_writer.finish().1;
                }
                ZipCompression::Deflated => {
                    let level = Compression::new(archive.options.compression_level.zlib_level());
                    let encoder = DeflateEncoder::new(&mut *writer, level);
                    let mut crc_writer = CrcWriter::new(encoder);
                    copy_entry_source(
                        index,
                        entry,
                        &mut crc_writer,
                        &mut total_input_bytes,
                        total_input_size,
                        entries_total,
                        observer,
                    )?;
                    let (encoder, crc) = crc_writer.finish();
                    encoder.finish()?;
                    crc32 = crc;
                }
            }
        }
        let payload_end = writer.stream_position()?;
        let compressed_size =
            payload_end
                .checked_sub(payload_start)
                .ok_or(PakError::IntegerOverflow {
                    context: "ZIP compressed entry size",
                })?;
        if !large && compressed_size > u32::MAX as u64 {
            return Err(PakError::LimitExceeded {
                resource: "ZIP compressed entry bytes",
                requested: compressed_size,
                limit: u32::MAX as u64,
            });
        }

        writer.seek(SeekFrom::Start(base + local_offset + 14))?;
        write_u32(writer, crc32)?;
        write_u32(
            writer,
            if large {
                u32::MAX
            } else {
                compressed_size as u32
            },
        )?;
        write_u32(
            writer,
            if large {
                u32::MAX
            } else {
                entry.original_size as u32
            },
        )?;
        if let Some(extra_offset) = zip64_extra_offset {
            writer.seek(SeekFrom::Start(local_extra_start + extra_offset as u64))?;
            write_u64(writer, entry.original_size)?;
            write_u64(writer, compressed_size)?;
        }
        writer.seek(SeekFrom::Start(payload_end))?;

        central_entries.push(CentralZipEntry {
            path,
            kind: entry.kind,
            flags,
            method,
            date,
            time,
            crc32,
            compressed_size,
            original_size: entry.original_size,
            local_offset,
            unix_mode: metadata.as_ref().and_then(|value| value.unix_mode()),
            comment: metadata
                .as_ref()
                .map_or_else(Vec::new, |value| value.comment().as_bytes().to_vec()),
            extra: zip_extra_bytes(metadata.as_ref(), true, index)?,
        });
    }

    let central_start =
        writer
            .stream_position()?
            .checked_sub(base)
            .ok_or(PakError::IntegerOverflow {
                context: "ZIP central directory offset",
            })?;
    for (index, entry) in central_entries.iter().enumerate() {
        write_central_entry(writer, entry, index, &mut needs_zip64)?;
    }
    let central_end =
        writer
            .stream_position()?
            .checked_sub(base)
            .ok_or(PakError::IntegerOverflow {
                context: "ZIP central directory end",
            })?;
    let central_size = central_end
        .checked_sub(central_start)
        .ok_or(PakError::IntegerOverflow {
            context: "ZIP central directory size",
        })?;
    needs_zip64 |= central_start > u32::MAX as u64 || central_size > u32::MAX as u64;
    if needs_zip64 {
        let zip64_eocd_offset =
            writer
                .stream_position()?
                .checked_sub(base)
                .ok_or(PakError::IntegerOverflow {
                    context: "ZIP64 end record offset",
                })?;
        write_u32(writer, 0x0606_4B50)?;
        write_u64(writer, 44)?;
        write_u16(writer, (3 << 8) | 45)?;
        write_u16(writer, 45)?;
        write_u32(writer, 0)?;
        write_u32(writer, 0)?;
        write_u64(writer, entries_total as u64)?;
        write_u64(writer, entries_total as u64)?;
        write_u64(writer, central_size)?;
        write_u64(writer, central_start)?;
        write_u32(writer, 0x0706_4B50)?;
        write_u32(writer, 0)?;
        write_u64(writer, zip64_eocd_offset)?;
        write_u32(writer, 1)?;
    }
    write_u32(writer, 0x0605_4B50)?;
    write_u16(writer, 0)?;
    write_u16(writer, 0)?;
    write_u16(writer, u16::try_from(entries_total).unwrap_or(u16::MAX))?;
    write_u16(writer, u16::try_from(entries_total).unwrap_or(u16::MAX))?;
    write_u32(writer, u32::try_from(central_size).unwrap_or(u32::MAX))?;
    write_u32(writer, u32::try_from(central_start).unwrap_or(u32::MAX))?;
    write_u16(writer, archive.zip_comment.len() as u16)?;
    writer.write_all(&archive.zip_comment)?;
    writer.flush()?;
    Ok(total_input_bytes)
}

#[cfg(feature = "tv")]
struct CentralZipEntry {
    path: Vec<u8>,
    kind: PakEntryKind,
    flags: u16,
    method: u16,
    date: u16,
    time: u16,
    crc32: u32,
    compressed_size: u64,
    original_size: u64,
    local_offset: u64,
    unix_mode: Option<u32>,
    comment: Vec<u8>,
    extra: Vec<u8>,
}

#[cfg(feature = "tv")]
fn write_central_entry(
    writer: &mut impl Write,
    entry: &CentralZipEntry,
    index: usize,
    needs_zip64: &mut bool,
) -> Result<()> {
    let large = entry.compressed_size > u32::MAX as u64
        || entry.original_size > u32::MAX as u64
        || entry.local_offset > u32::MAX as u64;
    *needs_zip64 |= large;
    let mut extra = entry.extra.clone();
    if large {
        let mut zip64 = Vec::with_capacity(24);
        zip64.extend_from_slice(&entry.original_size.to_le_bytes());
        zip64.extend_from_slice(&entry.compressed_size.to_le_bytes());
        zip64.extend_from_slice(&entry.local_offset.to_le_bytes());
        append_zip_extra(&mut extra, 0x0001, &zip64, index)?;
    }
    if entry.comment.len() > u16::MAX as usize {
        return Err(PakError::LimitExceeded {
            resource: "ZIP entry comment bytes",
            requested: entry.comment.len() as u64,
            limit: u16::MAX as u64,
        });
    }
    let file_type = match entry.kind {
        PakEntryKind::File => 0o100_000,
        PakEntryKind::Directory => 0o040_000,
        PakEntryKind::Symlink => 0o120_000,
    };
    let unix_mode = entry.unix_mode.unwrap_or(match entry.kind {
        PakEntryKind::File => 0o644,
        PakEntryKind::Directory => 0o755,
        PakEntryKind::Symlink => 0o777,
    });
    let external_attributes = ((file_type | (unix_mode & 0o7777)) << 16)
        | (u32::from(entry.kind == PakEntryKind::Directory) * 0x10);

    write_u32(writer, 0x0201_4B50)?;
    write_u16(writer, (3 << 8) | if large { 45 } else { 20 })?;
    write_u16(writer, if large { 45 } else { 20 })?;
    write_u16(writer, entry.flags)?;
    write_u16(writer, entry.method)?;
    write_u16(writer, entry.time)?;
    write_u16(writer, entry.date)?;
    write_u32(writer, entry.crc32)?;
    write_u32(
        writer,
        if large {
            u32::MAX
        } else {
            entry.compressed_size as u32
        },
    )?;
    write_u32(
        writer,
        if large {
            u32::MAX
        } else {
            entry.original_size as u32
        },
    )?;
    write_u16(writer, entry.path.len() as u16)?;
    write_u16(writer, extra.len() as u16)?;
    write_u16(writer, entry.comment.len() as u16)?;
    write_u16(writer, 0)?;
    write_u16(writer, 0)?;
    write_u32(writer, external_attributes)?;
    write_u32(
        writer,
        if large {
            u32::MAX
        } else {
            entry.local_offset as u32
        },
    )?;
    writer.write_all(&entry.path)?;
    writer.write_all(&extra)?;
    writer.write_all(&entry.comment)?;
    Ok(())
}

#[cfg(feature = "tv")]
fn zip_extra_bytes(
    metadata: Option<&ZipEntryMetadata>,
    central: bool,
    entry: usize,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    if let Some(metadata) = metadata {
        for field in metadata.extra_fields() {
            if field.header_id() == 0x0001 {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(entry),
                    message: "ZIP64 extra data is generated by the writer and cannot be supplied",
                });
            }
            if central || !field.central_only() {
                append_zip_extra(&mut output, field.header_id(), field.data(), entry)?;
            }
        }
    }
    Ok(output)
}

#[cfg(feature = "tv")]
fn append_zip_extra(output: &mut Vec<u8>, header_id: u16, data: &[u8], entry: usize) -> Result<()> {
    if data.len() > u16::MAX as usize {
        return Err(PakError::LimitExceeded {
            resource: "ZIP extra field bytes",
            requested: data.len() as u64,
            limit: u16::MAX as u64,
        });
    }
    let new_length = output
        .len()
        .checked_add(4)
        .and_then(|length| length.checked_add(data.len()))
        .ok_or(PakError::IntegerOverflow {
            context: "ZIP extra data length",
        })?;
    if new_length > u16::MAX as usize {
        return Err(PakError::invalid(
            "ZIP extra data",
            entry as u64,
            "combined extra fields exceed 65535 bytes",
        ));
    }
    output.extend_from_slice(&header_id.to_le_bytes());
    output.extend_from_slice(&(data.len() as u16).to_le_bytes());
    output.extend_from_slice(data);
    Ok(())
}

#[cfg(feature = "tv")]
struct CrcWriter<W> {
    inner: W,
    crc: crc32fast::Hasher,
}

#[cfg(feature = "tv")]
impl<W> CrcWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            crc: crc32fast::Hasher::new(),
        }
    }

    fn finish(self) -> (W, u32) {
        (self.inner, self.crc.finalize())
    }
}

#[cfg(feature = "tv")]
impl<W: Write> Write for CrcWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.crc.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(feature = "tv")]
fn write_u16(writer: &mut impl Write, value: u16) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

#[cfg(feature = "tv")]
fn write_u32(writer: &mut impl Write, value: u32) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

#[cfg(feature = "tv")]
fn write_u64(writer: &mut impl Write, value: u64) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

#[cfg(not(feature = "tv"))]
fn write_tv<W, F>(
    _archive: &mut PakWriter<'_>,
    _writer: &mut W,
    _total_input_size: u64,
    _observer: &mut F,
) -> Result<u64>
where
    W: Write + Seek,
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    Err(PakError::TvFeatureDisabled)
}

fn copy_entry_source<W, F>(
    index: usize,
    entry: &mut PakWriteEntry<'_>,
    output: &mut W,
    total_input_bytes: &mut u64,
    total_input_size: u64,
    entries_total: usize,
    observer: &mut F,
) -> Result<()>
where
    W: Write,
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    let mut entry_bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    emit_entry_progress(
        observer,
        index,
        (entries_total, total_input_size),
        &entry.path,
        0,
        entry.original_size,
        *total_input_bytes,
    )?;
    while entry_bytes < entry.original_size {
        let remaining = usize::try_from(entry.original_size - entry_bytes).unwrap_or(usize::MAX);
        let maximum = remaining.min(buffer.len());
        let read = loop {
            match entry.source.read(&mut buffer[..maximum]) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => break result?,
            }
        };
        if read == 0 {
            return Err(PakError::SourceLengthMismatch {
                entry: index,
                declared: entry.original_size,
                consumed: entry_bytes,
            });
        }
        output.write_all(&buffer[..read])?;
        entry_bytes += read as u64;
        *total_input_bytes =
            total_input_bytes
                .checked_add(read as u64)
                .ok_or(PakError::IntegerOverflow {
                    context: "writer input progress",
                })?;
        emit_entry_progress(
            observer,
            index,
            (entries_total, total_input_size),
            &entry.path,
            entry_bytes,
            entry.original_size,
            *total_input_bytes,
        )?;
    }
    let mut probe = [0_u8; 1];
    if entry.source.read(&mut probe)? != 0 {
        return Err(PakError::SourceLengthMismatch {
            entry: index,
            declared: entry.original_size,
            consumed: entry.original_size + 1,
        });
    }
    Ok(())
}

#[cfg(feature = "tv")]
fn verify_empty_source(index: usize, entry: &mut PakWriteEntry<'_>) -> Result<()> {
    let mut probe = [0_u8; 1];
    if entry.source.read(&mut probe)? != 0 {
        return Err(PakError::SourceLengthMismatch {
            entry: index,
            declared: 0,
            consumed: 1,
        });
    }
    Ok(())
}

fn emit_entry_progress<F>(
    observer: &mut F,
    entry_index: usize,
    totals: (usize, u64),
    path: &PakPath,
    entry_bytes: u64,
    entry_total: u64,
    total_input_bytes: u64,
) -> Result<()>
where
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    emit_progress(
        observer,
        WriteProgress {
            phase: WritePhase::Entry,
            entry_index: Some(entry_index),
            entries_total: totals.0,
            path: Some(path),
            entry_bytes,
            entry_total,
            total_input_bytes,
            total_input_size: totals.1,
        },
    )
}

fn emit_progress<F>(observer: &mut F, progress: WriteProgress<'_>) -> Result<()>
where
    F: FnMut(&WriteProgress<'_>) -> ControlFlow<()>,
{
    if observer(&progress).is_break() {
        Err(PakError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_write_entry(format: PakFormat, index: usize, entry: &PakWriteEntry<'_>) -> Result<()> {
    if entry.path.is_empty() {
        return Err(PakError::IncompatibleMetadata {
            entry: Some(index),
            message: "entry path cannot be empty",
        });
    }
    if entry.kind == PakEntryKind::Directory && entry.original_size != 0 {
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
            if matches!(entry.timestamp, PakTimestamp::ZipMsDos { .. }) || entry.zip.is_some() {
                return Err(PakError::IncompatibleMetadata {
                    entry: Some(index),
                    message: "flat PAK entry contains ZIP-only metadata",
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

fn validate_wire_entry(
    options: EncodeOptions,
    index: usize,
    entry: &PakWriteEntry<'_>,
) -> Result<()> {
    let path = entry.path.normalized(options.path_separator);
    match options.format {
        PakFormat::Flat { .. } => {
            if path.len() > u8::MAX as usize {
                return Err(PakError::invalid(
                    "entry path",
                    index as u64,
                    format!("{} bytes exceeds the 255-byte format limit", path.len()),
                ));
            }
            if entry.original_size > u32::MAX as u64 {
                return Err(PakError::LimitExceeded {
                    resource: "entry bytes",
                    requested: entry.original_size,
                    limit: u32::MAX as u64,
                });
            }
        }
        PakFormat::TvZip { .. } => {
            #[cfg(not(feature = "tv"))]
            return Err(PakError::TvFeatureDisabled);
            #[cfg(feature = "tv")]
            {
                if path.len() > u16::MAX as usize {
                    return Err(PakError::LimitExceeded {
                        resource: "ZIP entry path bytes",
                        requested: path.len() as u64,
                        limit: u16::MAX as u64,
                    });
                }
                if entry.kind == PakEntryKind::Directory && !path.ends_with(b"/") {
                    return Err(PakError::IncompatibleMetadata {
                        entry: Some(index),
                        message: "ZIP directory paths must end with a forward slash",
                    });
                }
                if let PakTimestamp::ZipMsDos { date, time } = entry.timestamp {
                    zip::DateTime::try_from_msdos(date, time).map_err(|_| {
                        PakError::invalid(
                            "ZIP timestamp",
                            index as u64,
                            format!("invalid MS-DOS timestamp {date:04X}:{time:04X}"),
                        )
                    })?;
                }
                if options.preserve_zip_metadata
                    && let Some(metadata) = &entry.zip
                {
                    if metadata.comment().len() > u16::MAX as usize {
                        return Err(PakError::LimitExceeded {
                            resource: "ZIP entry comment bytes",
                            requested: metadata.comment().len() as u64,
                            limit: u16::MAX as u64,
                        });
                    }
                    let _ = zip_extra_bytes(Some(metadata), false, index)?;
                    let _ = zip_extra_bytes(Some(metadata), true, index)?;
                }
            }
        }
    }
    Ok(())
}

fn write_xbox_alignment<W: Write + Seek>(output: &mut WireWriter<'_, W>) -> Result<()> {
    let remainder = output.logical_position()? % 8;
    let padding = if remainder <= 5 {
        6 - remainder
    } else {
        14 - remainder
    };
    output.write_u16(padding as u16)?;
    output.write_all(&[0_u8; 8][..padding as usize])?;
    debug_assert_eq!(output.logical_position()? % 8, 0);
    Ok(())
}

#[cfg(feature = "tv")]
const fn zip_method_code(compression: ZipCompression) -> u16 {
    match compression {
        ZipCompression::Stored => 0,
        ZipCompression::Deflated => 8,
    }
}

fn create_temporary_sibling(path: &Path) -> Result<(PathBuf, File)> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pak-output");
    for _ in 0..128 {
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(PakError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary PAK path",
    )))
}

struct WireWriter<'a, W> {
    writer: &'a mut W,
    base: u64,
    xor: bool,
}

impl<'a, W: Write + Seek> WireWriter<'a, W> {
    const fn new(writer: &'a mut W, base: u64, xor: bool) -> Self {
        Self { writer, base, xor }
    }

    fn absolute_position(&mut self) -> Result<u64> {
        Ok(self.writer.stream_position()?)
    }

    fn logical_position(&mut self) -> Result<u64> {
        self.absolute_position()?
            .checked_sub(self.base)
            .ok_or(PakError::IntegerOverflow {
                context: "logical output position",
            })
    }

    fn seek_logical(&mut self, position: u64) -> Result<()> {
        let absolute = self
            .base
            .checked_add(position)
            .ok_or(PakError::IntegerOverflow {
                context: "absolute output position",
            })?;
        self.seek_absolute(absolute)
    }

    fn seek_absolute(&mut self, position: u64) -> Result<()> {
        self.writer.seek(SeekFrom::Start(position))?;
        Ok(())
    }

    fn write_u16(&mut self, value: u16) -> Result<()> {
        self.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    fn write_u32(&mut self, value: u32) -> Result<()> {
        self.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    fn write_u64(&mut self, value: u64) -> Result<()> {
        self.write_all(&value.to_le_bytes())?;
        Ok(())
    }
}

impl<W: Write> Write for WireWriter<'_, W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if !self.xor {
            return self.writer.write(buffer);
        }
        let mut transformed = [0_u8; 8192];
        let length = transformed.len().min(buffer.len());
        if length == 0 {
            return Ok(0);
        }
        for (output, input) in transformed[..length].iter_mut().zip(buffer) {
            *output = *input ^ PC_XOR_KEY;
        }
        self.writer.write(&transformed[..length])
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
