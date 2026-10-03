//! Reader and writer for PopCap encrypted Compiled Text files.
//!
//! The format stores an SMF/zlib container encrypted with Rijndael-192-CBC,
//! then encodes the ciphertext as Base64. Both known SMF header layouts are
//! supported and detected automatically while decoding.

mod codec;
mod crypto;
mod error;
mod options;
mod types;

pub use codec::{
    decode, decode_detailed, decode_with_options, encode, encode_with_options, inspect,
};
pub use error::{CompiledTextError, Result};
pub use options::{DecodeOptions, EncodeOptions};
pub use types::{CompiledTextMetadata, DecodedCompiledText};

pub use smf_container::SmfVariant;

/// Rijndael block size used by Compiled Text files, in bytes.
pub const RIJNDAEL_BLOCK_SIZE: usize = 24;
