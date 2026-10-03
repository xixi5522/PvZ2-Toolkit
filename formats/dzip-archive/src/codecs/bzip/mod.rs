//! Internal safe, dependency-free BZip2 encoder and decoder.
//!
//! The implementation requires only `alloc`. [`Encoder`] and
//! [`Decoder`] retain their output allocations across calls; the free
//! functions are convenient one-shot wrappers.

mod bitstream;
mod checksum;
mod codec;
mod engine;
mod error;
mod options;
mod randtable;
mod transform;

#[allow(unused_imports)]
pub use codec::{Decoder, Encoder, decode, encode};
#[allow(unused_imports)]
pub use error::{Error, ErrorKind};
#[allow(unused_imports)]
pub use options::{DecoderOptions, EncoderOptions, ResourceLimits};

use alloc::vec::Vec;

/// Encode using the default 100-KiB BZip2 block size.
pub fn encode_default(input: &[u8]) -> Result<Vec<u8>, Error> {
    encode(input, &EncoderOptions::default())
}

/// Compatibility helper for choosing a BZip2 block size in `1..=9`.
pub fn encode_with_block_size(input: &[u8], block_size: u8) -> Result<Vec<u8>, Error> {
    encode(
        input,
        &EncoderOptions {
            block_size,
            ..EncoderOptions::default()
        },
    )
}

/// Decode one or more concatenated BZip2 streams to exactly `expected_size` bytes.
pub fn decode_exact(input: &[u8], expected_size: usize) -> Result<Vec<u8>, Error> {
    decode(input, &DecoderOptions::new(expected_size))
}
