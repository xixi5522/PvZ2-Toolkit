//! Unified Dzip chunk codec dispatch.
//!
//! Codec engines live in independent crates. This façade owns archive framing,
//! chunk-flag interpretation, output-size validation, feature gating, and
//! conversion into the public error model.

#[cfg(feature = "bzip")]
mod bzip;
pub mod dz;
mod error;
mod flags;
#[cfg(feature = "lzma")]
mod lzma;
#[cfg(feature = "zlib")]
mod zlib;

pub use dz::{DzDecodeContext, DzOptions};
pub use error::CodecError;
pub use flags::{ChunkEncoding, Codec, Compression, ContentHint, ParseCompressionError};

use crate::{RangeSettings, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecLimits {
    pub max_input_size: usize,
    pub max_output_size: usize,
    pub max_workspace_size: usize,
}

impl CodecLimits {
    pub const UNLIMITED: Self = Self {
        max_input_size: usize::MAX,
        max_output_size: usize::MAX,
        max_workspace_size: usize::MAX,
    };
}

impl Default for CodecLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DecodeContext<'a> {
    pub expected_len: usize,
    pub dz: Option<&'a DzDecodeContext>,
    pub limits: CodecLimits,
}

impl DecodeContext<'_> {
    pub const fn new(expected_len: usize) -> Self {
        Self {
            expected_len,
            dz: None,
            limits: CodecLimits::UNLIMITED,
        }
    }
}

pub fn encode(
    compression: Compression,
    input: &[u8],
    _dz_settings: RangeSettings,
) -> Result<Vec<u8>> {
    match compression {
        Compression::Copy => Ok(input.to_vec()),
        Compression::Zero => Ok(Vec::new()),
        Compression::Dz => {
            #[cfg(feature = "dz")]
            {
                dz::encode(input, _dz_settings)
            }
            #[cfg(not(feature = "dz"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Dz }.into())
            }
        }
        Compression::Bzip => {
            #[cfg(feature = "bzip")]
            {
                bzip::encode(input)
            }
            #[cfg(not(feature = "bzip"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Bzip }.into())
            }
        }
        Compression::Zlib => {
            #[cfg(feature = "zlib")]
            {
                zlib::encode(input)
            }
            #[cfg(not(feature = "zlib"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Zlib }.into())
            }
        }
        Compression::Lzma => {
            #[cfg(feature = "lzma")]
            {
                lzma::encode(input)
            }
            #[cfg(not(feature = "lzma"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Lzma }.into())
            }
        }
    }
}

pub fn decode(
    encoding: ChunkEncoding,
    input: &[u8],
    context: DecodeContext<'_>,
) -> Result<Vec<u8>> {
    decode_with_buffer(encoding, input, context, Vec::new())
}

pub(crate) fn decode_with_buffer(
    encoding: ChunkEncoding,
    input: &[u8],
    context: DecodeContext<'_>,
    mut output: Vec<u8>,
) -> Result<Vec<u8>> {
    if input.len() > context.limits.max_input_size {
        return Err(crate::DzipError::LimitExceeded {
            resource: "codec input size",
            limit: context.limits.max_input_size as u64,
            actual: input.len() as u64,
        });
    }
    if context.expected_len > context.limits.max_output_size {
        return Err(crate::DzipError::LimitExceeded {
            resource: "codec output size",
            limit: context.limits.max_output_size as u64,
            actual: context.expected_len as u64,
        });
    }
    match encoding.compression {
        Compression::Copy => {
            output.clear();
            output.extend_from_slice(input);
            Ok(output)
        }
        Compression::Zero => {
            output.clear();
            output.resize(context.expected_len, 0);
            Ok(output)
        }
        Compression::Dz => {
            #[cfg(feature = "dz")]
            {
                let dz_context = context
                    .dz
                    .ok_or(CodecError::MissingContext { codec: Codec::Dz })?;
                dz::decode_with_buffer(
                    input,
                    context.expected_len,
                    dz_context,
                    context.limits,
                    output,
                )
            }
            #[cfg(not(feature = "dz"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Dz }.into())
            }
        }
        Compression::Bzip => {
            #[cfg(feature = "bzip")]
            {
                bzip::decode_with_buffer(input, context.expected_len, context.limits, output)
            }
            #[cfg(not(feature = "bzip"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Bzip }.into())
            }
        }
        Compression::Zlib => {
            #[cfg(feature = "zlib")]
            {
                zlib::decode_with_buffer(input, context.expected_len, context.limits, output)
            }
            #[cfg(not(feature = "zlib"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Zlib }.into())
            }
        }
        Compression::Lzma => {
            #[cfg(feature = "lzma")]
            {
                lzma::decode_with_buffer(input, context.expected_len, context.limits, output)
            }
            #[cfg(not(feature = "lzma"))]
            {
                Err(CodecError::Unavailable { codec: Codec::Lzma }.into())
            }
        }
    }
}

#[cfg(all(test, feature = "encode"))]
mod feature_tests {
    use super::*;

    #[test]
    fn disabled_codec_features_report_unavailable() {
        let settings = RangeSettings::default();

        #[cfg(not(feature = "bzip"))]
        assert!(matches!(
            encode(Compression::Bzip, b"data", settings),
            Err(crate::DzipError::Codec(CodecError::Unavailable {
                codec: Codec::Bzip
            }))
        ));
        #[cfg(not(feature = "dz"))]
        assert!(matches!(
            encode(Compression::Dz, b"data", settings),
            Err(crate::DzipError::Codec(CodecError::Unavailable {
                codec: Codec::Dz
            }))
        ));
        #[cfg(not(feature = "lzma"))]
        assert!(matches!(
            encode(Compression::Lzma, b"data", settings),
            Err(crate::DzipError::Codec(CodecError::Unavailable {
                codec: Codec::Lzma
            }))
        ));
        #[cfg(not(feature = "zlib"))]
        assert!(matches!(
            encode(Compression::Zlib, b"data", settings),
            Err(crate::DzipError::Codec(CodecError::Unavailable {
                codec: Codec::Zlib
            }))
        ));
        let _ = settings;
    }

    #[test]
    fn unified_decode_limits_apply_before_dispatch() {
        let encoding = ChunkEncoding {
            compression: Compression::Copy,
            random_access: false,
            common_buffer: false,
            content_hint: None,
            unknown_flags: 0,
        };
        let error = decode(
            encoding,
            b"too large",
            DecodeContext {
                expected_len: 9,
                dz: None,
                limits: CodecLimits {
                    max_input_size: 1,
                    ..CodecLimits::UNLIMITED
                },
            },
        )
        .unwrap_err();
        assert!(matches!(error, crate::DzipError::LimitExceeded { .. }));
    }
}
