//! Error types for RTON parsing, serialization, and crypto helpers.

use serde::{de, ser};
use std::fmt::Display;
use thiserror::Error;

/// Error returned by `serde_rton` operations.
#[derive(Error, Debug)]
pub enum Error {
    // === External Errors (Automatic conversion) ===
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("UTF-8 Error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Formatting Error: {0}")]
    Fmt(#[from] std::fmt::Error),

    #[error("Integer Parse Error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Regex Error: {0}")]
    Regex(#[from] regex::Error),

    // === Logic Errors (Specific variants) ===
    #[error("Invalid RTON Header")]
    InvalidHeader,

    #[error("Invalid RTON Footer")]
    InvalidFooter,

    #[error("Reference index out of bounds")]
    RefIndexOutOfBounds,

    #[error("RTON arrays require a known length in advance")]
    UnknownLength,

    #[error("Unknown Identifier Byte: {0:#04x}")]
    UnknownTag(u8),

    #[error("Unknown RTID sub-identifier: {0:#04x}")]
    UnknownRtidSubId(u8),

    #[error("Expected array end marker 0xfe")]
    ArrayEndMismatch,

    #[error("Expected array capacity marker 0xfd")]
    ArrayCapacityTagMismatch,

    #[error("Invalid UTF-8 start byte: {0:#02x}")]
    InvalidUtf8StartByte(u8),

    #[error("Game Crash: Array overflowed declared capacity")]
    ArrayOverflow,

    #[error("Array exceeded declared capacity")]
    ArrayCapacityExceeded,

    // === Format Specific Errors ===
    #[error("Invalid RTID format: {0}")]
    InvalidRtid(String),

    #[error("Invalid BinaryBlob format: {0}")]
    InvalidBinaryBlob(String),

    #[error("Encountered unsupported extended tag: {0}")]
    UnsupportedExtendedTag(String),

    #[error("String length mismatch: expected {expected} bytes, got {actual} bytes")]
    StringLengthMismatch { expected: u64, actual: u64 },

    #[error("Decryption required but no key provided")]
    MissingKey,

    #[error("Decryption failed: {0}")]
    DecryptionError(String),

    // === Serde Generic Catch-all ===
    // Used when serde calls Error::custom()
    #[error("{0}")]
    Message(String),
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

/// Crate-local result type.
pub type Result<T> = std::result::Result<T, Error>;
