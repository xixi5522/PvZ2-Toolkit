use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid RSBPatch magic: expected RSBP (little-endian bytes PBSR), got {actual:02X?}")]
    InvalidMagic { actual: [u8; 4] },
    #[error("unsupported RSBPatch version: expected {expected}, got {actual}")]
    UnsupportedVersion { expected: u32, actual: u32 },
    #[error("invalid constant field {field}: expected {expected}, got {actual}")]
    InvalidConstant {
        field: &'static str,
        expected: u32,
        actual: u32,
    },
    #[error("invalid boolean flag {field}: expected 0 or 1, got {actual}")]
    InvalidFlag { field: &'static str, actual: u32 },
    #[error("{field} flag and payload size are inconsistent")]
    InconsistentPatchState { field: &'static str },
    #[error("packet name is not valid UTF-8")]
    InvalidPacketName(#[from] std::string::FromUtf8Error),
    #[error("packet name contains an embedded NUL byte")]
    PacketNameContainsNul,
    #[error("packet name is too long: {length} bytes, maximum is 128")]
    PacketNameTooLong { length: usize },
    #[error("{field} does not fit the RSBPatch u32 field: {value}")]
    ValueOverflow { field: &'static str, value: usize },
    #[error("unexpected trailing data after RSBPatch packets: {bytes} bytes")]
    TrailingData { bytes: usize },
    #[error("RSBPatch {field} exceeds the configured limit: {actual} > {limit}")]
    ContainerLimitExceeded {
        field: &'static str,
        limit: usize,
        actual: usize,
    },
    #[error("MD5 mismatch for {context}: expected {expected}, got {actual}")]
    HashMismatch {
        context: String,
        expected: String,
        actual: String,
    },
    #[error("invalid VCDIFF data: {0}")]
    InvalidVcdiff(String),
    #[error("unsupported VCDIFF feature: {0}")]
    UnsupportedVcdiff(String),
    #[error("decoded output would exceed the configured limit of {limit} bytes")]
    OutputLimitExceeded { limit: usize },
    #[cfg(feature = "rsb")]
    #[error("RSB archive error: {0}")]
    RsbArchive(#[from] rsb_archive::RsbError),
    #[cfg(feature = "rsb")]
    #[error("{context} RSB version must be 4 for RSBPatch, found {actual}")]
    UnsupportedRsbVersion { context: &'static str, actual: u32 },
    #[cfg(feature = "rsb")]
    #[error("RSBPatch packet count mismatch: patch has {patch}, target metadata has {target}")]
    PacketCountMismatch { patch: usize, target: usize },
    #[cfg(feature = "rsb")]
    #[error("RSBPatch packet #{index} is named {patch}, target metadata expects {target}")]
    PacketNameMismatch {
        index: usize,
        patch: String,
        target: String,
    },
    #[cfg(feature = "rsb")]
    #[error("invalid RSBPatch archive operation: {0}")]
    InvalidArchive(String),
}

pub type Result<T> = std::result::Result<T, PatchError>;
