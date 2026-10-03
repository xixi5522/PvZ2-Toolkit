#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamFormat {
    RawDeflate,
    Zlib,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_input_size: usize,
    pub max_output_size: usize,
    pub max_workspace_size: usize,
}

impl ResourceLimits {
    pub const UNLIMITED: Self = Self {
        max_input_size: usize::MAX,
        max_output_size: usize::MAX,
        max_workspace_size: usize::MAX,
    };
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderOptions {
    pub format: StreamFormat,
    pub limits: ResourceLimits,
}

impl Default for EncoderOptions {
    fn default() -> Self {
        Self {
            format: StreamFormat::Zlib,
            limits: ResourceLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderOptions {
    pub format: StreamFormat,
    pub expected_size: usize,
    pub limits: ResourceLimits,
}

impl DecoderOptions {
    pub const fn new(expected_size: usize) -> Self {
        Self {
            format: StreamFormat::Zlib,
            expected_size,
            limits: ResourceLimits::UNLIMITED,
        }
    }
}

pub(crate) fn encode_workspace(input_size: usize) -> Option<usize> {
    input_size
        .checked_mul(core::mem::size_of::<usize>())?
        .checked_add((1usize << 16) * core::mem::size_of::<usize>())?
        .checked_add(64 * 1024)
}

pub(crate) const fn decode_workspace() -> usize {
    // Two full 15-bit Huffman lookup tables plus construction scratch.
    2 * (1usize << 15) * core::mem::size_of::<u32>() + 64 * 1024
}
