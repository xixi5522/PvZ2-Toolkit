use crate::codecs::bzip::Error;

/// Input, output, and temporary-memory ceilings for a codec operation.
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
    /// BZip2 block size in units of 100 KiB (`1..=9`).
    pub block_size: u8,
    pub limits: ResourceLimits,
}

impl Default for EncoderOptions {
    fn default() -> Self {
        Self {
            block_size: 1,
            limits: ResourceLimits::default(),
        }
    }
}

impl EncoderOptions {
    pub fn validate(self) -> Result<Self, Error> {
        if !(1..=9).contains(&self.block_size) {
            return Err(Error::invalid_options("BZip2 block size must be in 1..=9"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecoderOptions {
    pub expected_size: usize,
    pub limits: ResourceLimits,
}

impl DecoderOptions {
    pub const fn new(expected_size: usize) -> Self {
        Self {
            expected_size,
            limits: ResourceLimits::UNLIMITED,
        }
    }
}

pub(crate) fn encode_workspace(input_size: usize, block_size: u8) -> Option<usize> {
    let block = input_size.min(usize::from(block_size) * 100_000);
    block
        .checked_mul(core::mem::size_of::<usize>() * 6 + 6)?
        .checked_add(256 * 1024)
}

pub(crate) fn decode_workspace(block_size: u8) -> Option<usize> {
    let block = usize::from(block_size) * 100_000;
    block
        .checked_mul(core::mem::size_of::<usize>() * 3 + 5)?
        .checked_add(256 * 1024)
}
