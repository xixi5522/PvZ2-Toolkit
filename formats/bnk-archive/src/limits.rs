#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Allocation and nesting limits used while decoding an untrusted bank.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(default))]
pub struct DecodeLimits {
    pub max_file_bytes: u64,
    pub max_chunk_bytes: u64,
    pub max_chunks: usize,
    pub max_entries: usize,
    pub max_string_bytes: usize,
    pub max_hierarchy_object_bytes: usize,
    /// Maximum number of scalar fields materialized for one HIRC object.
    ///
    /// HIRC fields carry editable paths and semantic metadata, so their
    /// in-memory representation can be substantially larger than the wire
    /// payload. This separate budget prevents a compact malicious object from
    /// expanding into an excessive number of allocations.
    pub max_hierarchy_fields: usize,
    /// Maximum combined UTF-8 path bytes materialized for one HIRC object.
    pub max_hierarchy_path_bytes: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            max_chunk_bytes: 1024 * 1024 * 1024,
            max_chunks: 65_536,
            max_entries: 2_000_000,
            max_string_bytes: 16 * 1024 * 1024,
            max_hierarchy_object_bytes: 256 * 1024 * 1024,
            max_hierarchy_fields: 2_000_000,
            max_hierarchy_path_bytes: 256 * 1024 * 1024,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ValidationMode {
    /// Reject non-canonical constants, unknown versions, and trailing bytes in
    /// known chunks.
    Strict,
    /// Preserve non-canonical and unknown data wherever the format permits it.
    Permissive,
    /// Apply strict field validation and additionally require Twinning's
    /// canonical top-level chunk state machine.
    TwinningCompatible,
}

impl ValidationMode {
    pub const fn is_strict(self) -> bool {
        !matches!(self, Self::Permissive)
    }

    pub const fn is_permissive(self) -> bool {
        matches!(self, Self::Permissive)
    }

    pub const fn is_twinning_compatible(self) -> bool {
        matches!(self, Self::TwinningCompatible)
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(default))]
pub struct DecodeOptions {
    pub limits: DecodeLimits,
    pub validation: ValidationMode,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            limits: DecodeLimits::default(),
            validation: ValidationMode::Strict,
        }
    }
}

impl DecodeOptions {
    pub fn permissive() -> Self {
        Self {
            validation: ValidationMode::Permissive,
            ..Self::default()
        }
    }

    pub fn twinning_compatible() -> Self {
        Self {
            validation: ValidationMode::TwinningCompatible,
            ..Self::default()
        }
    }
}
