use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct DecodeParams {
    pub width: u32,
    pub height: u32,
    pub alpha_offset: u32,
    pub reserved: u32,
    pub blocks_x: u32,
    pub blocks_y: u32,
    pub output_stride_words: u32,
    pub alpha_mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct BlockEncodeParams {
    pub width: u32,
    pub height: u32,
    pub block_width: u32,
    pub block_height: u32,
    pub blocks_x: u32,
    pub blocks_y: u32,
    pub output_offset_words: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct AlphaEncodeParams {
    pub width: u32,
    pub height: u32,
    pub reserved_0: u32,
    pub reserved_1: u32,
    pub output_word_count: u32,
    pub reserved_2: u32,
    pub output_offset_words: u32,
    pub mode: u32,
}
