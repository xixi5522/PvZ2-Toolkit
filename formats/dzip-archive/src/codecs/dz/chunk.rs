use crate::codecs::dz::archive::common::{
    DzCommonBuffer, ResolvedReference, validate_common_settings,
};
use crate::codecs::dz::matchfinder::{LazyLzParser, LzDecision, MatchCost, MatchScoring};
use crate::codecs::dz::model::{CommonModels, DzFixedCosts, DzFrequencyCounts, DzModels};
use crate::codecs::dz::range::{AdaptiveModel, RangeDecoder, RangeEncoder};
use crate::codecs::dz::{DzipError, END_SYMBOL, MIN_MATCH, RangeSettings, Result};
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

pub fn decompress_chunk(
    input: &[u8],
    expected_size: usize,
    settings: RangeSettings,
) -> Result<Vec<u8>> {
    decompress_chunk_with_common_buffer(input, expected_size, settings, None)
}

pub fn decompress_chunk_with_common_buffer(
    input: &[u8],
    expected_size: usize,
    settings: RangeSettings,
    common_buffer: Option<&DzCommonBuffer>,
) -> Result<Vec<u8>> {
    decompress_chunk_with_output(input, expected_size, settings, common_buffer, Vec::new())
}

pub(crate) fn decompress_chunk_with_output(
    input: &[u8],
    expected_size: usize,
    settings: RangeSettings,
    common_buffer: Option<&DzCommonBuffer>,
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    let settings = settings.validate()?;
    if common_buffer.is_none() {
        return decompress_chunk_with_reference_base_update(
            input,
            expected_size,
            settings,
            common_buffer,
            ReferenceBaseUpdate::CompressedEndpoint,
            output,
        );
    }

    // dzip.exe records the compressed COMBUF endpoint as the next recent
    // base when trimming built attachment chains. With trimming disabled its
    // record field remains zero, so archives produced by that mode require a
    // second decode using the original zero-base behavior.
    match decompress_chunk_with_reference_base_update(
        input,
        expected_size,
        settings,
        common_buffer,
        ReferenceBaseUpdate::CompressedEndpoint,
        output,
    ) {
        Ok(output) => Ok(output),
        Err(endpoint_error) => decompress_chunk_with_reference_base_update(
            input,
            expected_size,
            settings,
            common_buffer,
            ReferenceBaseUpdate::Zero,
            Vec::new(),
        )
        .or(Err(endpoint_error)),
    }
}

#[derive(Clone, Copy)]
enum ReferenceBaseUpdate {
    CompressedEndpoint,
    Zero,
}

fn decompress_chunk_with_reference_base_update(
    input: &[u8],
    expected_size: usize,
    settings: RangeSettings,
    common_buffer: Option<&DzCommonBuffer>,
    base_update: ReferenceBaseUpdate,
    mut output: Vec<u8>,
) -> Result<Vec<u8>> {
    let mut decoder = RangeDecoder::new(input)?;
    let mut models = DzModels::new(settings, common_buffer.is_some())?;
    let mut recent_offsets = [0usize; 4];
    output.clear();
    if output.capacity() < expected_size {
        output.reserve(expected_size);
    }

    loop {
        let symbol = decoder.decode(&mut models.top)?;
        match symbol {
            0..=255 => {
                if output.len() >= expected_size {
                    return Err(DzipError::InvalidDz(
                        "literal follows the expected end of a DZ chunk".to_string(),
                    ));
                }
                output.push(symbol as u8);
            }
            256..=512 => {
                let length = symbol - 254;
                let context = usize::min(length - MIN_MATCH, models.offsets.len() - 1);
                let code = decode_grouped(
                    &mut decoder,
                    &mut models.offsets[context],
                    settings.offset_table_size,
                )?;
                let distance = decode_recent_distance(code, &mut recent_offsets)?;
                copy_match(&mut output, distance, length, expected_size)?;
            }
            END_SYMBOL => {
                if output.len() != expected_size {
                    return Err(DzipError::InvalidDz(format!(
                        "DZ stream ended after {} of {} bytes",
                        output.len(),
                        expected_size
                    )));
                }
                break;
            }
            _ => {
                validate_common_settings(settings, true)?;
                let common_buffer = common_buffer.ok_or_else(|| {
                    DzipError::InvalidDz(
                        "common-buffer reference without a common buffer".to_string(),
                    )
                })?;
                let length = decode_reference_length(
                    symbol,
                    &mut decoder,
                    &mut models.reference_lengths,
                    settings.ref_length_table_size,
                    usize::from(settings.big_min_match),
                )?;
                if output.len().saturating_add(length) > expected_size {
                    return Err(DzipError::InvalidDz(format!(
                        "COMBUF reference of {} bytes exceeds expected output size {}",
                        length, expected_size
                    )));
                }
                let base_index = decoder.decode(&mut models.reference_base)?;
                let negative = decoder.decode(&mut models.reference_sign)? != 0;
                let magnitude = decode_grouped(
                    &mut decoder,
                    &mut models.reference_offsets,
                    settings.ref_offset_table_size,
                )?;
                let delta = if negative {
                    -1i64
                        - i64::try_from(magnitude).map_err(|_| {
                            DzipError::InvalidDz("COMBUF offset delta overflow".to_string())
                        })?
                } else {
                    i64::try_from(magnitude).map_err(|_| {
                        DzipError::InvalidDz("COMBUF offset delta overflow".to_string())
                    })?
                };
                let target = models.reference_bases[base_index]
                    .checked_add(delta)
                    .and_then(|value| value.checked_sub(3))
                    .ok_or_else(|| DzipError::InvalidDz("COMBUF offset overflow".to_string()))?;
                let target = usize::try_from(target).map_err(|_| {
                    DzipError::InvalidDz(format!("negative COMBUF offset {}", target))
                })?;
                let (common_data, consumed) = common_buffer.decode_at(target, length)?;
                output.extend_from_slice(&common_data);
                models.reference_bases[base_index] = match base_update {
                    ReferenceBaseUpdate::CompressedEndpoint => {
                        let endpoint = target.checked_add(consumed).ok_or_else(|| {
                            DzipError::InvalidDz("COMBUF reference endpoint overflow".to_string())
                        })?;
                        i64::try_from(endpoint).map_err(|_| {
                            DzipError::InvalidDz("COMBUF reference endpoint overflow".to_string())
                        })?
                    }
                    ReferenceBaseUpdate::Zero => 0,
                };
            }
        }
    }
    Ok(output)
}

fn decode_reference_length(
    first_symbol: usize,
    decoder: &mut RangeDecoder<'_>,
    models: &mut [AdaptiveModel],
    bits: u8,
    minimum_match: usize,
) -> Result<usize> {
    let first_group = first_symbol.checked_sub(514).ok_or_else(|| {
        DzipError::InvalidDz(format!("invalid COMBUF reference symbol {}", first_symbol))
    })?;
    let grouped = decode_grouped_from_first(decoder, models, bits, first_group)?;
    grouped
        .checked_add(minimum_match)
        .ok_or_else(|| DzipError::InvalidDz("COMBUF reference length overflow".to_string()))
}

fn decode_grouped_from_first(
    decoder: &mut RangeDecoder<'_>,
    models: &mut [AdaptiveModel],
    bits: u8,
    first_group: usize,
) -> Result<usize> {
    let continuation = 1usize << (bits - 1);
    let payload_mask = continuation - 1;
    let mut value = first_group & payload_mask;
    if first_group & continuation == 0 {
        return Ok(value);
    }
    if models.is_empty() {
        return Err(DzipError::InvalidDz(
            "continued grouped integer has no continuation model".to_string(),
        ));
    }
    let mut shift = u32::from(bits - 1);
    let mut table = 0usize;
    loop {
        let group = decoder.decode(&mut models[table])?;
        value |= (group & payload_mask)
            .checked_shl(shift)
            .ok_or_else(|| DzipError::InvalidDz("grouped integer overflow".to_string()))?;
        if group & continuation == 0 {
            return Ok(value);
        }
        shift = shift
            .checked_add(u32::from(bits - 1))
            .ok_or_else(|| DzipError::InvalidDz("grouped integer overflow".to_string()))?;
        table = usize::min(table + 1, models.len() - 1);
        if shift >= usize::BITS {
            return Err(DzipError::InvalidDz(
                "grouped integer is too large".to_string(),
            ));
        }
    }
}

pub(crate) fn decode_common_payload(
    payload: &[u8],
    expected_size: usize,
    settings: RangeSettings,
    initial_models: CommonModels,
) -> Result<(Vec<u8>, usize)> {
    let mut stream_offset = 0usize;
    let mut decoder = RangeDecoder::new(payload)?;
    let mut models = initial_models.clone();
    let mut recent_offsets = [0usize; 4];
    let mut output = Vec::with_capacity(expected_size);

    while output.len() < expected_size {
        let symbol = decoder.decode(&mut models.top)?;
        match symbol {
            0..=255 => output.push(symbol as u8),
            256..=512 => {
                let full_length = symbol - 254;
                let context = usize::min(full_length - MIN_MATCH, models.offsets.len() - 1);
                let code = decode_grouped(
                    &mut decoder,
                    &mut models.offsets[context],
                    settings.offset_table_size,
                )?;
                let distance = decode_recent_distance(code, &mut recent_offsets)?;
                let length = usize::min(full_length, expected_size - output.len());
                copy_match(&mut output, distance, length, expected_size)?;
            }
            END_SYMBOL => {
                stream_offset = stream_offset
                    .checked_add(decoder.finished_position())
                    .ok_or_else(|| DzipError::InvalidDz("COMBUF offset overflow".to_string()))?;
                decoder = RangeDecoder::new(payload.get(stream_offset..).ok_or_else(|| {
                    DzipError::InvalidDz("COMBUF segment ends outside its chunk".to_string())
                })?)?;
                models = initial_models.clone();
            }
            _ => {
                return Err(DzipError::InvalidDz(format!(
                    "invalid COMBUF token {}",
                    symbol
                )));
            }
        }
    }
    let consumed = stream_offset
        .checked_add(decoder.consumed())
        .ok_or_else(|| DzipError::InvalidDz("COMBUF offset overflow".to_string()))?;
    Ok((output, consumed))
}

pub fn compress_chunk(input: &[u8], settings: RangeSettings) -> Result<Vec<u8>> {
    compress_chunk_with_references(input, settings, false, &[], &[])
}

pub(crate) fn compress_chunk_with_output(
    input: &[u8],
    settings: RangeSettings,
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    compress_chunk_with_references_and_output(input, settings, false, &[], &[], output)
}

fn analyze_dz_costs(
    input: &[u8],
    settings: RangeSettings,
    has_combuf: bool,
    references: &[ResolvedReference],
    boundaries: &[usize],
) -> Result<DzFixedCosts> {
    let mut frequencies = DzFrequencyCounts::new(settings, has_combuf);
    let mut recent_offsets = [0usize; 4];
    let window = 1usize
        .checked_shl(u32::from(settings.win_size))
        .unwrap_or(usize::MAX);
    if references.is_empty() && boundaries.is_empty() {
        let mut parser = LazyLzParser::new(window);
        while let Some(decision) = parser.next(
            input,
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
                    frequencies.record_top(length + 254)?;
                    let code = encode_recent_distance(distance, &mut recent_offsets);
                    let context = usize::min(length - MIN_MATCH, frequencies.offsets.len() - 1);
                    frequencies.record_grouped(context, settings.offset_table_size, code)?;
                }
            }
        }
        return Ok(frequencies.into_costs());
    }
    let mut parser = LazyLzParser::new(window);
    let mut position = 0usize;
    let mut reference_index = 0usize;
    let mut boundary_index = 0usize;

    while position < input.len() {
        while boundaries
            .get(boundary_index)
            .is_some_and(|&boundary| boundary < position)
        {
            boundary_index += 1;
        }
        if boundaries.get(boundary_index).copied() == Some(position) {
            frequencies.record_top(usize::from(input[position]))?;
            parser.skip_boundary(
                input,
                position,
                MatchCost {
                    scoring: MatchScoring::Heuristic,
                    recent_offsets: &recent_offsets,
                },
            )?;
            position += 1;
            boundary_index += 1;
            continue;
        }
        if let Some(reference) = references.get(reference_index)
            && reference.position == position
        {
            let mut value = reference
                .length
                .checked_sub(usize::from(settings.big_min_match))
                .ok_or_else(|| {
                    DzipError::InvalidDz("short COMBUF reference during DZ analysis".to_string())
                })?;
            let continuation = 1usize << (settings.ref_length_table_size - 1);
            let payload_mask = continuation - 1;
            let payload = value & payload_mask;
            value >>= settings.ref_length_table_size - 1;
            let first_group = payload | if value != 0 { continuation } else { 0 };
            frequencies.record_top(514 + first_group)?;
            parser.skip_range(input, position, position + reference.length);
            position += reference.length;
            reference_index += 1;
            continue;
        }

        let next_reference = references
            .get(reference_index)
            .map(|reference| reference.position)
            .unwrap_or(input.len());
        let next_boundary = boundaries
            .get(boundary_index)
            .copied()
            .unwrap_or(input.len());
        let next_event = usize::min(next_reference, next_boundary);
        let Some(decision) = parser.next_bounded(
            input,
            next_event,
            MatchCost {
                scoring: MatchScoring::Heuristic,
                recent_offsets: &recent_offsets,
            },
        )?
        else {
            position = next_event;
            continue;
        };
        match decision {
            LzDecision::Literal {
                position: literal_position,
            } => {
                frequencies.record_top(usize::from(input[literal_position]))?;
                position = literal_position + 1;
            }
            LzDecision::Match { length, distance } => {
                frequencies.record_top(length + 254)?;
                let code = encode_recent_distance(distance, &mut recent_offsets);
                let context = usize::min(length - MIN_MATCH, frequencies.offsets.len() - 1);
                frequencies.record_grouped(context, settings.offset_table_size, code)?;
                position += length;
            }
        }
    }
    Ok(frequencies.into_costs())
}

pub(crate) fn compress_chunk_with_references(
    input: &[u8],
    settings: RangeSettings,
    has_combuf: bool,
    references: &[ResolvedReference],
    boundaries: &[usize],
) -> Result<Vec<u8>> {
    compress_chunk_with_references_and_output(
        input,
        settings,
        has_combuf,
        references,
        boundaries,
        Vec::new(),
    )
}

fn compress_chunk_with_references_and_output(
    input: &[u8],
    settings: RangeSettings,
    has_combuf: bool,
    references: &[ResolvedReference],
    boundaries: &[usize],
    output: Vec<u8>,
) -> Result<Vec<u8>> {
    let settings = settings.validate()?;
    let fixed_costs = analyze_dz_costs(input, settings, has_combuf, references, boundaries)?;
    let mut encoder = RangeEncoder::with_output(output);
    let mut models = DzModels::new(settings, has_combuf)?;
    let mut recent_offsets = [0usize; 4];
    let window = 1usize
        .checked_shl(u32::from(settings.win_size))
        .unwrap_or(usize::MAX);
    if references.is_empty() && boundaries.is_empty() {
        let mut parser = LazyLzParser::new(window);
        while let Some(decision) = parser.next(
            input,
            MatchCost {
                scoring: MatchScoring::Fixed {
                    costs: &fixed_costs,
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
        encoder.encode(&mut models.top, END_SYMBOL)?;
        return Ok(encoder.finish());
    }
    let mut parser = LazyLzParser::new(window);
    let mut position = 0usize;
    let mut reference_index = 0usize;
    let mut boundary_index = 0usize;

    while position < input.len() {
        while boundaries
            .get(boundary_index)
            .is_some_and(|&boundary| boundary < position)
        {
            boundary_index += 1;
        }
        if boundaries.get(boundary_index).copied() == Some(position) {
            encoder.encode(&mut models.top, usize::from(input[position]))?;
            parser.skip_boundary(
                input,
                position,
                MatchCost {
                    scoring: MatchScoring::Fixed {
                        costs: &fixed_costs,
                        dynamic_offsets: &models.offsets,
                    },
                    recent_offsets: &recent_offsets,
                },
            )?;
            position += 1;
            boundary_index += 1;
            continue;
        }
        if let Some(reference) = references.get(reference_index)
            && reference.position == position
        {
            encode_common_reference(&mut encoder, &mut models, settings, *reference)?;
            parser.skip_range(input, position, position + reference.length);
            position += reference.length;
            reference_index += 1;
            continue;
        }

        let next_reference = references
            .get(reference_index)
            .map(|reference| reference.position)
            .unwrap_or(input.len());
        let next_boundary = boundaries
            .get(boundary_index)
            .copied()
            .unwrap_or(input.len());
        let next_event = usize::min(next_reference, next_boundary);
        let Some(decision) = parser.next_bounded(
            input,
            next_event,
            MatchCost {
                scoring: MatchScoring::Fixed {
                    costs: &fixed_costs,
                    dynamic_offsets: &models.offsets,
                },
                recent_offsets: &recent_offsets,
            },
        )?
        else {
            position = next_event;
            continue;
        };
        match decision {
            LzDecision::Literal {
                position: literal_position,
            } => {
                encoder.encode(&mut models.top, usize::from(input[literal_position]))?;
                position = literal_position + 1;
            }
            LzDecision::Match { length, distance } => {
                encoder.encode(&mut models.top, length.saturating_add(254))?;
                let code = encode_recent_distance(distance, &mut recent_offsets);
                let context = usize::min(length - MIN_MATCH, models.offsets.len() - 1);
                encode_grouped(
                    &mut encoder,
                    &mut models.offsets[context],
                    settings.offset_table_size,
                    code,
                )?;
                position += length;
            }
        }
    }
    encoder.encode(&mut models.top, END_SYMBOL)?;
    Ok(encoder.finish())
}

fn encode_common_reference(
    encoder: &mut RangeEncoder,
    models: &mut DzModels,
    settings: RangeSettings,
    reference: ResolvedReference,
) -> Result<()> {
    let minimum_match = usize::from(settings.big_min_match);
    let value = reference.length.checked_sub(minimum_match).ok_or_else(|| {
        DzipError::InvalidDz(format!(
            "COMBUF reference length {} is shorter than BigMinMatch {}",
            reference.length, minimum_match
        ))
    })?;
    encode_grouped_with_first_in_top(
        encoder,
        &mut models.top,
        &mut models.reference_lengths,
        settings.ref_length_table_size,
        value,
    )?;

    let target = i64::try_from(reference.target)
        .map_err(|_| DzipError::InvalidDz("COMBUF target overflow".to_string()))?;
    let (base_index, delta) = models
        .reference_bases
        .iter()
        .enumerate()
        .filter_map(|(index, &base)| target.checked_sub(base).map(|delta| (index, delta)))
        .min_by_key(|(_, delta)| delta.unsigned_abs())
        .ok_or_else(|| DzipError::InvalidDz("COMBUF target overflow".to_string()))?;
    let delta = delta
        .checked_add(3)
        .ok_or_else(|| DzipError::InvalidDz("COMBUF delta overflow".to_string()))?;
    encoder.encode(&mut models.reference_base, base_index)?;
    let (negative, magnitude) = if delta < 0 {
        (
            1usize,
            usize::try_from(-1i64 - delta)
                .map_err(|_| DzipError::InvalidDz("COMBUF delta overflow".to_string()))?,
        )
    } else {
        (
            0usize,
            usize::try_from(delta)
                .map_err(|_| DzipError::InvalidDz("COMBUF delta overflow".to_string()))?,
        )
    };
    encoder.encode(&mut models.reference_sign, negative)?;
    encode_grouped(
        encoder,
        &mut models.reference_offsets,
        settings.ref_offset_table_size,
        magnitude,
    )?;
    models.reference_bases[base_index] = i64::try_from(reference.next_base)
        .map_err(|_| DzipError::InvalidDz("COMBUF reference endpoint overflow".to_string()))?;
    Ok(())
}

fn encode_grouped_with_first_in_top(
    encoder: &mut RangeEncoder,
    top: &mut AdaptiveModel,
    continuation_models: &mut [AdaptiveModel],
    bits: u8,
    mut value: usize,
) -> Result<()> {
    let continuation = 1usize << (bits - 1);
    let payload_mask = continuation - 1;
    let payload = value & payload_mask;
    value >>= bits - 1;
    let first_group = payload | if value != 0 { continuation } else { 0 };
    encoder.encode(top, 514 + first_group)?;
    if value == 0 {
        return Ok(());
    }
    if continuation_models.is_empty() {
        return Err(DzipError::InvalidDz(
            "continued COMBUF length has no model".to_string(),
        ));
    }
    let mut table = 0usize;
    loop {
        let payload = value & payload_mask;
        value >>= bits - 1;
        let group = payload | if value != 0 { continuation } else { 0 };
        encoder.encode(&mut continuation_models[table], group)?;
        if value == 0 {
            return Ok(());
        }
        table = usize::min(table + 1, continuation_models.len() - 1);
    }
}

fn decode_grouped(
    decoder: &mut RangeDecoder<'_>,
    models: &mut [AdaptiveModel],
    bits: u8,
) -> Result<usize> {
    let continuation = 1usize << (bits - 1);
    let payload_mask = continuation - 1;
    let mut value = 0usize;
    let mut shift = 0u32;
    let mut table = 0usize;
    loop {
        let group = decoder.decode(&mut models[table])?;
        value |= (group & payload_mask)
            .checked_shl(shift)
            .ok_or_else(|| DzipError::InvalidDz("grouped integer overflow".to_string()))?;
        if group & continuation == 0 {
            return Ok(value);
        }
        shift = shift
            .checked_add(u32::from(bits - 1))
            .ok_or_else(|| DzipError::InvalidDz("grouped integer overflow".to_string()))?;
        table = usize::min(table + 1, models.len() - 1);
        if shift >= usize::BITS {
            return Err(DzipError::InvalidDz(
                "grouped integer is too large".to_string(),
            ));
        }
    }
}

pub(crate) fn encode_grouped(
    encoder: &mut RangeEncoder,
    models: &mut [AdaptiveModel],
    bits: u8,
    mut value: usize,
) -> Result<()> {
    let continuation = 1usize << (bits - 1);
    let payload_mask = continuation - 1;
    let mut table = 0usize;
    loop {
        let payload = value & payload_mask;
        value >>= bits - 1;
        let group = payload | if value != 0 { continuation } else { 0 };
        encoder.encode(&mut models[table], group)?;
        if value == 0 {
            return Ok(());
        }
        table = usize::min(table + 1, models.len() - 1);
    }
}

fn decode_recent_distance(code: usize, recent: &mut [usize; 4]) -> Result<usize> {
    if code < recent.len() {
        let distance = recent[code];
        recent.swap(0, code);
        if distance == 0 {
            return Err(DzipError::InvalidDz(
                "DZ stream used an uninitialized recent distance".to_string(),
            ));
        }
        Ok(distance)
    } else {
        let distance = code - 3;
        recent.copy_within(0..3, 1);
        recent[0] = distance;
        Ok(distance)
    }
}

pub(crate) fn encode_recent_distance(distance: usize, recent: &mut [usize; 4]) -> usize {
    if let Some(index) = recent.iter().position(|&candidate| candidate == distance) {
        recent.swap(0, index);
        index
    } else {
        recent.copy_within(0..3, 1);
        recent[0] = distance;
        distance + 3
    }
}

fn copy_match(
    output: &mut Vec<u8>,
    distance: usize,
    length: usize,
    expected_size: usize,
) -> Result<()> {
    if distance == 0 || distance > output.len() {
        return Err(DzipError::InvalidDz(format!(
            "invalid LZ distance {} at output position {}",
            distance,
            output.len()
        )));
    }
    if output.len().saturating_add(length) > expected_size {
        return Err(DzipError::InvalidDz(format!(
            "LZ match exceeds expected output size {}",
            expected_size
        )));
    }
    for _ in 0..length {
        let value = output[output.len() - distance];
        output.push(value);
    }
    Ok(())
}
