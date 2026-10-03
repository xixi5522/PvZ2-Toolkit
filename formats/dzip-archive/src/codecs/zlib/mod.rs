//! Internal safe, dependency-free RFC 1951 DEFLATE and RFC 1950 zlib codec.
//!
//! The implementation requires only `alloc`. Dzip's unusual
//! gzip-without-trailer framing intentionally remains in the `dzip` crate.

mod bitstream;
mod checksum;
mod codec;
mod engine;
mod error;
mod matchfinder;
mod options;

#[allow(unused_imports)]
pub use codec::{Decoder, Encoder, decode, encode};
#[allow(unused_imports)]
pub use error::{Error, ErrorKind};
#[allow(unused_imports)]
pub use options::{DecoderOptions, EncoderOptions, ResourceLimits, StreamFormat};

use alloc::vec::Vec;

/// Encode a raw RFC 1951 stream without applying resource limits.
pub fn encode_raw(input: &[u8]) -> Vec<u8> {
    engine::encode_raw(input)
}

/// Decode a raw RFC 1951 stream to exactly `expected_size` bytes.
pub fn decode_raw(input: &[u8], expected_size: usize) -> Result<Vec<u8>, Error> {
    decode(
        input,
        &DecoderOptions {
            format: StreamFormat::RawDeflate,
            expected_size,
            ..DecoderOptions::new(expected_size)
        },
    )
}

/// Encode an RFC 1950 zlib stream without applying resource limits.
pub fn encode_zlib(input: &[u8]) -> Vec<u8> {
    engine::encode_zlib(input)
}

/// Decode an RFC 1950 zlib stream to exactly `expected_size` bytes.
pub fn decode_zlib(input: &[u8], expected_size: usize) -> Result<Vec<u8>, Error> {
    decode(input, &DecoderOptions::new(expected_size))
}

pub fn adler32(input: &[u8]) -> u32 {
    checksum::adler32(input)
}
