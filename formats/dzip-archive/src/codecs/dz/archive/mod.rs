mod analysis;
pub(crate) mod common;

use self::analysis::find_common_references;
use self::common::{
    DzCommonBuffer, ResolvedReference, build_common_static_prefix, compress_common_segment,
    validate_common_settings,
};
use crate::codecs::dz::chunk::{compress_chunk, compress_chunk_with_references};
use crate::codecs::dz::model::{CommonModels, DzFixedCosts};
use crate::codecs::dz::{DzipError, RangeSettings, Result};
use alloc::collections::BTreeSet;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};

#[derive(Clone, Debug)]
pub struct DzEncoderOptions {
    pub settings: RangeSettings,
    pub max_mem_usage: i32,
    pub use_combuf: bool,
    pub preprocess: bool,
    pub trim_reference_factor: i32,
    pub max_common_match: usize,
}

impl Default for DzEncoderOptions {
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
pub struct EncodedDzArchive {
    pub chunks: Vec<Vec<u8>>,
    pub common_buffer: Option<Vec<u8>>,
}

pub fn compress_archive(
    inputs: &[Vec<u8>],
    options: &DzEncoderOptions,
) -> Result<EncodedDzArchive> {
    let inputs = inputs.iter().map(Vec::as_slice).collect::<Vec<_>>();
    compress_archive_slices(&inputs, options)
}

/// Compress archive-scoped DZ chunks without requiring the caller to clone
/// input buffers into a second `Vec<Vec<u8>>`.
pub fn compress_archive_slices(
    inputs: &[&[u8]],
    options: &DzEncoderOptions,
) -> Result<EncodedDzArchive> {
    let settings = options.settings.validate()?;
    if inputs.len() > 0x8000 {
        return Err(DzipError::InvalidDz(
            "dzip.exe accepts at most 32768 DZ chunks".to_string(),
        ));
    }
    if options.max_mem_usage >= 0 {
        // sub_411090 performs this minimum allocation check before preparing
        // any per-chunk encoder state. The original subsequently crashes when
        // its caller ignores the failure, so report the condition cleanly.
        let input_bytes = inputs.iter().try_fold(0usize, |total, input| {
            total.checked_add(input.len()).ok_or_else(|| {
                DzipError::InvalidDz("DZ input size overflows memory estimate".to_string())
            })
        })?;
        let required = inputs
            .len()
            .checked_mul(0x40000)
            .and_then(|base| {
                input_bytes
                    .checked_mul(2)
                    .and_then(|data| base.checked_add(data))
            })
            .ok_or_else(|| DzipError::InvalidDz("DZ memory estimate overflow".to_string()))?;
        if required > options.max_mem_usage as usize {
            return Err(DzipError::InvalidDz(format!(
                "max_mem_usage {} is below dzip.exe's minimum DZ estimate {}",
                options.max_mem_usage, required
            )));
        }
    }
    if !options.use_combuf {
        let chunks = inputs
            .iter()
            .map(|input| compress_chunk(input, settings))
            .collect::<Result<Vec<_>>>()?;
        return Ok(EncodedDzArchive {
            chunks,
            common_buffer: None,
        });
    }
    validate_common_settings(settings, true)?;

    let (mut segments, references) = find_common_references(
        inputs,
        settings,
        options.preprocess,
        usize::from(settings.big_min_match),
        options.max_common_match,
        options.trim_reference_factor,
    )?;

    if segments.is_empty() {
        let chunks = inputs
            .iter()
            .map(|input| compress_chunk_with_references(input, settings, true, &[], &[]))
            .collect::<Result<Vec<_>>>()?;
        return Ok(EncodedDzArchive {
            chunks,
            // Enabling COMBUF expands the DZ top model even when the selector
            // retains no references. dzip.exe also registers a zero-length
            // CHUNK_COMBUF placeholder in that case.
            common_buffer: Some(Vec::new()),
        });
    }

    let prefix_size = settings.combuf_static_prefix_size();
    let (static_prefix, common_costs) = if prefix_size == 0 {
        (
            Vec::new(),
            DzFixedCosts {
                top: vec![8; 514],
                offsets: (0..settings.offset_contexts)
                    .map(|_| {
                        (0..settings.offset_tables)
                            .map(|_| {
                                vec![
                                    i32::from(settings.offset_table_size);
                                    1usize << settings.offset_table_size
                                ]
                            })
                            .collect()
                    })
                    .collect(),
                offset_bits: settings.offset_table_size,
            },
        )
    } else {
        build_common_static_prefix(&segments, settings)?
    };
    let initial_models = if static_prefix.is_empty() {
        CommonModels::uniform(settings)?
    } else {
        CommonModels::from_static_prefix(&static_prefix, settings)?
    };
    let mut common_bytes = static_prefix;
    // The combined COMBUF range stream retains the initial cache byte. Source
    // restart offsets are measured after it.
    common_bytes.push(0x7f);
    let mut payload_offset = 1usize;
    let mut stale_match_keys = BTreeSet::new();
    for segment in &mut segments {
        segment.target = payload_offset;
        segment.encoded = compress_common_segment(
            &segment.lookahead,
            segment.decision_len,
            segment.allow_position_zero,
            &mut stale_match_keys,
            segment.emit_end,
            segment.trailing_literal,
            settings,
            initial_models.clone(),
            &common_costs,
        )?;
        payload_offset = payload_offset
            .checked_add(segment.encoded.len())
            .ok_or_else(|| DzipError::InvalidDz("COMBUF size overflow".to_string()))?;
        common_bytes.extend_from_slice(&segment.encoded);
    }

    let common_decoder = DzCommonBuffer::new(settings, vec![common_bytes.clone()])?;
    for (segment_index, segment) in segments.iter().enumerate() {
        let (decoded, _) = common_decoder.decode_at(segment.target, segment.raw.len())?;
        if decoded != segment.raw {
            let first_difference = decoded
                .iter()
                .zip(&segment.raw)
                .position(|(actual, expected)| actual != expected);
            return Err(DzipError::InvalidDz(format!(
                "internal COMBUF validation failed for segment {} (raw {}, encoded {}, first difference {:?})",
                segment_index,
                segment.raw.len(),
                segment.encoded.len(),
                first_difference
            )));
        }
    }

    let mut resolved_references = vec![Vec::new(); inputs.len()];
    for (file_index, chunk_references) in references.iter().enumerate() {
        for reference in chunk_references {
            let target = segments[reference.segment].target;
            let (decoded, consumed) = common_decoder.decode_at(target, reference.length)?;
            let expected = inputs[file_index]
                .get(reference.position..reference.position + reference.length)
                .ok_or_else(|| {
                    DzipError::InvalidDz("internal COMBUF reference overflow".to_string())
                })?;
            if decoded != expected {
                return Err(DzipError::InvalidDz(format!(
                    "internal COMBUF reference validation failed for file {} at {}",
                    file_index, reference.position
                )));
            }
            resolved_references[file_index].push(ResolvedReference {
                position: reference.position,
                length: reference.length,
                target,
                // sub_49BF00 only builds source attachment chains when
                // trimming is enabled. The common encoder writes the
                // compressed endpoint cached by those chains into its recent
                // reference-base slot; with trimming disabled that record
                // field remains zero in dzip.exe.
                next_base: if options.trim_reference_factor == 0 {
                    0
                } else {
                    target.checked_add(consumed).ok_or_else(|| {
                        DzipError::InvalidDz("COMBUF reference endpoint overflow".to_string())
                    })?
                },
            });
        }
    }

    let mut source_boundaries = vec![Vec::new(); inputs.len()];
    for segment in &segments {
        source_boundaries[segment.source_file].push(segment.source_position);
    }
    for boundaries in &mut source_boundaries {
        boundaries.sort_unstable();
        boundaries.dedup();
    }

    let chunks = inputs
        .iter()
        .zip(&resolved_references)
        .zip(&source_boundaries)
        .map(|((input, resolved), boundaries)| {
            compress_chunk_with_references(input, settings, true, resolved, boundaries)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(EncodedDzArchive {
        chunks,
        common_buffer: Some(common_bytes),
    })
}
