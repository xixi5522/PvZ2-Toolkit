//! A pure-Rust library for reading, extracting, creating, and inspecting
//! Dzip archives.
//!
//! Use `Archive` for indexed reading and safe extraction,
//! `ArchiveBuilder` for deterministic archive creation, and `ArchiveImage`
//! for byte-for-byte volume retention. Low-level binary structures remain
//! available through [`mod@format`], [`reader`], and [`writer`] for
//! format-analysis tools.
//!
//! # Reading an archive
//!
//! ```no_run
//! use dzip::Archive;
//!
//! let mut archive = Archive::open_path("game.dz")?;
//! for entry in archive.entries() {
//!     println!("{} ({} bytes)", entry.path().display(), entry.decompressed_size());
//! }
//! let data = archive.read_entry_by_path("Data/config.bin")?;
//! # let _ = data;
//! # Ok::<(), dzip::DzipError>(())
//! ```

//! # Creating an archive
//!
//! ```no_run
//! use dzip::{ArchiveBuilder, Compression, EntryOptions};
//!
//! let mut builder = ArchiveBuilder::new();
//! builder.add_path(
//!     "Data/config.bin",
//!     "input/config.bin",
//!     EntryOptions::new().compression(Compression::Dz),
//! )?;
//! builder.write_to_path("game.dz")?;
//! # Ok::<(), dzip::DzipError>(())
//! ```

#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "decode")]
pub mod archive;
#[cfg(feature = "encode")]
pub mod builder;
pub mod codec;
mod codecs;
mod compat;
pub mod error;
#[cfg(feature = "decode")]
pub mod extract;
pub mod format;
#[cfg(feature = "decode")]
pub mod image;
pub mod options;
pub mod path;
pub mod raw;
pub mod reader;
pub mod volume;
pub mod writer;

#[cfg(feature = "decode")]
pub use archive::{
    Archive, ArchiveIndex, ArchivePreparation, Entry, EntryId, EntrySegment, ReadLimits,
    ReadOptions,
};
#[cfg(feature = "encode")]
pub use builder::{
    ArchiveBuilder, BuildReport, EntryOptions, FileSystemVolumeSink, MemoryVolumeSink, PackOptions,
    VolumeSink, WriteSeek,
};
pub use codec::{
    ChunkEncoding, Codec, CodecError, CodecLimits, Compression, ContentHint, DzOptions,
    ParseCompressionError,
};
pub use error::{DzipError, Result};
#[cfg(feature = "decode")]
pub use extract::{ExtractOptions, ExtractionReport};
pub use format::{
    ArchivePath, ArchiveSettings, ArchiveString, Chunk, ChunkSettings, RangeSettings, RawArchive,
    RawFileRecord, ResolvedChunk,
};
#[cfg(feature = "decode")]
pub use image::ArchiveImage;
pub use path::ArchivePathKey;
#[cfg(feature = "decode")]
pub use reader::{ReadSeek, VolumeSource};
#[cfg(feature = "decode")]
pub use volume::{FileSystemVolumeManager, MemoryVolumeSource, PathVolumeSource};
#[cfg(feature = "encode")]
pub use writer::compress_data;
