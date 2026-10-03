//! Ericsson Texture Compression (ETC1) codec and PvZ alpha variants.

mod alpha;
mod block;
mod decode;
mod encode;
mod palette;
mod plane;

pub(crate) use alpha::{apply_a8, apply_etc1};
pub use block::{
    ETC1_MODIFIERS, color_clamp, decode_etc1_block, encode_etc1_alpha_block, encode_etc1_block,
    gen_etc1, vertical_etc1,
};
pub use decode::{
    decode_etc1_a8_rgba8, decode_etc1_rgba8, decode_palette_alpha_rgba8,
    decode_palette_alpha_values,
};
pub use encode::{encode_alpha, encode_palette_alpha_rgba8};
pub(crate) use palette::{decode_palette, encode_palette};
pub(crate) use plane::{Etc1Plane, decode_plane, encode_plane};
