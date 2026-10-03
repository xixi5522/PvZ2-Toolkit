//! Reader and writer for PopCap RSBPatch (`RSBP`) containers.
//!
//! The semantic FourCC is `RSBP`. Because the format writes its numeric magic
//! as a little-endian `u32`, the first four on-disk bytes are `PBSR`.
//!
//! The crate also contains a pure Rust VCDIFF implementation that decodes
//! both RFC 3284 sectioned windows and the interleaved `S` extension emitted
//! by open-vcdiff and Twinning.
//!
//! With the default `rsb` feature, `create_archive_patch` and
//! `apply_archive_patch` implement Twinning's complete RSB v4 workflow,
//! including its optional raw-packet differencing mode.

mod delta;
mod error;
mod hash;
#[cfg(feature = "rsb")]
mod rsb;
mod rsbp;
pub mod vcdiff;

pub use error::{PatchError, Result};
pub use hash::{md5_hash, verify_hash};
#[cfg(feature = "rsb")]
pub use rsb::{
    ArchiveDecodeOptions, ArchiveEncodeOptions, PacketPatchMode, apply_archive_patch,
    create_archive_patch,
};
pub use rsbp::{ContainerDecodeOptions, PacketPatch, RsbPatch};

/// Semantic RSBPatch FourCC, matching Twinning's `0x52534250` marker.
pub const RSB_PATCH_MAGIC: u32 = u32::from_be_bytes(*b"RSBP");

/// Little-endian on-disk representation of [`RSB_PATCH_MAGIC`].
pub const RSB_PATCH_MAGIC_BYTES: [u8; 4] = RSB_PATCH_MAGIC.to_le_bytes();

/// Supported RSBPatch format version.
pub const RSB_PATCH_VERSION: u32 = 1;

/// Fixed package header size, including magic and version.
pub const RSB_PATCH_HEADER_SIZE: usize = 0x30;

/// Fixed packet record size before its optional VCDIFF payload.
pub const RSB_PATCH_PACKET_RECORD_SIZE: usize = 0x98;
