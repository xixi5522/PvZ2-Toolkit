//! Errors produced by compression engines.

use super::Codec;
use std::fmt;

#[derive(Debug)]
#[non_exhaustive]
pub enum CodecError {
    InvalidData {
        codec: Codec,
        message: String,
    },
    LengthMismatch {
        codec: Codec,
        expected: usize,
        actual: usize,
    },
    SizeLimit {
        codec: Codec,
        message: String,
    },
    MissingContext {
        codec: Codec,
    },
    Unavailable {
        codec: Codec,
    },
}

impl CodecError {
    #[allow(dead_code)]
    pub(crate) fn invalid(codec: Codec, message: impl Into<String>) -> Self {
        Self::InvalidData {
            codec,
            message: message.into(),
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidData { codec, message } => {
                write!(formatter, "Invalid {codec} stream: {message}")
            }
            Self::LengthMismatch {
                codec,
                expected,
                actual,
            } => write!(
                formatter,
                "{codec} decompressed length mismatch: expected {expected}, got {actual}"
            ),
            Self::SizeLimit { codec, message } => {
                write!(formatter, "{codec} stream size limit: {message}")
            }
            Self::MissingContext { codec } => {
                write!(formatter, "{codec} decoding requires archive context")
            }
            Self::Unavailable { codec } => {
                write!(formatter, "{codec} support is not enabled in this build")
            }
        }
    }
}

impl std::error::Error for CodecError {}
