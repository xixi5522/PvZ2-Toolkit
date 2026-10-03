use std::fmt;
use std::io;

#[cfg(feature = "dz")]
use crate::codecs::dz;

#[non_exhaustive]
#[derive(Debug)]
pub enum DzipError {
    Io(io::Error),

    InvalidHeader,

    UnsupportedVersion(u8),

    Utf8(std::string::FromUtf8Error),

    UnsupportedCompression(u16),

    InvalidDz(String),

    VolumeNotFound(u16),

    VolumeOpenError(u16, String),

    Codec(crate::codec::CodecError),

    InvalidArchive(String),

    LimitExceeded {
        resource: &'static str,
        limit: u64,
        actual: u64,
    },

    EntryNotFound(String),
}

impl fmt::Display for DzipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IO error: {error}"),
            Self::InvalidHeader => formatter.write_str("Invalid DTRZ header"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "Unsupported version: {version}")
            }
            Self::Utf8(error) => write!(formatter, "UTF-8 error: {error}"),
            Self::UnsupportedCompression(flags) => {
                write!(
                    formatter,
                    "Unsupported compression method: flags={flags:#x}"
                )
            }
            Self::InvalidDz(message) => write!(formatter, "Invalid DZ stream: {message}"),
            Self::VolumeNotFound(volume) => {
                write!(formatter, "Volume {volume} not found in file list")
            }
            Self::VolumeOpenError(volume, message) => {
                write!(formatter, "Failed to open volume {volume}: {message}")
            }
            Self::Codec(error) => write!(formatter, "{error}"),
            Self::InvalidArchive(message) => write!(formatter, "Invalid Dzip archive: {message}"),
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "Dzip {resource} limit exceeded: limit {limit}, actual {actual}"
            ),
            Self::EntryNotFound(path) => write!(formatter, "Archive entry not found: {path}"),
        }
    }
}

impl std::error::Error for DzipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Utf8(error) => Some(error),
            Self::Codec(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for DzipError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<std::string::FromUtf8Error> for DzipError {
    fn from(error: std::string::FromUtf8Error) -> Self {
        Self::Utf8(error)
    }
}

impl From<crate::codec::CodecError> for DzipError {
    fn from(error: crate::codec::CodecError) -> Self {
        Self::Codec(error)
    }
}

#[cfg(feature = "dz")]
impl From<dz::DzError> for DzipError {
    fn from(error: dz::DzError) -> Self {
        match error.kind() {
            dz::ErrorKind::InputLimitExceeded
            | dz::ErrorKind::OutputLimitExceeded
            | dz::ErrorKind::WorkspaceLimitExceeded => {
                Self::Codec(crate::codec::CodecError::SizeLimit {
                    codec: crate::codec::Codec::Dz,
                    message: error.to_string(),
                })
            }
            _ => Self::Codec(crate::codec::CodecError::invalid(
                crate::codec::Codec::Dz,
                error.to_string(),
            )),
        }
    }
}

pub type Result<T> = std::result::Result<T, DzipError>;
