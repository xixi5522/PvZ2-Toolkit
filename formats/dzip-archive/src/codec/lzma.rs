//! Dzip's LZMA1 codec, stored in the standard 13-byte LZMA-alone framing.
//!
//! The properties retain dzip.exe's 64-KiB dictionary and level-5-style match
//! limits. The independent encoder is format-compatible rather than
//! byte-for-byte compatible with any particular LZMA SDK release.

use super::{Codec, CodecError, CodecLimits};
use crate::Result;
#[cfg(feature = "lzma")]
use crate::codecs::lzma;
use lzma::{LzmaProps, decoder_props, encode_with_end_marker};

pub(crate) fn encode(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let properties = LzmaProps {
        lc: 3,
        lp: 0,
        pb: 2,
        dict_size: 0x10000,
        fb: 32,
        mc: 32,
    };
    let raw =
        encode_with_end_marker(input, &properties).map_err(|error| codec_error(error.as_str()))?;

    let mut output = Vec::with_capacity(13 + raw.len());
    output.extend_from_slice(
        &decoder_props(&properties).map_err(|error| codec_error(error.as_str()))?,
    );
    output.extend_from_slice(&(input.len() as u64).to_le_bytes());
    output.extend_from_slice(&raw);
    Ok(output)
}

#[cfg(test)]
pub(crate) fn decode(input: &[u8], expected_length: usize, limits: CodecLimits) -> Result<Vec<u8>> {
    decode_with_buffer(input, expected_length, limits, Vec::new())
}

pub(crate) fn decode_with_buffer(
    input: &[u8],
    expected_length: usize,
    limits: CodecLimits,
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    if input.is_empty() && expected_length == 0 {
        let mut output = output;
        output.clear();
        return Ok(output);
    }
    let header = input
        .get(..13)
        .ok_or_else(|| codec_error("truncated LZMA-alone header"))?;
    let properties: [u8; 5] = header[..5]
        .try_into()
        .expect("the five-byte properties slice has a fixed length");
    let declared_length = u64::from_le_bytes(
        header[5..13]
            .try_into()
            .expect("the eight-byte length slice has a fixed length"),
    );
    let expected_u64 = u64::try_from(expected_length)
        .map_err(|_| codec_error("expected output length does not fit u64"))?;
    if declared_length != u64::MAX && declared_length != expected_u64 {
        return Err(codec_error(&format!(
            "decompressed length mismatch: chunk expects {expected_length}, header declares {declared_length}"
        )));
    }

    let options = lzma::DecoderOptions::from_decoder_properties(properties, expected_length)
        .map_err(codec_engine_error)?;
    let mut decoder = lzma::Decoder::with_output(
        lzma::DecoderOptions {
            limits: lzma::ResourceLimits {
                max_input_size: limits.max_input_size,
                max_output_size: limits.max_output_size,
                max_workspace_size: limits.max_workspace_size,
            },
            ..options
        },
        output,
    )
    .map_err(codec_engine_error)?;
    decoder.decode(&input[13..]).map_err(codec_engine_error)?;
    Ok(decoder.take_output())
}

fn codec_engine_error(error: lzma::Error) -> crate::DzipError {
    match error.kind() {
        lzma::ErrorKind::InputLimitExceeded
        | lzma::ErrorKind::OutputLimitExceeded
        | lzma::ErrorKind::WorkspaceLimitExceeded => CodecError::SizeLimit {
            codec: Codec::Lzma,
            message: error.to_string(),
        }
        .into(),
        _ => codec_error(error.as_str()),
    }
}

fn codec_error(message: &str) -> crate::DzipError {
    CodecError::invalid(Codec::Lzma, message).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzma_alone_header_uses_dzip_properties_and_size() {
        let encoded = encode(b"payload").unwrap();
        assert_eq!(&encoded[..5], &[0x5d, 0x00, 0x00, 0x01, 0x00]);
        assert_eq!(&encoded[5..13], &7u64.to_le_bytes());
    }

    #[test]
    fn empty_input_matches_dzip_zero_length_storage() {
        assert!(encode(&[]).unwrap().is_empty());
        assert_eq!(
            decode(&[], 0, CodecLimits::UNLIMITED).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn lzma_alone_round_trip_uses_local_decoder() {
        for input in [
            Vec::new(),
            b"local LZMA decoder".repeat(100),
            (0..=255).cycle().take(100_000).collect(),
        ] {
            let encoded = encode(&input).unwrap();
            assert_eq!(
                decode(&encoded, input.len(), CodecLimits::UNLIMITED).unwrap(),
                input
            );
        }
    }

    #[test]
    fn malformed_lzma_alone_streams_return_errors() {
        assert!(decode(&[0], 0, CodecLimits::UNLIMITED).is_err());

        let mut mismatched = encode(b"payload").unwrap();
        mismatched[5..13].copy_from_slice(&8u64.to_le_bytes());
        assert!(decode(&mismatched, 7, CodecLimits::UNLIMITED).is_err());

        let mut truncated = encode(b"payload").unwrap();
        truncated.truncate(15);
        assert!(decode(&truncated, 7, CodecLimits::UNLIMITED).is_err());
    }
}
