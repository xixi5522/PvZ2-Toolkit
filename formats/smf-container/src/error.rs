use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SmfError {
    #[error("SMF input is too short: expected at least {minimum} bytes, got {actual}")]
    InputTooShort { minimum: u64, actual: u64 },
    #[error("invalid SMF magic: expected 0x{expected:08X}, got 0x{actual:08X}")]
    InvalidMagic { expected: u32, actual: u32 },
    #[error("invalid zlib stream header at SMF offset {offset}")]
    InvalidZlibHeader { offset: u64 },
    #[error("SMF payload size mismatch: header declares {declared} bytes, decoded {actual} bytes")]
    SizeMismatch { declared: u64, actual: u64 },
    #[error("payload is too large for the compact SMF header: {size} bytes")]
    CompactSizeOverflow { size: u64 },
    #[error("invalid zlib compression level {level}; expected 0 through 9")]
    InvalidCompressionLevel { level: u32 },
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, SmfError>;
