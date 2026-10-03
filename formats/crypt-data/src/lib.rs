//! Reader and writer for PopCap Crypt-Data resources.
//!
//! Crypt-Data conditionally wraps a payload in a `CRYPT_RES` header and XORs a
//! configurable prefix using a repeating byte key. The default prefix limit is
//! 256 bytes. Payloads shorter than the selected limit pass through unchanged.

mod codec;
mod error;
mod header;

pub use codec::{
    decrypt, decrypt_with_limit, decrypt_wrapped, encrypt, encrypt_with_limit, inspect,
};
pub use error::{CryptDataError, Result};

/// Marker at the start of wrapped Crypt-Data resources: `CRYPT_RES\n\0`.
pub const MAGIC: [u8; 11] = *b"CRYPT_RES\n\0";

/// Marker and little-endian `u64` raw-size field.
pub const HEADER_SIZE: usize = MAGIC.len() + size_of::<u64>();

/// Number of payload bytes encrypted by the conventional PopCap layout.
pub const DEFAULT_LIMIT: usize = 0x100;

/// Conventional key used by PopCap Crypt-Data resources.
pub const DEFAULT_KEY: &[u8] = b"AS23DSREPLKL335KO4439032N8345NF";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CryptDataMetadata {
    /// Whether the input contains a `CRYPT_RES` wrapper.
    pub wrapped: bool,
    /// Original unwrapped payload size.
    pub raw_size: usize,
    /// Payload bytes present after the wrapper, or the full input size when unwrapped.
    pub payload_size: usize,
    /// Prefix limit used to interpret the resource.
    pub limit: usize,
}
