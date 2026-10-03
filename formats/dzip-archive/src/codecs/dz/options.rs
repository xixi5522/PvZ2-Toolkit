use crate::codecs::dz::{DzCommonBuffer, DzError, RangeSettings, Result};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EncoderOptions {
    pub settings: RangeSettings,
    pub limits: ResourceLimits,
}

#[derive(Debug, Clone, Copy)]
pub struct DecoderOptions<'a> {
    pub settings: RangeSettings,
    pub expected_size: usize,
    pub common_buffer: Option<&'a DzCommonBuffer>,
    pub limits: ResourceLimits,
}

impl DecoderOptions<'_> {
    pub const fn new(settings: RangeSettings, expected_size: usize) -> Self {
        Self {
            settings,
            expected_size,
            common_buffer: None,
            limits: ResourceLimits::UNLIMITED,
        }
    }
}

/// Conservative allocation estimate for the adaptive DZ models.
pub fn model_workspace_size(settings: RangeSettings, has_combuf: bool) -> Result<usize> {
    let settings = settings.validate()?;
    let top_symbols = if has_combuf {
        514usize
            .checked_add(1usize << settings.ref_length_table_size)
            .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?
    } else {
        514
    };
    let offset_symbols = 1usize << settings.offset_table_size;
    let offset_models = usize::from(settings.offset_contexts)
        .checked_mul(usize::from(settings.offset_tables))
        .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?;
    let mut symbols = top_symbols
        .checked_add(
            offset_models
                .checked_mul(offset_symbols)
                .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?,
        )
        .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?;
    if has_combuf {
        symbols = symbols
            .checked_add(
                usize::from(settings.ref_length_tables)
                    .checked_mul(1usize << settings.ref_length_table_size)
                    .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?,
            )
            .and_then(|value| {
                value.checked_add(
                    usize::from(settings.ref_offset_tables)
                        .checked_mul(1usize << settings.ref_offset_table_size)?,
                )
            })
            .and_then(|value| value.checked_add(10))
            .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))?;
    }
    // Each AdaptiveModel symbol owns one u16 frequency and one u32 Fenwick
    // entry. Add one extra Fenwick entry and Vec headers per model.
    symbols
        .checked_mul(core::mem::size_of::<u16>() + core::mem::size_of::<u32>())
        .and_then(|bytes| bytes.checked_add((offset_models + 16) * 64))
        .ok_or_else(|| DzError::workspace_limit("DZ model size overflow"))
}

pub(crate) fn encode_workspace_size(input_size: usize, settings: RangeSettings) -> Result<usize> {
    model_workspace_size(settings, false)?
        .checked_mul(2)
        .and_then(|model_bytes| {
            input_size
                .checked_mul(32)
                .and_then(|input_bytes| model_bytes.checked_add(input_bytes))
        })
        .and_then(|bytes| bytes.checked_add(256 * 1024))
        .ok_or_else(|| DzError::workspace_limit("DZ encoder workspace estimate overflow"))
}
