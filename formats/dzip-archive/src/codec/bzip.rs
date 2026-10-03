use super::{Codec, CodecError, CodecLimits};
use crate::Result;
#[cfg(feature = "bzip")]
use crate::codecs::bzip;

/// Writes a standard BZip2 stream using 100-KiB blocks, as expected by dzip.
pub(crate) fn encode(input: &[u8]) -> Result<Vec<u8>> {
    bzip::encode_default(input).map_err(codec_engine_error)
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
    let mut decoder = bzip::Decoder::with_output(
        bzip::DecoderOptions {
            expected_size: expected_length,
            limits: bzip::ResourceLimits {
                max_input_size: limits.max_input_size,
                max_output_size: limits.max_output_size,
                max_workspace_size: limits.max_workspace_size,
            },
        },
        output,
    )
    .map_err(codec_engine_error)?;
    decoder.decode(input).map_err(codec_engine_error)?;
    Ok(decoder.take_output())
}

fn codec_error(message: &str) -> crate::DzipError {
    CodecError::invalid(Codec::Bzip, message).into()
}

fn codec_engine_error(error: bzip::Error) -> crate::DzipError {
    match error.kind() {
        bzip::ErrorKind::InputLimitExceeded
        | bzip::ErrorKind::OutputLimitExceeded
        | bzip::ErrorKind::WorkspaceLimitExceeded => CodecError::SizeLimit {
            codec: Codec::Bzip,
            message: error.to_string(),
        }
        .into(),
        _ => codec_error(error.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_keeps_the_original_bzip2_stream() {
        assert_eq!(
            encode(&[]).unwrap(),
            [
                0x42, 0x5a, 0x68, 0x31, 0x17, 0x72, 0x45, 0x38, 0x50, 0x90, 0x00, 0x00, 0x00, 0x00,
            ]
        );
    }
}
