//! Pure Rust indexed reader and canonical writer for PopCap PAK archives.
//!
//! The crate supports XOR-obfuscated PC archives, canonical plain archives,
//! aligned Xbox 360 archives, and (with the default `tv` feature) ZIP-backed
//! TV archives. [`PakReader`] indexes seekable inputs and materializes only the
//! entry being read. [`PakWriter`] accepts independent `Read` sources so large
//! archives can also be created without owning every payload. Flat writers emit Twinning's canonical
//! `stored_size`, `original_size`, `time` directory order while the compatible
//! decoder can also read the historical Toolkit order.

#![forbid(unsafe_code)]

mod archive;
pub mod error;
mod flat;
pub mod limits;
mod options;
mod path;
mod reader;
mod wire;
mod writer;

use std::io::{Cursor, Read, Seek};

pub use archive::*;
pub use error::{FormatAttempt, PakError, Result};
pub use limits::DecodeLimits;
pub use options::*;
pub use path::{PakPath, PakPathError};
pub use reader::{PakEntryReader, PakIndex, PakRange, PakReader};
pub use writer::{
    PakWriteEntry, PakWriter, WritePhase, WriteProgress, WriteReport, WriteWarning, to_bytes,
    to_bytes_with_report, to_path_atomic, to_path_atomic_with_progress, to_seekable_writer,
    to_writer,
};

/// Decode one complete PAK byte slice with default resource limits.
pub fn from_bytes(bytes: &[u8]) -> Result<PakArchive> {
    from_bytes_with_options(bytes, DecodeOptions::default())
}

/// Decode one complete PAK byte slice with caller-provided resource limits.
pub fn from_bytes_with_limits(bytes: &[u8], limits: DecodeLimits) -> Result<PakArchive> {
    from_bytes_with_options(
        bytes,
        DecodeOptions {
            limits,
            ..DecodeOptions::default()
        },
    )
}

/// Decode one complete byte slice with full caller-provided options.
pub fn from_bytes_with_options(bytes: &[u8], options: DecodeOptions) -> Result<PakArchive> {
    PakReader::with_options(Cursor::new(bytes), options)?.into_archive()
}

/// Read and decode one complete PAK with default resource limits.
pub fn from_reader(reader: impl Read) -> Result<PakArchive> {
    from_reader_with_options(reader, DecodeOptions::default())
}

/// Read and decode one complete PAK with caller-provided resource limits.
pub fn from_reader_with_limits(reader: impl Read, limits: DecodeLimits) -> Result<PakArchive> {
    from_reader_with_options(
        reader,
        DecodeOptions {
            limits,
            ..DecodeOptions::default()
        },
    )
}

/// Buffer a non-seekable input under the configured archive limit and decode it.
pub fn from_reader_with_options(
    mut reader: impl Read,
    options: DecodeOptions,
) -> Result<PakArchive> {
    let read_limit =
        options
            .limits
            .max_archive_bytes
            .checked_add(1)
            .ok_or(PakError::IntegerOverflow {
                context: "archive read limit",
            })?;
    let mut bytes = Vec::new();
    reader.by_ref().take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > options.limits.max_archive_bytes {
        return Err(PakError::LimitExceeded {
            resource: "archive bytes",
            requested: bytes.len() as u64,
            limit: options.limits.max_archive_bytes,
        });
    }
    from_bytes_with_options(&bytes, options)
}

/// Decode a seekable source into a complete owned archive without first
/// buffering the encoded container. Use [`PakReader`] to keep payloads lazy.
pub fn from_seekable_reader(reader: impl Read + Seek) -> Result<PakArchive> {
    from_seekable_reader_with_options(reader, DecodeOptions::default())
}

/// Decode a seekable source into a complete owned archive with full options.
pub fn from_seekable_reader_with_options(
    reader: impl Read + Seek,
    options: DecodeOptions,
) -> Result<PakArchive> {
    PakReader::with_options(reader, options)?.into_archive()
}
