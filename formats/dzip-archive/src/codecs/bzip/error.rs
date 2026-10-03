use alloc::string::String;
use core::fmt;

/// Stable, machine-readable category shared by the codec APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidOptions,
    InvalidData,
    TruncatedData,
    InputLimitExceeded,
    OutputLimitExceeded,
    WorkspaceLimitExceeded,
    UnsupportedFeature,
}

/// A BZip2 configuration or stream error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let kind = if message.contains("truncated") || message.contains("ended before") {
            ErrorKind::TruncatedData
        } else if message.contains("unsupported") {
            ErrorKind::UnsupportedFeature
        } else {
            ErrorKind::InvalidData
        };
        Self { kind, message }
    }

    pub(crate) fn invalid_options(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InvalidOptions,
            message: message.into(),
        }
    }

    pub(crate) fn output_limit(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::OutputLimitExceeded,
            message: message.into(),
        }
    }

    pub(crate) fn input_limit(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InputLimitExceeded,
            message: message.into(),
        }
    }

    pub(crate) fn workspace_limit(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::WorkspaceLimitExceeded,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn as_str(&self) -> &str {
        self.message()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl core::error::Error for Error {}
