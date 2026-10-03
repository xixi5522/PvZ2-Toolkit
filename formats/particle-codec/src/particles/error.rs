use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParticlesError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid Particles platform variant")]
    InvalidVariant,

    #[error("invalid {kind} length: {value}")]
    InvalidLength { kind: &'static str, value: i64 },

    #[error("{kind} length {value} exceeds the supported limit of {limit}")]
    LimitExceeded {
        kind: &'static str,
        value: usize,
        limit: usize,
    },

    #[error("{kind} length {value} cannot be represented by this format")]
    LengthOverflow { kind: &'static str, value: usize },

    #[error("decompressed size mismatch: header declares {expected} bytes, decoded {actual} bytes")]
    SizeMismatch { expected: usize, actual: usize },

    #[error("Unsupported Particles format: expected {0}, got {1}")]
    UnsupportedFormat(u32, u32),

    #[error("String decode error")]
    StringDecodeError,
}
