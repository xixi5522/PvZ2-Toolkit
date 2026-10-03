//! Allocation and decompression limits for untrusted archives.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Decoder resource limits.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    /// Maximum byte span occupied by the flat directory table.
    pub max_directory_bytes: u64,
    /// Aggregate length of all entry names retained by an index.
    pub max_total_path_bytes: u64,
    pub max_entry_stored_bytes: u64,
    pub max_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
}

impl DecodeLimits {
    /// Conservative limits suitable for a browser/Wasm process.
    pub const fn web() -> Self {
        Self {
            max_archive_bytes: 256 * 1024 * 1024,
            max_entries: 100_000,
            max_directory_bytes: 32 * 1024 * 1024,
            max_total_path_bytes: 16 * 1024 * 1024,
            max_entry_stored_bytes: 128 * 1024 * 1024,
            max_entry_uncompressed_bytes: 256 * 1024 * 1024,
            max_total_uncompressed_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 1_000_000,
            max_directory_bytes: 256 * 1024 * 1024,
            max_total_path_bytes: 128 * 1024 * 1024,
            max_entry_stored_bytes: 1024 * 1024 * 1024,
            max_entry_uncompressed_bytes: 1024 * 1024 * 1024,
            max_total_uncompressed_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}
