use std::io;
use thiserror::Error;

/// Errors that can occur during PAM encoding/decoding.
#[derive(Error, Debug)]
pub enum Error {
    /// Invalid magic number (not a PAM file).
    #[error("Invalid PAM magic: {0:#X}")]
    InvalidMagic(u32),

    /// PAM version is outside the supported range (1..=6).
    #[error("PAM version out of range: {0}")]
    VersionOutOfRange(i32),

    /// A value cannot be represented by the PAM binary field being written.
    #[error("PAM field {field} out of range: {value}")]
    ValueOutOfRange { field: &'static str, value: i64 },

    /// A required field is missing for the selected PAM version.
    #[error("PAM field {field} is required")]
    MissingRequiredField { field: &'static str },

    /// A transform does not match the selected PAM version's definition.
    #[error("PAM field {field} has invalid transform length {actual}, expected {expected}")]
    InvalidTransform {
        field: &'static str,
        expected: &'static str,
        actual: usize,
    },

    /// I/O error during read or write.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
