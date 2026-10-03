use crate::codecs::dz::chunk::{decode_common_payload, encode_grouped, encode_recent_distance};
use crate::codecs::dz::matchfinder::{
    LazyLzParser, LzDecision, MatchCost, MatchScoring, local_match_key,
};
use crate::codecs::dz::model::{CommonFrequencies, CommonModels, DzFixedCosts};
use crate::codecs::dz::range::RangeEncoder;
use crate::codecs::dz::{DzipError, END_SYMBOL, MIN_MATCH, RangeSettings, Result};
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
pub struct DzCommonBuffer {
    chunks: Vec<Vec<u8>>,
    payload_starts: Vec<usize>,
    prefix_size: usize,
    settings: RangeSettings,
}

impl DzCommonBuffer {
    pub fn new(settings: RangeSettings, chunks: Vec<Vec<u8>>) -> Result<Self> {
        let settings = settings.validate()?;
        if chunks.is_empty() {
            return Err(DzipError::InvalidDz(
                "a common buffer must contain at least one chunk".to_string(),
            ));
        }
        let prefix_size = settings.combuf_static_prefix_size();
        let mut payload_starts = Vec::with_capacity(chunks.len() + 1);
        payload_starts.push(0);
        for chunk in &chunks {
            if chunk.len() < prefix_size {
                if chunk.is_empty() {
                    payload_starts.push(*payload_starts.last().unwrap());
                    continue;
                }
                return Err(DzipError::InvalidDz(format!(
                    "COMBUF chunk is {} bytes, smaller than its {} byte static prefix",
                    chunk.len(),
                    prefix_size
                )));
            }
            let next = payload_starts
                .last()
                .copied()
                .unwrap_or(0usize)
                .checked_add(chunk.len() - prefix_size)
                .ok_or_else(|| DzipError::InvalidDz("COMBUF size overflow".to_string()))?;
            payload_starts.push(next);
        }
        Ok(Self {
            chunks,
            payload_starts,
            prefix_size,
            settings,
        })
    }

    pub fn payload_len(&self) -> usize {
        self.payload_starts.last().copied().unwrap_or(0)
    }

    pub(crate) fn decode_at(
        &self,
        absolute_offset: usize,
        length: usize,
    ) -> Result<(Vec<u8>, usize)> {
        let chunk_index = self
            .payload_starts
            .windows(2)
            .position(|range| absolute_offset >= range[0] && absolute_offset < range[1])
            .ok_or_else(|| {
                DzipError::InvalidDz(format!(
                    "COMBUF offset {} exceeds {} bytes",
                    absolute_offset,
                    self.payload_len()
                ))
            })?;
        let local_offset = absolute_offset - self.payload_starts[chunk_index];
        let chunk = &self.chunks[chunk_index];
        let payload_start = self.prefix_size + local_offset;
        let payload = chunk.get(payload_start..).ok_or_else(|| {
            DzipError::InvalidDz("COMBUF reference starts outside its chunk".to_string())
        })?;
        let initial_models = if self.prefix_size == 0 {
            CommonModels::uniform(self.settings)?
        } else {
            CommonModels::from_static_prefix(&chunk[..self.prefix_size], self.settings)?
        };
        decode_common_payload(payload, length, self.settings, initial_models)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CommonSegment {
    pub(crate) source_file: usize,
    pub(crate) source_position: usize,
    pub(crate) raw: Vec<u8>,
    pub(crate) lookahead: Vec<u8>,
    pub(crate) decision_len: usize,
    pub(crate) allow_position_zero: bool,
    pub(crate) emit_end: bool,
    pub(crate) trailing_literal: Option<u8>,
    pub(crate) encoded: Vec<u8>,
    pub(crate) target: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommonReference {
    pub(crate) position: usize,
    pub(crate) length: usize,
    pub(crate) segment: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommonSelection {
    pub(crate) target_file: usize,
    pub(crate) target_position: usize,
    pub(crate) source_file: usize,
    pub(crate) source_position: usize,
    pub(crate) length: usize,
}

#[derive(Debug)]
pub(crate) struct CommonRoot {
    pub(crate) source_file: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) boundaries: Vec<usize>,
}

pub(crate) fn build_common_static_prefix(
    segments: &[CommonSegment],
    settings: RangeSettings,
) -> Result<(Vec<u8>, DzFixedCosts)> {
    let mut frequencies = CommonFrequencies::new(settings);
    for segment in segments {
        analyze_common_segment(
            &segment.lookahead,
            segment.decision_len,
            segment.allow_position_zero,
            segment.trailing_literal,
            settings,
            &mut frequencies,
        )?;
    }
    let prefix = frequencies.normalized_prefix();
    if prefix.len() != settings.combuf_static_prefix_size() {
        return Err(DzipError::InvalidDz(
            "internal COMBUF static table size mismatch".to_string(),
        ));
    }
    let costs = frequencies.fixed_costs(settings);
    Ok((prefix, costs))
}

fn analyze_common_segment(
    input: &[u8],
    boundary: usize,
    allow_position_zero: bool,
    trailing_literal: Option<u8>,
    settings: RangeSettings,
    frequencies: &mut CommonFrequencies,
) -> Result<()> {
    let mut recent_offsets = [0usize; 4];
    let window = 1usize
        .checked_shl(u32::from(settings.win_size))
        .unwrap_or(usize::MAX);
    let mut parser = if allow_position_zero {
        LazyLzParser::new_common(window)
    } else {
        LazyLzParser::new(window)
    };
    while let Some(decision) = parser.next_bounded(
        input,
        boundary,
        MatchCost {
            scoring: MatchScoring::Heuristic,
            recent_offsets: &recent_offsets,
        },
    )? {
        match decision {
            LzDecision::Literal { position } => {
                frequencies.record_top(usize::from(input[position]))?;
            }
            LzDecision::Match {
                length, distance, ..
            } => {
                let symbol = length + 254;
                frequencies.record_top(symbol)?;
                let code = encode_recent_distance(distance, &mut recent_offsets);
                let context = usize::min(length - MIN_MATCH, frequencies.offsets.len() - 1);
                frequencies.record_grouped(context, settings.offset_table_size, code)?;
            }
        }
    }
    if let Some(literal) = trailing_literal {
        frequencies.record_top(usize::from(literal))?;
    }
    frequencies.record_top(END_SYMBOL)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compress_common_segment(
    input: &[u8],
    boundary: usize,
    allow_position_zero: bool,
    stale_keys: &mut BTreeSet<u32>,
    emit_end: bool,
    trailing_literal: Option<u8>,
    settings: RangeSettings,
    mut models: CommonModels,
    fixed_costs: &DzFixedCosts,
) -> Result<Vec<u8>> {
    let mut encoder = RangeEncoder::new();
    let mut recent_offsets = [0usize; 4];
    let window = 1usize
        .checked_shl(u32::from(settings.win_size))
        .unwrap_or(usize::MAX);
    let mut parser = LazyLzParser::new_with_stale_keys(window, allow_position_zero, stale_keys);
    while let Some(decision) = parser.next_bounded(
        input,
        boundary,
        MatchCost {
            scoring: MatchScoring::Fixed {
                costs: fixed_costs,
                dynamic_offsets: &models.offsets,
            },
            recent_offsets: &recent_offsets,
        },
    )? {
        match decision {
            LzDecision::Literal { position } => {
                encoder.encode(&mut models.top, usize::from(input[position]))?;
            }
            LzDecision::Match {
                length, distance, ..
            } => {
                encoder.encode(&mut models.top, length + 254)?;
                let code = encode_recent_distance(distance, &mut recent_offsets);
                let context = usize::min(length - MIN_MATCH, models.offsets.len() - 1);
                encode_grouped(
                    &mut encoder,
                    &mut models.offsets[context],
                    settings.offset_table_size,
                    code,
                )?;
            }
        }
    }
    if let Some(literal) = trailing_literal {
        encoder.encode(&mut models.top, usize::from(literal))?;
    }
    if emit_end {
        encoder.encode(&mut models.top, END_SYMBOL)?;
    }
    let encoded = encoder.finish();
    let history_start = usize::from(!allow_position_zero);
    for position in history_start..boundary {
        if let Some(key) = local_match_key(input, position) {
            stale_keys.insert(key);
        }
    }
    Ok(encoded)
}

pub(crate) fn validate_common_settings(settings: RangeSettings, use_combuf: bool) -> Result<()> {
    if !use_combuf {
        return Ok(());
    }
    if settings.flags & RangeSettings::USE_COMBUF_STATIC_TABLES == 0 {
        return Err(DzipError::InvalidDz(
            "dzip.exe requires COMBUF static tables when common references are enabled".to_string(),
        ));
    }
    if settings.ref_length_table_size == 0
        || settings.ref_offset_table_size == 0
        || settings.ref_length_tables == 0
        || settings.ref_offset_tables == 0
    {
        return Err(DzipError::InvalidDz(
            "COMBUF references require length and offset continuation models".to_string(),
        ));
    }
    if settings.big_min_match == 0 {
        return Err(DzipError::InvalidDz(
            "BigMinMatch must not be zero when common references are enabled".to_string(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedReference {
    pub(crate) position: usize,
    pub(crate) length: usize,
    pub(crate) target: usize,
    pub(crate) next_base: usize,
}
