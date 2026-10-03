use crate::error::{Result, RsbError};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read, Write};

/// Alignment used for compressed and uncompressed RSG packet data sections.
pub const RSG_DATA_ALIGNMENT: usize = 4096;

/// The zlib compression level used by [`compress_zlib`] and [`compress_rsg_zlib`].
pub const DEFAULT_ZLIB_LEVEL: u32 = 6;

/// Returns whether `data` begins with a structurally valid zlib header.
///
/// This validates the DEFLATE compression method, window size, and FCHECK
/// checksum. It does not decompress or verify the complete stream.
pub fn is_zlib_stream(data: &[u8]) -> bool {
    let Some((&cmf, rest)) = data.split_first() else {
        return false;
    };
    let Some(&flg) = rest.first() else {
        return false;
    };

    cmf & 0x0f == 8 && cmf >> 4 <= 7 && (u16::from(cmf) << 8 | u16::from(flg)).is_multiple_of(31)
}

/// Compresses `data` as a zlib-wrapped DEFLATE stream at level 6.
pub fn compress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    compress_zlib_with_level(data, DEFAULT_ZLIB_LEVEL)
}

/// Compresses `data` as a zlib-wrapped DEFLATE stream.
///
/// `level` follows the zlib scale: `0` stores without compression and `9`
/// requests the strongest compression.
pub fn compress_zlib_with_level(data: &[u8], level: u32) -> Result<Vec<u8>> {
    if level > 9 {
        return Err(RsbError::InvalidZlibLevel { level });
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
    encoder
        .write_all(data)
        .map_err(|source| RsbError::ZlibCompression { source })?;
    encoder
        .finish()
        .map_err(|source| RsbError::ZlibCompression { source })
}

/// Decompresses a zlib-wrapped DEFLATE stream.
///
/// Trailing bytes after the zlib stream are permitted so this function can
/// directly consume the zero-padded data stored in an RSG section.
pub fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|source| RsbError::ZlibDecompression { source })?;
    Ok(output)
}

/// Decompresses a zlib stream and verifies its exact output size.
pub fn decompress_zlib_exact(data: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let output = decompress_zlib(data)?;
    if output.len() != expected_size {
        return Err(RsbError::ZlibSizeMismatch {
            expected: expected_size,
            actual: output.len(),
        });
    }
    Ok(output)
}

/// Compresses data using the RSG convention: zlib level 6 followed by zero
/// padding to a 4096-byte boundary.
pub fn compress_rsg_zlib(data: &[u8]) -> Result<Vec<u8>> {
    compress_rsg_zlib_with_level(data, DEFAULT_ZLIB_LEVEL)
}

/// Compresses data using the RSG zlib and alignment convention at `level`.
pub fn compress_rsg_zlib_with_level(data: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut output = compress_zlib_with_level(data, level)?;
    let aligned_len = aligned_data_len(output.len())?;
    output.resize(aligned_len, 0);
    Ok(output)
}

pub(crate) fn padding_for_data_len(len: usize) -> Result<usize> {
    Ok(aligned_data_len(len)? - len)
}

fn aligned_data_len(len: usize) -> Result<usize> {
    if len == 0 {
        return Ok(0);
    }
    len.checked_add(RSG_DATA_ALIGNMENT - 1)
        .map(|value| value / RSG_DATA_ALIGNMENT * RSG_DATA_ALIGNMENT)
        .ok_or_else(|| RsbError::Other("RSG data alignment overflow".into()))
}
