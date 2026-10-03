//! Wire-format profiles and codec options.

use crate::limits::DecodeLimits;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Physical layout used by a flat PopCap PAK.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FlatProfile {
    /// The complete stream is XOR-obfuscated with `0xF7`.
    #[default]
    PcXor,
    /// Canonical, unobfuscated PopCap/Twinning stream.
    Plain,
    /// Unobfuscated stream with an eight-byte-aligned prefix before each payload.
    Xbox360,
}

/// Payload encoding used by flat PAK entries.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PakCompression {
    #[default]
    None,
    Zlib,
}

/// Compression method used when creating a TV ZIP container.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ZipCompression {
    Stored,
    #[default]
    Deflated,
}

/// Complete, valid PAK wire format.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(tag = "container", rename_all = "snake_case"))]
pub enum PakFormat {
    Flat {
        profile: FlatProfile,
        compression: PakCompression,
    },
    TvZip {
        compression: ZipCompression,
    },
}

impl PakFormat {
    pub const fn pc(compression: PakCompression) -> Self {
        Self::Flat {
            profile: FlatProfile::PcXor,
            compression,
        }
    }

    pub const fn plain(compression: PakCompression) -> Self {
        Self::Flat {
            profile: FlatProfile::Plain,
            compression,
        }
    }

    pub const fn xbox360(compression: PakCompression) -> Self {
        Self::Flat {
            profile: FlatProfile::Xbox360,
            compression,
        }
    }
}

impl Default for PakFormat {
    fn default() -> Self {
        Self::pc(PakCompression::None)
    }
}

/// Path separator policy applied while encoding.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PathSeparator {
    #[default]
    Preserve,
    ForwardSlash,
    Backslash,
}

/// Compression effort shared by zlib and ZIP-deflate writers.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CompressionLevel {
    Fast,
    #[default]
    Default,
    Best,
}

impl CompressionLevel {
    pub(crate) const fn zlib_level(self) -> u32 {
        match self {
            Self::Fast => 1,
            Self::Default => 6,
            Self::Best => 9,
        }
    }
}

/// Encoding policy for a newly written archive.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub format: PakFormat,
    pub path_separator: PathSeparator,
    pub compression_level: CompressionLevel,
    /// Preserve per-entry ZIP compression and timestamps when available.
    pub preserve_zip_metadata: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            format: PakFormat::default(),
            path_separator: PathSeparator::Preserve,
            compression_level: CompressionLevel::Default,
            preserve_zip_metadata: true,
        }
    }
}

/// Directory-field layout observed in a flat archive.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PakDirectoryLayout {
    Uncompressed,
    CanonicalZlib,
    LegacyToolkitZlib,
}

/// Wire properties detected while decoding.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PakSourceInfo {
    pub format: PakFormat,
    pub path_separator: PathSeparator,
    pub directory_layout: Option<PakDirectoryLayout>,
}

/// Optional caller knowledge used to avoid heuristic format detection.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PakFormatHint {
    #[default]
    Auto,
    Exact(PakFormat),
}

/// Whether files emitted by the historical Toolkit writer are accepted.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CompatibilityMode {
    CanonicalOnly,
    #[default]
    LegacyToolkit,
}

/// Structural strictness applied to padding and container metadata.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ValidationMode {
    #[default]
    Strict,
    Permissive,
}

/// Full decoder configuration.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeOptions {
    pub limits: DecodeLimits,
    pub format_hint: PakFormatHint,
    pub compatibility: CompatibilityMode,
    pub validation: ValidationMode,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            limits: DecodeLimits::default(),
            format_hint: PakFormatHint::Auto,
            compatibility: CompatibilityMode::LegacyToolkit,
            validation: ValidationMode::Strict,
        }
    }
}
