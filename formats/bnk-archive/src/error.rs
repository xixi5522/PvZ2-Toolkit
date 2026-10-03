use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, BnkError>;

#[derive(Debug, Error)]
pub enum BnkError {
    #[error("I/O error at byte {offset}: {source}")]
    Io {
        offset: u64,
        #[source]
        source: io::Error,
    },

    #[error("invalid BNK magic at byte {offset}: expected BKHD, found {found:?}")]
    InvalidMagic { offset: u64, found: [u8; 4] },

    #[error("unsupported Wwise bank version {version}")]
    UnsupportedVersion { version: u32 },

    #[error(
        "truncated {context} at byte {offset}: need {needed} bytes but only {remaining} remain"
    )]
    Truncated {
        context: &'static str,
        offset: u64,
        needed: usize,
        remaining: usize,
    },

    #[error("invalid {context} at byte {offset}: {message}")]
    InvalidData {
        context: &'static str,
        offset: u64,
        message: String,
    },

    #[error("{resource} limit exceeded: requested {requested}, limit {limit}")]
    LimitExceeded {
        resource: &'static str,
        requested: u64,
        limit: u64,
    },

    #[error("integer overflow while calculating {context}")]
    IntegerOverflow { context: &'static str },

    #[error("missing {chunk:?} chunk required by {context}")]
    MissingChunk {
        chunk: [u8; 4],
        context: &'static str,
    },

    #[error("media {id} points outside DATA: offset {offset}, size {size}, DATA length {data_len}")]
    MediaOutOfBounds {
        id: u32,
        offset: u32,
        size: u32,
        data_len: usize,
    },

    #[error("string in {context} is too long: {length} bytes, maximum {maximum}")]
    StringTooLong {
        context: &'static str,
        length: usize,
        maximum: usize,
    },
}

impl BnkError {
    pub(crate) fn io(offset: usize, source: io::Error) -> Self {
        Self::io_at(offset as u64, source)
    }

    pub(crate) fn io_at(offset: u64, source: io::Error) -> Self {
        Self::Io { offset, source }
    }

    pub(crate) fn invalid(
        context: &'static str,
        offset: usize,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidData {
            context,
            offset: offset as u64,
            message: message.into(),
        }
    }
}
