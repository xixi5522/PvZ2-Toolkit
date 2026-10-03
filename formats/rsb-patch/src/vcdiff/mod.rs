//! Pure Rust VCDIFF support used by PopCap RSBPatch files.
//!
//! Decoding accepts RFC 3284's standard sectioned representation and the
//! open-vcdiff `S` extension used by Twinning. Encoding uses that interleaved
//! representation by default.

mod address_cache;
mod code_table;
mod decoder;
mod encoder;
mod io;
mod options;

use crate::PatchError;

pub use decoder::{decode, decode_stream, decode_with_options};
pub use encoder::{encode, encode_stream, encode_with_options};
pub use options::{DecodeOptions, EncodeOptions, VcdiffFormat};

const MAGIC: [u8; 3] = [0xD6, 0xC3, 0xC4];
const STANDARD_VERSION: u8 = 0;
const EXTENDED_VERSION: u8 = b'S';
const VCD_DECOMPRESS: u8 = 0x01;
const VCD_CODETABLE: u8 = 0x02;
const VCD_SOURCE: u8 = 0x01;
const VCD_TARGET: u8 = 0x02;
const VCD_CHECKSUM: u8 = 0x04;

fn invalid(message: impl Into<String>) -> PatchError {
    PatchError::InvalidVcdiff(message.into())
}
