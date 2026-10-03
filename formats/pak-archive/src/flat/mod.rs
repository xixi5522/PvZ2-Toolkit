//! Flat PopCap directory parsing and transformed payload I/O.

use std::io::{self, Read, Seek, SeekFrom};

use crate::archive::{PakEntryInfo, PakEntryKind, PakTimestamp};
use crate::error::{FormatAttempt, PakError, Result};
use crate::limits::DecodeLimits;
use crate::options::{
    CompatibilityMode, DecodeOptions, FlatProfile, PakCompression, PakDirectoryLayout, PakFormat,
    PakFormatHint, PakSourceInfo, PathSeparator, ValidationMode,
};
use crate::path::PakPath;
use crate::wire::{PAK_MAGIC, PAK_VERSION, PC_MAGIC, PC_XOR_KEY};

#[derive(Debug, Clone)]
pub(crate) struct FlatEntry {
    pub info: PakEntryInfo,
    pub payload_offset: u64,
}

#[derive(Debug)]
pub(crate) struct FlatIndex {
    pub source: PakSourceInfo,
    pub entries: Vec<FlatEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Candidate {
    profile: FlatProfile,
    compression: PakCompression,
    layout: PakDirectoryLayout,
}

impl Candidate {
    const fn format(self) -> PakFormat {
        PakFormat::Flat {
            profile: self.profile,
            compression: self.compression,
        }
    }

    const fn source(self) -> PakSourceInfo {
        PakSourceInfo {
            format: self.format(),
            path_separator: PathSeparator::Preserve,
            directory_layout: Some(self.layout),
        }
    }
}

pub(crate) fn index<R: Read + Seek>(
    reader: &mut R,
    archive_len: u64,
    raw_magic: u32,
    options: DecodeOptions,
) -> Result<FlatIndex> {
    enforce_limit(
        "archive bytes",
        archive_len,
        options.limits.max_archive_bytes,
    )?;
    let candidates = candidates(raw_magic, options)?;
    let mut successes = Vec::new();
    let mut attempts = Vec::new();
    for candidate in candidates {
        match probe_candidate(reader, archive_len, candidate, options) {
            Ok(entry_count) => successes.push((candidate, entry_count)),
            Err(error) => attempts.push(FormatAttempt {
                candidate: candidate.source(),
                reason: error.to_string(),
            }),
        }
    }

    if successes.is_empty() {
        return Err(PakError::NoMatchingFormat { attempts });
    }

    // Empty archives have no payload from which compression or alignment can
    // be inferred. Auto mode intentionally resolves them to the simplest form.
    if matches!(options.format_hint, PakFormatHint::Auto)
        && successes.iter().all(|(_, entry_count)| *entry_count == 0)
    {
        successes.retain(|(candidate, _)| {
            matches!(
                candidate.format(),
                PakFormat::Flat {
                    compression: PakCompression::None,
                    ..
                }
            )
        });
        if successes.len() > 1 {
            successes.retain(|(candidate, _)| {
                matches!(
                    candidate.format(),
                    PakFormat::Flat {
                        profile: FlatProfile::Plain,
                        ..
                    }
                )
            });
        }
    }

    // When both size orders are indistinguishable (for example equal sizes),
    // canonical Twinning order is the stable choice.
    let canonical_formats = successes
        .iter()
        .filter(|(candidate, _)| candidate.layout == PakDirectoryLayout::CanonicalZlib)
        .map(|(candidate, _)| candidate.format())
        .collect::<Vec<_>>();
    successes.retain(|(candidate, _)| {
        candidate.layout != PakDirectoryLayout::LegacyToolkitZlib
            || !canonical_formats.contains(&candidate.format())
    });

    if successes.len() != 1 {
        return Err(PakError::AmbiguousFormat {
            candidates: successes
                .iter()
                .map(|(candidate, _)| candidate.source())
                .collect(),
        });
    }
    parse_candidate(reader, archive_len, successes.remove(0).0, options)
}

/// Validate a candidate without retaining paths or full entry metadata. This
/// keeps heuristic detection bounded even when several layouts must be tried.
fn probe_candidate<R: Read + Seek>(
    reader: &mut R,
    archive_len: u64,
    candidate: Candidate,
    options: DecodeOptions,
) -> Result<usize> {
    let mut cursor = WireCursor::new(reader, archive_len, xor_key(candidate.profile));
    cursor.seek(0, "PAK header")?;
    let magic = cursor.u32("PAK magic")?;
    if magic != PAK_MAGIC {
        return Err(PakError::InvalidMagic {
            found: magic.to_le_bytes(),
        });
    }
    let version = cursor.u32("PAK version")?;
    if version != PAK_VERSION {
        return Err(PakError::InvalidVersion { found: version });
    }

    let mut sizes = Vec::new();
    let mut total_uncompressed = 0_u64;
    let mut total_path_bytes = 0_u64;
    loop {
        let marker_offset = cursor.position()?;
        let marker = cursor.u8("directory marker")?;
        if marker == 0x80 {
            break;
        }
        if marker != 0 {
            return Err(PakError::InvalidMarker {
                offset: marker_offset,
                marker,
            });
        }
        if sizes.len() >= options.limits.max_entries {
            return Err(PakError::LimitExceeded {
                resource: "entry count",
                requested: sizes.len() as u64 + 1,
                limit: options.limits.max_entries as u64,
            });
        }
        let path_length = cursor.u8("entry path length")? as usize;
        total_path_bytes =
            total_path_bytes
                .checked_add(path_length as u64)
                .ok_or(PakError::IntegerOverflow {
                    context: "total path bytes",
                })?;
        enforce_limit(
            "total path bytes",
            total_path_bytes,
            options.limits.max_total_path_bytes,
        )?;
        cursor.skip(path_length as u64, "entry path")?;
        let first_size = cursor.u32("entry size")?;
        let (stored_size, original_size) = match candidate.layout {
            PakDirectoryLayout::Uncompressed => (first_size, first_size),
            PakDirectoryLayout::CanonicalZlib => (first_size, cursor.u32("entry original size")?),
            PakDirectoryLayout::LegacyToolkitZlib => (cursor.u32("entry stored size")?, first_size),
        };
        let _time = cursor.u64("entry time")?;
        enforce_entry_limits(
            stored_size as u64,
            original_size as u64,
            &mut total_uncompressed,
            options.limits,
        )?;
        sizes
            .try_reserve(1)
            .map_err(|_| PakError::AllocationFailed {
                context: "candidate entry sizes",
                requested: sizes.len() as u64 + 1,
            })?;
        sizes.push((stored_size as u64, original_size as u64));
    }
    enforce_limit(
        "directory bytes",
        cursor.position()?,
        options.limits.max_directory_bytes,
    )?;

    for (stored_size, _) in &sizes {
        if candidate.profile == FlatProfile::Xbox360 {
            read_xbox_alignment(&mut cursor, options.validation)?;
        }
        let payload_offset = cursor.position()?;
        if candidate.compression == PakCompression::Zlib {
            validate_zlib_header_at(&mut cursor, payload_offset, *stored_size)?;
        }
        let payload_end =
            payload_offset
                .checked_add(*stored_size)
                .ok_or(PakError::IntegerOverflow {
                    context: "entry payload end",
                })?;
        cursor.seek(payload_end, "entry payload")?;
    }
    let final_position = cursor.position()?;
    if final_position != archive_len {
        return Err(PakError::invalid(
            "PAK payload",
            final_position,
            format!("{} trailing bytes remain", archive_len - final_position),
        ));
    }
    Ok(sizes.len())
}

fn candidates(raw_magic: u32, options: DecodeOptions) -> Result<Vec<Candidate>> {
    let mut output = Vec::new();
    match options.format_hint {
        PakFormatHint::Exact(PakFormat::TvZip { .. }) => {
            return Err(PakError::InvalidMagic {
                found: raw_magic.to_le_bytes(),
            });
        }
        PakFormatHint::Exact(PakFormat::Flat {
            profile,
            compression,
        }) => {
            let expected = match profile {
                FlatProfile::PcXor => PC_MAGIC,
                FlatProfile::Plain | FlatProfile::Xbox360 => PAK_MAGIC,
            };
            if raw_magic != expected {
                return Err(PakError::InvalidMagic {
                    found: raw_magic.to_le_bytes(),
                });
            }
            append_layouts(&mut output, profile, compression, options.compatibility);
        }
        PakFormatHint::Auto => {
            let profiles: &[FlatProfile] = if raw_magic == PC_MAGIC {
                &[FlatProfile::PcXor]
            } else if raw_magic == PAK_MAGIC {
                &[FlatProfile::Plain, FlatProfile::Xbox360]
            } else {
                return Err(PakError::InvalidMagic {
                    found: raw_magic.to_le_bytes(),
                });
            };
            for profile in profiles {
                append_layouts(
                    &mut output,
                    *profile,
                    PakCompression::None,
                    options.compatibility,
                );
                append_layouts(
                    &mut output,
                    *profile,
                    PakCompression::Zlib,
                    options.compatibility,
                );
            }
        }
    }
    Ok(output)
}

fn append_layouts(
    output: &mut Vec<Candidate>,
    profile: FlatProfile,
    compression: PakCompression,
    compatibility: CompatibilityMode,
) {
    match compression {
        PakCompression::None => output.push(Candidate {
            profile,
            compression,
            layout: PakDirectoryLayout::Uncompressed,
        }),
        PakCompression::Zlib => {
            output.push(Candidate {
                profile,
                compression,
                layout: PakDirectoryLayout::CanonicalZlib,
            });
            if compatibility == CompatibilityMode::LegacyToolkit {
                output.push(Candidate {
                    profile,
                    compression,
                    layout: PakDirectoryLayout::LegacyToolkitZlib,
                });
            }
        }
    }
}

fn parse_candidate<R: Read + Seek>(
    reader: &mut R,
    archive_len: u64,
    candidate: Candidate,
    options: DecodeOptions,
) -> Result<FlatIndex> {
    let xor_key = xor_key(candidate.profile);
    let mut cursor = WireCursor::new(reader, archive_len, xor_key);
    cursor.seek(0, "PAK header")?;
    let magic = cursor.u32("PAK magic")?;
    if magic != PAK_MAGIC {
        return Err(PakError::InvalidMagic {
            found: magic.to_le_bytes(),
        });
    }
    let version = cursor.u32("PAK version")?;
    if version != PAK_VERSION {
        return Err(PakError::InvalidVersion { found: version });
    }

    let mut entries = Vec::new();
    let mut total_uncompressed = 0_u64;
    let mut total_path_bytes = 0_u64;
    loop {
        let marker_offset = cursor.position()?;
        let marker = cursor.u8("directory marker")?;
        if marker == 0x80 {
            break;
        }
        if marker != 0 {
            return Err(PakError::InvalidMarker {
                offset: marker_offset,
                marker,
            });
        }
        if entries.len() >= options.limits.max_entries {
            return Err(PakError::LimitExceeded {
                resource: "entry count",
                requested: entries.len() as u64 + 1,
                limit: options.limits.max_entries as u64,
            });
        }
        entries
            .try_reserve(1)
            .map_err(|_| PakError::AllocationFailed {
                context: "entry metadata",
                requested: (entries.len() + 1) as u64,
            })?;

        let path_length = cursor.u8("entry path length")? as usize;
        total_path_bytes =
            total_path_bytes
                .checked_add(path_length as u64)
                .ok_or(PakError::IntegerOverflow {
                    context: "total path bytes",
                })?;
        enforce_limit(
            "total path bytes",
            total_path_bytes,
            options.limits.max_total_path_bytes,
        )?;
        let path = PakPath::from(cursor.bytes(path_length, "entry path")?);
        let first_size = cursor.u32("entry size")?;
        let (stored_size, original_size) = match candidate.layout {
            PakDirectoryLayout::Uncompressed => (first_size, first_size),
            PakDirectoryLayout::CanonicalZlib => (first_size, cursor.u32("entry original size")?),
            PakDirectoryLayout::LegacyToolkitZlib => (cursor.u32("entry stored size")?, first_size),
        };
        let time = cursor.u64("entry time")?;
        enforce_entry_limits(
            stored_size as u64,
            original_size as u64,
            &mut total_uncompressed,
            options.limits,
        )?;
        entries.push(FlatEntry {
            info: PakEntryInfo {
                path,
                kind: PakEntryKind::File,
                timestamp: PakTimestamp::PopCap(time),
                stored_size: stored_size as u64,
                original_size: original_size as u64,
                zip: None,
            },
            payload_offset: 0,
        });
    }
    enforce_limit(
        "directory bytes",
        cursor.position()?,
        options.limits.max_directory_bytes,
    )?;

    for entry in &mut entries {
        if candidate.profile == FlatProfile::Xbox360 {
            read_xbox_alignment(&mut cursor, options.validation)?;
        }
        entry.payload_offset = cursor.position()?;
        if candidate.compression == PakCompression::Zlib {
            validate_zlib_header(&mut cursor, entry)?;
        }
        let payload_end = entry
            .payload_offset
            .checked_add(entry.info.stored_size)
            .ok_or(PakError::IntegerOverflow {
                context: "entry payload end",
            })?;
        cursor.seek(payload_end, "entry payload")?;
    }
    let final_position = cursor.position()?;
    if final_position != archive_len {
        return Err(PakError::invalid(
            "PAK payload",
            final_position,
            format!("{} trailing bytes remain", archive_len - final_position),
        ));
    }

    let separator = detect_path_separator(entries.iter().map(|entry| &entry.info.path));
    let mut source = candidate.source();
    source.path_separator = separator;
    Ok(FlatIndex { source, entries })
}

fn read_xbox_alignment<R: Read + Seek>(
    cursor: &mut WireCursor<'_, R>,
    validation: ValidationMode,
) -> Result<()> {
    let prefix_offset = cursor.position()?;
    let padding = cursor.u16("payload alignment length")? as usize;
    if validation == ValidationMode::Strict && !(1..=8).contains(&padding) {
        return Err(PakError::invalid(
            "Xbox 360 payload alignment",
            prefix_offset,
            format!("padding length {padding} is outside 1..=8"),
        ));
    }
    let padding_is_zero = cursor.consume_and_check_zero(padding, "payload alignment")?;
    if validation == ValidationMode::Strict && !padding_is_zero {
        return Err(PakError::invalid(
            "Xbox 360 payload alignment",
            prefix_offset,
            "padding contains non-zero bytes",
        ));
    }
    let position = cursor.position()?;
    if position % 8 != 0 {
        return Err(PakError::invalid(
            "Xbox 360 payload alignment",
            position,
            "payload does not begin on an eight-byte boundary",
        ));
    }
    Ok(())
}

fn validate_zlib_header<R: Read + Seek>(
    cursor: &mut WireCursor<'_, R>,
    entry: &FlatEntry,
) -> Result<()> {
    validate_zlib_header_at(cursor, entry.payload_offset, entry.info.stored_size)
}

fn validate_zlib_header_at<R: Read + Seek>(
    cursor: &mut WireCursor<'_, R>,
    payload_offset: u64,
    stored_size: u64,
) -> Result<()> {
    if stored_size < 2 {
        return Err(PakError::invalid(
            "zlib payload",
            payload_offset,
            "payload is shorter than the zlib header",
        ));
    }
    let header = cursor.array::<2>("zlib header")?;
    let cmf = header[0];
    let flg = header[1];
    let header_value = u16::from(cmf) << 8 | u16::from(flg);
    if cmf & 0x0F != 8 || cmf >> 4 > 7 || header_value % 31 != 0 || flg & 0x20 != 0 {
        return Err(PakError::invalid(
            "zlib payload",
            payload_offset,
            format!("invalid zlib header {cmf:02X} {flg:02X}"),
        ));
    }
    Ok(())
}

fn enforce_entry_limits(
    stored_size: u64,
    original_size: u64,
    total_uncompressed: &mut u64,
    limits: DecodeLimits,
) -> Result<()> {
    enforce_limit(
        "stored entry bytes",
        stored_size,
        limits.max_entry_stored_bytes,
    )?;
    enforce_limit(
        "uncompressed entry bytes",
        original_size,
        limits.max_entry_uncompressed_bytes,
    )?;
    *total_uncompressed =
        total_uncompressed
            .checked_add(original_size)
            .ok_or(PakError::IntegerOverflow {
                context: "total uncompressed bytes",
            })?;
    enforce_limit(
        "total uncompressed bytes",
        *total_uncompressed,
        limits.max_total_uncompressed_bytes,
    )
}

pub(crate) fn enforce_limit(resource: &'static str, requested: u64, limit: u64) -> Result<()> {
    if requested > limit {
        Err(PakError::LimitExceeded {
            resource,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn detect_path_separator<'a>(paths: impl Iterator<Item = &'a PakPath>) -> PathSeparator {
    let mut forward = false;
    let mut backward = false;
    for path in paths {
        forward |= path.as_bytes().contains(&b'/');
        backward |= path.as_bytes().contains(&b'\\');
    }
    match (forward, backward) {
        (true, false) => PathSeparator::ForwardSlash,
        (false, true) => PathSeparator::Backslash,
        _ => PathSeparator::Preserve,
    }
}

pub(crate) const fn xor_key(profile: FlatProfile) -> Option<u8> {
    match profile {
        FlatProfile::PcXor => Some(PC_XOR_KEY),
        FlatProfile::Plain | FlatProfile::Xbox360 => None,
    }
}

pub(crate) struct StoredReader<'a, R> {
    reader: &'a mut R,
    remaining: u64,
    xor_key: Option<u8>,
}

impl<'a, R: Read + Seek> StoredReader<'a, R> {
    pub(crate) fn new(
        reader: &'a mut R,
        offset: u64,
        length: u64,
        xor_key: Option<u8>,
    ) -> Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            reader,
            remaining: length,
            xor_key,
        })
    }

    pub(crate) const fn remaining(&self) -> u64 {
        self.remaining
    }
}

impl<R: Read> Read for StoredReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let maximum = usize::try_from(self.remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = self.reader.read(&mut buffer[..maximum])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PAK entry payload ended early",
            ));
        }
        if let Some(key) = self.xor_key {
            for byte in &mut buffer[..read] {
                *byte ^= key;
            }
        }
        self.remaining -= read as u64;
        Ok(read)
    }
}

struct WireCursor<'a, R> {
    reader: &'a mut R,
    archive_len: u64,
    xor_key: Option<u8>,
}

impl<'a, R: Read + Seek> WireCursor<'a, R> {
    const fn new(reader: &'a mut R, archive_len: u64, xor_key: Option<u8>) -> Self {
        Self {
            reader,
            archive_len,
            xor_key,
        }
    }

    fn position(&mut self) -> Result<u64> {
        Ok(self.reader.stream_position()?)
    }

    fn seek(&mut self, position: u64, context: &'static str) -> Result<()> {
        if position > self.archive_len {
            return Err(PakError::invalid(
                context,
                self.archive_len,
                format!("offset {position} exceeds archive length"),
            ));
        }
        self.reader.seek(SeekFrom::Start(position))?;
        Ok(())
    }

    fn bytes(&mut self, length: usize, context: &'static str) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        output
            .try_reserve_exact(length)
            .map_err(|_| PakError::AllocationFailed {
                context,
                requested: length as u64,
            })?;
        output.resize(length, 0);
        self.read_exact(&mut output, context)?;
        Ok(output)
    }

    fn array<const N: usize>(&mut self, context: &'static str) -> Result<[u8; N]> {
        let mut output = [0_u8; N];
        self.read_exact(&mut output, context)?;
        Ok(output)
    }

    fn read_exact(&mut self, output: &mut [u8], context: &'static str) -> Result<()> {
        let position = self.position()?;
        let end = position
            .checked_add(output.len() as u64)
            .ok_or(PakError::IntegerOverflow { context })?;
        if end > self.archive_len {
            return Err(PakError::invalid(
                context,
                position,
                format!(
                    "need {} bytes but only {} remain",
                    output.len(),
                    self.archive_len - position
                ),
            ));
        }
        self.reader.read_exact(output)?;
        if let Some(key) = self.xor_key {
            for byte in output {
                *byte ^= key;
            }
        }
        Ok(())
    }

    fn skip(&mut self, length: u64, context: &'static str) -> Result<()> {
        let position = self.position()?;
        let end = position
            .checked_add(length)
            .ok_or(PakError::IntegerOverflow { context })?;
        self.seek(end, context)
    }

    fn consume_and_check_zero(&mut self, mut length: usize, context: &'static str) -> Result<bool> {
        let mut all_zero = true;
        let mut buffer = [0_u8; 64];
        while length != 0 {
            let chunk = length.min(buffer.len());
            self.read_exact(&mut buffer[..chunk], context)?;
            all_zero &= buffer[..chunk].iter().all(|byte| *byte == 0);
            length -= chunk;
        }
        Ok(all_zero)
    }

    fn u8(&mut self, context: &'static str) -> Result<u8> {
        Ok(self.array::<1>(context)?[0])
    }

    fn u16(&mut self, context: &'static str) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array(context)?))
    }

    fn u32(&mut self, context: &'static str) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array(context)?))
    }

    fn u64(&mut self, context: &'static str) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array(context)?))
    }
}
