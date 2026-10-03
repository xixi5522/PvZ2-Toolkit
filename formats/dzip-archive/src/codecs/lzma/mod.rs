//! Internal safe, dependency-free LZMA1 range coder.
//!
//! The implementation requires only `alloc`. It operates on raw
//! LZMA1 payloads; the LZMA-alone length header used by Dzip belongs to the
//! `dzip` crate.

mod codec;
mod engine;
mod error;
mod matchfinder;
mod options;
mod range;

#[allow(unused_imports)]
pub use codec::{Decoder, Encoder, decode, encode};
#[allow(unused_imports)]
pub use error::{Error, ErrorKind};
#[allow(unused_imports)]
pub use options::{DecoderOptions, EncoderOptions, LzmaProps, ResourceLimits};

use alloc::vec::Vec;

/// Backwards-compatible alias for callers that distinguish decoder failures.
pub type DecodeError = Error;

/// Return the five-byte LZMA-alone properties field after validation.
pub fn decoder_props(props: &LzmaProps) -> Result<[u8; 5], Error> {
    props.decoder_properties()
}

/// Encode a raw LZMA1 payload terminated by the standard end marker.
pub fn encode_with_end_marker(input: &[u8], props: &LzmaProps) -> Result<Vec<u8>, Error> {
    encode(
        input,
        &EncoderOptions {
            properties: *props,
            ..EncoderOptions::default()
        },
    )
}

/// Compatibility name for the checked encoder.
pub fn encode_checked(input: &[u8], props: &LzmaProps) -> Result<Vec<u8>, Error> {
    encode_with_end_marker(input, props)
}

/// Decode a raw LZMA1 payload to exactly `expected_size` bytes.
pub fn decode_raw(
    input: &[u8],
    properties: &[u8; 5],
    expected_size: usize,
) -> Result<Vec<u8>, Error> {
    decode(
        input,
        &DecoderOptions::from_decoder_properties(*properties, expected_size)?,
    )
}
