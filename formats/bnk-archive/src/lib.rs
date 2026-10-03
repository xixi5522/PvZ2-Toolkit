//! Lossless, version-aware reader and writer for Audiokinetic Wwise BNK
//! SoundBanks.
//!
//! The parser keeps the original chunk order, bounds every sub-parser to its
//! declared chunk, retains unknown chunks and HIRC payloads, resolves embedded
//! WEM media, deeply decodes every HIRC layout described by Twinning for the
//! supported Wwise versions, and exposes typed, editable version-specific
//! container settings from Wwise 72 through 150. Large banks can use
//! [`SoundBankReader`] for seek-based lazy access and streaming WEM extraction.

#![forbid(unsafe_code)]

mod binary;
mod container;
pub mod error;
pub mod hierarchy;
pub mod index;
pub mod limits;
mod media;
pub mod semantics;
pub mod stream;
pub mod types;
pub mod version;

use std::io::{Read, Write};

pub use error::{BnkError, Result};
pub use hierarchy::{HierarchyField, HierarchyFieldIndex, HierarchyFieldValue, HierarchyFields};
pub use index::{EmbeddedMediaLocation, HierarchyObjectLocation, SoundBankIndex};
pub use limits::{DecodeLimits, DecodeOptions, ValidationMode};
pub use semantics::{
    CommonPropertyDefault, CommonPropertyDescriptor, CommonPropertyDomain, CommonPropertyValue,
    CommonPropertyValueKind, EnumerationKind, EnumerationVariant, HierarchyFieldSemantic,
    PackedLayout, PackedMember, common_property_descriptor,
};
pub use stream::{ChunkLocation, SoundBankReader};
pub use types::*;
pub use version::{BankVersion, SUPPORTED_VERSIONS};

/// Decode a complete, canonical BNK byte slice.
pub fn from_bytes(bytes: &[u8]) -> Result<SoundBank> {
    binary::from_bytes_with_options(bytes, DecodeOptions::default())
}

/// Decode a complete BNK with caller-provided limits and validation policy.
pub fn from_bytes_with_options(bytes: &[u8], options: DecodeOptions) -> Result<SoundBank> {
    binary::from_bytes_with_options(bytes, options)
}

/// Decode a complete BNK while preserving malformed known chunks as raw
/// chunks whenever their declared container framing is still valid.
pub fn from_bytes_lossless(bytes: &[u8]) -> Result<SoundBank> {
    binary::from_bytes_with_options(bytes, DecodeOptions::permissive())
}

/// Read and decode a BNK with default limits.
pub fn from_reader(reader: impl Read) -> Result<SoundBank> {
    binary::from_reader_with_options(reader, DecodeOptions::default())
}

/// Read and decode a BNK with caller-provided limits and validation policy.
pub fn from_reader_with_options(reader: impl Read, options: DecodeOptions) -> Result<SoundBank> {
    binary::from_reader_with_options(reader, options)
}

/// Encode a bank into a newly allocated byte vector.
pub fn to_bytes(bank: &SoundBank) -> Result<Vec<u8>> {
    binary::to_bytes(bank)
}

/// Encode a bank to a stream.
pub fn to_writer(bank: &SoundBank, writer: impl Write) -> Result<()> {
    binary::to_writer(bank, writer)
}
