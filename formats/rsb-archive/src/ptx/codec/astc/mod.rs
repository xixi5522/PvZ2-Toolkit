//! Pure Rust ASTC LDR codec.

mod decode;
mod decoder_core;
mod encode;
mod format;
mod pack;
mod search;

pub use decode::decode_astc_rgba8;
pub use encode::encode_astc_rgba8;
pub use format::{ASTC_BLOCK_SIZES, AstcQuality, astc_data_size, is_valid_astc_block_size};
