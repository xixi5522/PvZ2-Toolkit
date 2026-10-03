//! Reader and writer for PopCap's SMF zlib container.
//!
//! SMF stores a magic value and the uncompressed payload size before a zlib
//! stream. Two header layouts are in use: an 8-byte compact header and a
//! 16-byte extended header. This crate detects both layouts automatically.

mod error;
mod reader;
mod tag;
mod types;
mod writer;

pub use error::{Result, SmfError};
pub use reader::{decode, decode_to, inspect};
pub use tag::{md5_hex, tag_contents};
pub use types::{EncodeOptions, SmfMetadata, SmfVariant};
pub use writer::{encode, encode_to};

/// SMF magic as it appears after little-endian decoding.
pub const SMF_MAGIC: u32 = 0xDEAD_FED4;

/// Little-endian bytes stored at the beginning of every SMF file.
pub const SMF_MAGIC_BYTES: [u8; 4] = SMF_MAGIC.to_le_bytes();
