use super::{Codec, CodecError, CodecLimits};
use crate::Result;
#[cfg(feature = "zlib")]
use crate::codecs::zlib;
const GZIP_HEADER: [u8; 10] = [
    0x1f, 0x8b, 8, 0, // magic, deflate, no optional fields
    0, 0, 0, 0, // timestamp
    0, 11, // default compression, Windows/NTFS
];

/// Writes dzip's gzip-like framing: a ten-byte gzip header followed by raw
/// DEFLATE, deliberately without the normal CRC32/ISIZE trailer.
pub(crate) fn encode(input: &[u8]) -> Result<Vec<u8>> {
    // dzip's archive layer does not invoke the external coder for empty
    // chunks; it stores a zero-length physical stream with the codec flag.
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let raw = zlib::encode_raw(input);

    let mut output = Vec::with_capacity(GZIP_HEADER.len() + raw.len());
    output.extend_from_slice(&GZIP_HEADER);
    output.extend_from_slice(&raw);
    Ok(output)
}

/// Accepts both dzip's truncated-gzip framing and ordinary RFC 1950 zlib
/// streams, matching the two paths in the original decoder.
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
    if input.starts_with(&[0x1f, 0x8b]) {
        let payload = &input[gzip_payload_offset(input)?..];
        decode_engine(
            payload,
            expected_length,
            zlib::StreamFormat::RawDeflate,
            limits,
            output,
        )
    } else {
        decode_engine(
            input,
            expected_length,
            zlib::StreamFormat::Zlib,
            limits,
            output,
        )
    }
}

fn decode_engine(
    input: &[u8],
    expected_length: usize,
    format: zlib::StreamFormat,
    limits: CodecLimits,
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    let mut decoder = zlib::Decoder::with_output(
        zlib::DecoderOptions {
            format,
            expected_size: expected_length,
            limits: zlib::ResourceLimits {
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

fn codec_engine_error(error: zlib::Error) -> crate::DzipError {
    match error.kind() {
        zlib::ErrorKind::InputLimitExceeded
        | zlib::ErrorKind::OutputLimitExceeded
        | zlib::ErrorKind::WorkspaceLimitExceeded => CodecError::SizeLimit {
            codec: Codec::Zlib,
            message: error.to_string(),
        }
        .into(),
        _ => codec_error(error.as_str()),
    }
}

fn gzip_payload_offset(input: &[u8]) -> Result<usize> {
    if input.len() < GZIP_HEADER.len() {
        return Err(codec_error("truncated gzip header"));
    }
    if input[2] != 8 {
        return Err(codec_error("gzip stream does not use DEFLATE"));
    }

    let flags = input[3];
    if flags & 0xe0 != 0 {
        return Err(codec_error("gzip header uses reserved flags"));
    }

    let mut offset = GZIP_HEADER.len();
    if flags & 0x04 != 0 {
        let length_bytes = input
            .get(offset..offset + 2)
            .ok_or_else(|| codec_error("truncated gzip extra-field length"))?;
        let length = usize::from(u16::from_le_bytes([length_bytes[0], length_bytes[1]]));
        offset = offset
            .checked_add(2)
            .and_then(|value| value.checked_add(length))
            .filter(|&value| value <= input.len())
            .ok_or_else(|| codec_error("truncated gzip extra field"))?;
    }
    if flags & 0x08 != 0 {
        offset = skip_c_string(input, offset, "file name")?;
    }
    if flags & 0x10 != 0 {
        offset = skip_c_string(input, offset, "comment")?;
    }
    if flags & 0x02 != 0 {
        offset = offset
            .checked_add(2)
            .filter(|&value| value <= input.len())
            .ok_or_else(|| codec_error("truncated gzip header checksum"))?;
    }
    Ok(offset)
}

fn skip_c_string(input: &[u8], offset: usize, field: &str) -> Result<usize> {
    let relative_end = input
        .get(offset..)
        .and_then(|bytes| bytes.iter().position(|&byte| byte == 0))
        .ok_or_else(|| codec_error(&format!("unterminated gzip {field}")))?;
    Ok(offset + relative_end + 1)
}

fn codec_error(message: &str) -> crate::DzipError {
    CodecError::invalid(Codec::Zlib, message).into()
}
