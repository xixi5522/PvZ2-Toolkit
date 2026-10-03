use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReanimError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid Reanim platform variant")]
    InvalidVariant,

    #[error("Invalid track signature")]
    InvalidTrack,

    #[error("Invalid magic number: expected {0}, got {1}")]
    InvalidMagic(u32, u32),

    #[error("String decode error")]
    StringDecodeError,

    #[error("Invalid {kind} length: {value}")]
    InvalidLength { kind: &'static str, value: i64 },

    #[error("{kind} length {value} exceeds limit {max}")]
    LimitExceeded {
        kind: &'static str,
        value: usize,
        max: usize,
    },

    #[error("{kind} length {value} cannot be represented by the format")]
    LengthOverflow { kind: &'static str, value: usize },

    #[error("Invalid PopCap REANIM container header")]
    InvalidContainer,

    #[error("Uncompressed size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },
}
