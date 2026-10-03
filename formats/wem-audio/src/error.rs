//! Structured errors for WEM inspection and conversion.

use thiserror::Error;

pub type WemResult<T> = Result<T, WemError>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WemError {
    #[error("invalid RIFF/WEM container: {reason}")]
    InvalidRiff { reason: String },

    #[error("required RIFF chunk `{chunk}` is missing")]
    MissingChunk { chunk: &'static str },

    #[error("invalid `{chunk}` chunk: {reason}")]
    InvalidChunk { chunk: &'static str, reason: String },

    #[error("unsupported WEM codec tag 0x{format_tag:04X}")]
    UnsupportedCodec { format_tag: u16 },

    #[error("unsupported WEM variant: {feature}")]
    UnsupportedVariant { feature: &'static str },

    #[error("invalid WEM field `{field}` ({value}): {reason}")]
    InvalidField {
        field: &'static str,
        value: u64,
        reason: &'static str,
    },

    #[error("{what} exceeds the supported size range")]
    SizeOverflow { what: &'static str },

    #[error("Vorbis WEM decoding requires a codebook library")]
    MissingCodebooks,

    #[error("malformed WEM data: {message}")]
    Malformed { message: String },

    #[error("codebook error: {message}")]
    Codebook { message: String },

    #[error(
        "codebook data size mismatch: expected {expected} bytes, consumed {actual}; the selected codebook set may be wrong"
    )]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("codebook id {id} does not exist in the selected codebook set")]
    InvalidCodebookId { id: i32 },

    #[error("unexpected end of bitstream while {context}")]
    EndOfStream { context: String },

    #[error("audio backend error: {message}")]
    Audio { message: String },

    #[error("Ogg container error: {message}")]
    Ogg { message: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "wav")]
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
}

#[allow(dead_code)]
impl WemError {
    pub(crate) fn invalid_riff(reason: impl Into<String>) -> Self {
        Self::InvalidRiff {
            reason: reason.into(),
        }
    }

    pub(crate) const fn missing_chunk(chunk: &'static str) -> Self {
        Self::MissingChunk { chunk }
    }

    pub(crate) fn invalid_chunk(chunk: &'static str, reason: impl Into<String>) -> Self {
        Self::InvalidChunk {
            chunk,
            reason: reason.into(),
        }
    }

    pub(crate) const fn unsupported_variant(feature: &'static str) -> Self {
        Self::UnsupportedVariant { feature }
    }

    pub(crate) const fn invalid_field(
        field: &'static str,
        value: u64,
        reason: &'static str,
    ) -> Self {
        Self::InvalidField {
            field,
            value,
            reason,
        }
    }

    pub(crate) const fn size_overflow(what: &'static str) -> Self {
        Self::SizeOverflow { what }
    }

    pub(crate) fn parse(message: impl Into<String>) -> Self {
        Self::Malformed {
            message: message.into(),
        }
    }

    pub(crate) fn codebook(message: impl Into<String>) -> Self {
        Self::Codebook {
            message: message.into(),
        }
    }

    pub(crate) const fn size_mismatch(expected: u64, actual: u64) -> Self {
        Self::SizeMismatch { expected, actual }
    }

    pub(crate) const fn invalid_codebook_id(id: i32) -> Self {
        Self::InvalidCodebookId { id }
    }

    pub(crate) fn end_of_stream(context: impl Into<String>) -> Self {
        Self::EndOfStream {
            context: context.into(),
        }
    }

    pub(crate) fn audio(message: impl Into<String>) -> Self {
        Self::Audio {
            message: message.into(),
        }
    }

    pub(crate) fn ogg(message: impl Into<String>) -> Self {
        Self::Ogg {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_errors_are_matchable() {
        let error = WemError::missing_chunk("data");
        assert!(matches!(error, WemError::MissingChunk { chunk: "data" }));

        let error = WemError::invalid_field("channels", 0, "must be non-zero");
        assert!(matches!(
            error,
            WemError::InvalidField {
                field: "channels",
                value: 0,
                ..
            }
        ));
    }

    #[test]
    fn io_errors_preserve_the_source() {
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let error: WemError = source.into();
        assert!(matches!(error, WemError::Io(_)));
    }
}
