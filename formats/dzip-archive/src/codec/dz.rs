//! Adapter for the optional native DZ range/LZ codec.

#[cfg(feature = "dz")]
use super::CodecLimits;
#[cfg(all(not(feature = "dz"), feature = "encode"))]
use super::{Codec, CodecError};
#[cfg(feature = "dz")]
use crate::codecs::dz;
use crate::{RangeSettings, Result};

#[cfg(feature = "dz")]
pub use dz::DzCommonBuffer;

/// Archive-wide native-DZ encoder and COMBUF analysis settings.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DzOptions {
    pub settings: RangeSettings,
    pub max_mem_usage: i32,
    pub use_combuf: bool,
    pub preprocess: bool,
    pub trim_reference_factor: i32,
    pub max_common_match: usize,
}

impl Default for DzOptions {
    fn default() -> Self {
        Self {
            settings: RangeSettings::default(),
            max_mem_usage: -1,
            use_combuf: false,
            preprocess: true,
            trim_reference_factor: 20,
            max_common_match: usize::MAX,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DzDecodeContext {
    pub settings: RangeSettings,
    retained_bytes: usize,
    #[cfg(feature = "dz")]
    common_buffer: Option<DzCommonBuffer>,
}

impl DzDecodeContext {
    pub(crate) fn from_encoded_chunks(
        settings: RangeSettings,
        chunks: Vec<Vec<u8>>,
    ) -> Result<Self> {
        let retained_bytes = chunks.iter().try_fold(0usize, |total, chunk| {
            total
                .checked_add(chunk.len())
                .ok_or_else(|| crate::DzipError::InvalidArchive("COMBUF size overflow".to_string()))
        })?;
        #[cfg(feature = "dz")]
        {
            let common_buffer = if chunks.is_empty() {
                None
            } else {
                Some(DzCommonBuffer::new(settings.into(), chunks)?)
            };
            Ok(Self {
                settings,
                retained_bytes,
                common_buffer,
            })
        }
        #[cfg(not(feature = "dz"))]
        {
            let _ = chunks;
            Ok(Self {
                settings,
                retained_bytes,
            })
        }
    }

    pub const fn has_common_buffer(&self) -> bool {
        #[cfg(feature = "dz")]
        {
            self.common_buffer.is_some()
        }
        #[cfg(not(feature = "dz"))]
        {
            false
        }
    }

    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

#[cfg(feature = "encode")]
pub(crate) struct EncodedDzArchive {
    pub(crate) chunks: Vec<Vec<u8>>,
    pub(crate) common_buffer: Option<Vec<u8>>,
}

#[cfg(feature = "dz")]
pub(crate) fn encode(input: &[u8], settings: RangeSettings) -> Result<Vec<u8>> {
    dz::compress_chunk(input, settings.into()).map_err(Into::into)
}

#[cfg(feature = "dz")]
pub(crate) fn decode_with_buffer(
    input: &[u8],
    expected_length: usize,
    context: &DzDecodeContext,
    limits: CodecLimits,
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    let mut decoder = dz::Decoder::with_output(
        dz::DecoderOptions {
            settings: context.settings.into(),
            expected_size: expected_length,
            common_buffer: context.common_buffer.as_ref(),
            limits: dz::ResourceLimits {
                max_input_size: limits.max_input_size,
                max_output_size: limits.max_output_size,
                max_workspace_size: limits.max_workspace_size,
            },
        },
        output,
    )?;
    decoder.decode(input)?;
    Ok(decoder.take_output())
}

#[cfg(feature = "encode")]
pub(crate) fn encode_archive(inputs: &[&[u8]], options: &DzOptions) -> Result<EncodedDzArchive> {
    #[cfg(feature = "dz")]
    {
        let encoded = dz::compress_archive_slices(inputs, &options.to_engine())?;
        Ok(EncodedDzArchive {
            chunks: encoded.chunks,
            common_buffer: encoded.common_buffer,
        })
    }
    #[cfg(not(feature = "dz"))]
    {
        let _ = (inputs, options);
        Err(CodecError::Unavailable { codec: Codec::Dz }.into())
    }
}

#[cfg(all(feature = "dz", feature = "encode"))]
impl DzOptions {
    fn to_engine(&self) -> dz::DzEncoderOptions {
        dz::DzEncoderOptions {
            settings: self.settings.into(),
            max_mem_usage: self.max_mem_usage,
            use_combuf: self.use_combuf,
            preprocess: self.preprocess,
            trim_reference_factor: self.trim_reference_factor,
            max_common_match: self.max_common_match,
        }
    }
}

#[cfg(feature = "dz")]
impl From<RangeSettings> for dz::RangeSettings {
    fn from(settings: RangeSettings) -> Self {
        Self {
            win_size: settings.win_size,
            flags: settings.flags,
            offset_table_size: settings.offset_table_size,
            offset_tables: settings.offset_tables,
            offset_contexts: settings.offset_contexts,
            ref_length_table_size: settings.ref_length_table_size,
            ref_length_tables: settings.ref_length_tables,
            ref_offset_table_size: settings.ref_offset_table_size,
            ref_offset_tables: settings.ref_offset_tables,
            big_min_match: settings.big_min_match,
        }
    }
}
