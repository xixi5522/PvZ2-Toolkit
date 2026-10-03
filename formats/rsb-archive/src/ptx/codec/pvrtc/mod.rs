//! Pure Rust PVRTC1 4bpp codec.
//!
//! The packet layout and baseline endpoint/modulation strategy are derived
//! from Jeffrey Lim's PVRTCCompressor reference implementation. See
//! THIRD_PARTY_NOTICES.md for the BSD-3-Clause notice.

mod decode;
mod encode;
mod layout;
mod packet;

pub use decode::{decode_4bpp, decode_pvrtc_4bpp_a8_rgba8, decode_pvrtc_4bpp_rgba8};
pub use encode::{encode_pvrtc_4bpp_rgba8, encode_rgba_4bpp};
pub use packet::PvrTcPacket;
