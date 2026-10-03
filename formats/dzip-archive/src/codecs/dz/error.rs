use alloc::string::String;
use core::fmt;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DzError {
    kind: ErrorKind,
    message: String,
}

impl DzError {
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let kind = if message.contains("truncated") || message.contains("ended after") {
            ErrorKind::TruncatedData
        } else if message.contains("unsupported") || message.contains("rejected") {
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

    // Kept as a temporary source-compatibility constructor while the native
    // codec is separated from the archive crate.
    #[allow(non_snake_case)]
    pub(crate) fn InvalidDz(message: String) -> Self {
        Self::new(message)
    }
}

impl fmt::Display for DzError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl core::error::Error for DzError {}

pub type Result<T> = core::result::Result<T, DzError>;
