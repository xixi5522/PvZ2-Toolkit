use alloc::vec;
use alloc::vec::Vec;

use crate::codecs::lzma::matchfinder::{
    find_match, insert_position as insert_match_position, match_length as match_length_at,
};
use crate::codecs::lzma::range::{RangeDecoder, RangeEncoder};
use crate::codecs::lzma::{Error, LzmaProps};

const PROB_TOTAL: u32 = 1 << 11;
const NUM_STATES: usize = 12;
const NUM_POS_STATES_MAX: usize = 16;
const NUM_LEN_TO_POS_STATES: usize = 4;
const MATCH_MIN_LEN: usize = 2;
const NUM_POS_SLOT_BITS: usize = 6;
const START_POS_MODEL_INDEX: u32 = 4;
const END_POS_MODEL_INDEX: u32 = 14;
const NUM_FULL_DISTANCES: usize = 1 << (END_POS_MODEL_INDEX as usize / 2);
const NUM_ALIGN_BITS: usize = 4;
const ALIGN_TABLE_SIZE: usize = 1 << NUM_ALIGN_BITS;

#[cfg(test)]
pub(crate) fn encode_checked(input: &[u8], props: &LzmaProps) -> Result<Vec<u8>, Error> {
    encode_checked_with_buffer(input, props, Vec::new())
}

pub(crate) fn encode_checked_with_buffer(
    input: &[u8],
    props: &LzmaProps,
    output: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let props = props.validate()?;
    let mut models = Models::new(props);
    let mut encoder = RangeEncoder::with_output(output);
    let mut state = 0usize;
    let mut previous = 0u8;
    let mut reps = [0u32; 4];
    let mut head = vec![usize::MAX; 1 << 16];
    let mut chain = vec![usize::MAX; input.len()];
    let mut position = 0usize;
    while position < input.len() {
        let pos_state = position & models.pos_state_mask;
        let (match_length, match_distance) = find_match(
            input,
            position,
            props.dict_size as usize,
            props.fb.clamp(5, 273) as usize,
            props.mc.max(1) as usize,
            &head,
            &chain,
        );
        let mut rep_index = 0usize;
        let mut rep_length = 0usize;
        for (index, &rep) in reps.iter().enumerate() {
            let distance = rep as usize + 1;
            if distance > position {
                continue;
            }
            let length = match_length_at(input, position, position - distance, 273);
            if length > rep_length {
                rep_length = length;
                rep_index = index;
            }
        }

        let use_rep = rep_length >= 2 && rep_length >= match_length
            || rep_index == 0 && rep_length == 1 && match_length < 3;
        let selected_length = if use_rep {
            rep_length.min(273)
        } else {
            match_length.min(273)
        };

        insert_match_position(input, position, &mut head, &mut chain);
        if use_rep {
            encoder.encode_bit(&mut models.is_match[state][pos_state], 1);
            encoder.encode_bit(&mut models.is_rep[state], 1);
            encode_rep_index(
                &mut encoder,
                &mut models,
                state,
                pos_state,
                rep_index,
                selected_length,
            );
            if selected_length == 1 {
                state = update_short_rep_state(state);
            } else {
                models
                    .rep_length
                    .encode(&mut encoder, pos_state, selected_length - MATCH_MIN_LEN);
                state = update_rep_state(state);
            }
            if rep_index != 0 {
                let distance = reps[rep_index];
                if rep_index == 3 {
                    reps[3] = reps[2];
                }
                if rep_index >= 2 {
                    reps[2] = reps[1];
                }
                reps[1] = reps[0];
                reps[0] = distance;
            }
            for next in position + 1..(position + selected_length).min(input.len()) {
                insert_match_position(input, next, &mut head, &mut chain);
            }
            position += selected_length;
            previous = input[position - 1];
        } else if selected_length >= 3 {
            encoder.encode_bit(&mut models.is_match[state][pos_state], 1);
            encoder.encode_bit(&mut models.is_rep[state], 0);
            models
                .match_length
                .encode(&mut encoder, pos_state, selected_length - MATCH_MIN_LEN);
            let zero_based_distance = (match_distance - 1) as u32;
            encode_distance(
                &mut encoder,
                &mut models,
                selected_length,
                zero_based_distance,
            )?;
            reps[3] = reps[2];
            reps[2] = reps[1];
            reps[1] = reps[0];
            reps[0] = zero_based_distance;
            state = update_match_state(state);
            for next in position + 1..(position + selected_length).min(input.len()) {
                insert_match_position(input, next, &mut head, &mut chain);
            }
            position += selected_length;
            previous = input[position - 1];
        } else {
            encoder.encode_bit(&mut models.is_match[state][pos_state], 0);
            let context = models.literal_context(position, previous);
            let literal_models = &mut models.literal[context..context + 0x300];
            let byte = input[position];
            if state < 7 || reps[0] as usize + 1 > position {
                encode_literal(&mut encoder, literal_models, byte);
            } else {
                let match_byte = input[position - reps[0] as usize - 1];
                encode_matched_literal(&mut encoder, literal_models, byte, match_byte);
            }
            state = update_literal_state(state);
            previous = byte;
            position += 1;
        }
    }

    // A length-two match with distance 0xffff_ffff is the LZMA end marker.
    let pos_state = input.len() & models.pos_state_mask;
    encoder.encode_bit(&mut models.is_match[state][pos_state], 1);
    encoder.encode_bit(&mut models.is_rep[state], 0);
    models.match_length.encode(&mut encoder, pos_state, 0);
    encode_bit_tree(&mut encoder, &mut models.pos_slot[0], 63, NUM_POS_SLOT_BITS);
    encoder.encode_direct_bits((1 << 26) - 1, 26);
    encode_reverse_bit_tree(&mut encoder, &mut models.pos_align, 15, NUM_ALIGN_BITS);
    Ok(encoder.finish())
}

fn encode_rep_index(
    encoder: &mut RangeEncoder,
    models: &mut Models,
    state: usize,
    pos_state: usize,
    index: usize,
    length: usize,
) {
    if index == 0 {
        encoder.encode_bit(&mut models.is_rep_g0[state], 0);
        encoder.encode_bit(
            &mut models.is_rep0_long[state][pos_state],
            if length != 1 { 1 } else { 0 },
        );
    } else {
        encoder.encode_bit(&mut models.is_rep_g0[state], 1);
        if index == 1 {
            encoder.encode_bit(&mut models.is_rep_g1[state], 0);
        } else {
            encoder.encode_bit(&mut models.is_rep_g1[state], 1);
            encoder.encode_bit(&mut models.is_rep_g2[state], u32::from(index == 3));
        }
    }
}

fn encode_distance(
    encoder: &mut RangeEncoder,
    models: &mut Models,
    length: usize,
    distance: u32,
) -> Result<(), Error> {
    let len_state = (length - MATCH_MIN_LEN).min(NUM_LEN_TO_POS_STATES - 1);
    let slot = if distance < 4 {
        distance
    } else {
        let highest = 31 - distance.leading_zeros();
        (highest << 1) + (distance >> (highest - 1) & 1)
    };
    encode_bit_tree(
        encoder,
        &mut models.pos_slot[len_state],
        slot,
        NUM_POS_SLOT_BITS,
    );
    if slot >= START_POS_MODEL_INDEX {
        let direct_bits = (slot >> 1) - 1;
        let base = (2 | (slot & 1)) << direct_bits;
        let footer = distance - base;
        if slot < END_POS_MODEL_INDEX {
            // The SDK passes `pos_decoders + base - slot - 1` to a bit-tree
            // whose first lookup is model 1.  Store the absolute root index
            // instead so slot 4 maps to index 0 without unsigned underflow.
            let root = usize::try_from(
                base.checked_sub(slot)
                    .ok_or_else(|| Error::new("invalid LZMA distance model"))?,
            )
            .map_err(|_| Error::new("invalid LZMA distance model"))?;
            encode_reverse_bit_tree_at(
                encoder,
                &mut models.pos_decoders,
                root,
                footer,
                direct_bits as usize,
            )?;
        } else {
            encoder.encode_direct_bits(
                footer >> NUM_ALIGN_BITS,
                direct_bits - NUM_ALIGN_BITS as u32,
            );
            encode_reverse_bit_tree(
                encoder,
                &mut models.pos_align,
                footer & (ALIGN_TABLE_SIZE as u32 - 1),
                NUM_ALIGN_BITS,
            );
        }
    }
    Ok(())
}

/// Decode a raw LZMA1 stream using its five-byte properties field.
#[cfg(test)]
pub(crate) fn decode_raw(
    input: &[u8],
    properties: &[u8; 5],
    out_len: usize,
) -> Result<Vec<u8>, Error> {
    decode_raw_with_buffer(input, properties, out_len, Vec::new())
}

pub(crate) fn decode_raw_with_buffer(
    input: &[u8],
    properties: &[u8; 5],
    out_len: usize,
    mut output: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let props = parse_decoder_props(properties)?;
    let mut models = Models::new(props);
    let mut decoder = RangeDecoder::new(input)?;
    output.clear();
    if output.capacity() < out_len {
        output.reserve(out_len);
    }
    let mut state = 0usize;
    let mut reps = [0u32; 4];
    let mut previous = 0u8;

    while output.len() < out_len {
        let pos_state = output.len() & models.pos_state_mask;
        if decoder.decode_bit(&mut models.is_match[state][pos_state])? == 0 {
            let context = models.literal_context(output.len(), previous);
            let literal_models = &mut models.literal[context..context + 0x300];
            let byte = if state < 7 {
                decode_literal(&mut decoder, literal_models)?
            } else {
                let distance = usize::try_from(reps[0])
                    .map_err(|_| Error::new("LZMA distance does not fit usize"))?
                    .saturating_add(1);
                if distance > output.len() || distance > models.props.dict_size as usize {
                    return Err(Error::new("LZMA matched literal precedes output"));
                }
                let match_byte = output[output.len() - distance];
                decode_matched_literal(&mut decoder, literal_models, match_byte)?
            };
            output.push(byte);
            previous = byte;
            state = update_literal_state(state);
            continue;
        }

        let length;
        if decoder.decode_bit(&mut models.is_rep[state])? != 0 {
            if decoder.decode_bit(&mut models.is_rep_g0[state])? == 0 {
                if decoder.decode_bit(&mut models.is_rep0_long[state][pos_state])? == 0 {
                    state = update_short_rep_state(state);
                    copy_match(
                        &mut output,
                        reps[0],
                        1,
                        out_len,
                        models.props.dict_size as usize,
                    )?;
                    previous = *output.last().expect("a short rep produces one byte");
                    continue;
                }
            } else {
                let distance;
                if decoder.decode_bit(&mut models.is_rep_g1[state])? == 0 {
                    distance = reps[1];
                } else {
                    if decoder.decode_bit(&mut models.is_rep_g2[state])? == 0 {
                        distance = reps[2];
                    } else {
                        distance = reps[3];
                        reps[3] = reps[2];
                    }
                    reps[2] = reps[1];
                }
                reps[1] = reps[0];
                reps[0] = distance;
            }
            length = models.rep_length.decode(&mut decoder, pos_state)? + MATCH_MIN_LEN;
            state = update_rep_state(state);
        } else {
            reps[3] = reps[2];
            reps[2] = reps[1];
            reps[1] = reps[0];
            length = models.match_length.decode(&mut decoder, pos_state)? + MATCH_MIN_LEN;
            state = update_match_state(state);
            let len_state = (length - MATCH_MIN_LEN).min(NUM_LEN_TO_POS_STATES - 1);
            let slot = decode_bit_tree(
                &mut decoder,
                &mut models.pos_slot[len_state],
                NUM_POS_SLOT_BITS,
            )?;
            reps[0] = if slot < START_POS_MODEL_INDEX {
                slot
            } else {
                let direct_bits = (slot >> 1) - 1;
                let mut distance = (2 | (slot & 1)) << direct_bits;
                if slot < END_POS_MODEL_INDEX {
                    let root = usize::try_from(
                        distance
                            .checked_sub(slot)
                            .ok_or_else(|| Error::new("invalid LZMA distance model"))?,
                    )
                    .map_err(|_| Error::new("invalid LZMA distance model"))?;
                    distance += decode_reverse_bit_tree_at(
                        &mut decoder,
                        &mut models.pos_decoders,
                        root,
                        direct_bits as usize,
                    )?;
                } else {
                    distance += decoder.decode_direct_bits(direct_bits - NUM_ALIGN_BITS as u32)?
                        << NUM_ALIGN_BITS;
                    distance += decode_reverse_bit_tree(
                        &mut decoder,
                        &mut models.pos_align,
                        NUM_ALIGN_BITS,
                    )?;
                }
                distance
            };
            if reps[0] == u32::MAX {
                return Err(Error::new(
                    "LZMA end marker precedes declared output length",
                ));
            }
        }
        copy_match(
            &mut output,
            reps[0],
            length,
            out_len,
            models.props.dict_size as usize,
        )?;
        previous = *output.last().expect("a match produces at least two bytes");
    }
    Ok(output)
}

fn parse_decoder_props(properties: &[u8; 5]) -> Result<LzmaProps, Error> {
    let mut property = u32::from(properties[0]);
    if property >= 9 * 5 * 5 {
        return Err(Error::new("invalid LZMA properties byte"));
    }
    let lc = property % 9;
    property /= 9;
    let lp = property % 5;
    let pb = property / 5;
    LzmaProps {
        lc,
        lp,
        pb,
        dict_size: u32::from_le_bytes(properties[1..].try_into().unwrap()),
        fb: 0,
        mc: 0,
    }
    .validate()
}

fn copy_match(
    output: &mut Vec<u8>,
    zero_based_distance: u32,
    length: usize,
    limit: usize,
    dictionary_size: usize,
) -> Result<(), Error> {
    let distance = usize::try_from(zero_based_distance)
        .map_err(|_| Error::new("LZMA distance does not fit usize"))?
        .checked_add(1)
        .ok_or_else(|| Error::new("LZMA distance overflow"))?;
    if distance > output.len() || distance > dictionary_size {
        return Err(Error::new("LZMA match precedes output window"));
    }
    if output.len().saturating_add(length) > limit {
        return Err(Error::new("LZMA output exceeds declared length"));
    }
    for _ in 0..length {
        output.push(output[output.len() - distance]);
    }
    Ok(())
}

fn update_literal_state(state: usize) -> usize {
    if state < 4 {
        0
    } else if state < 10 {
        state - 3
    } else {
        state - 6
    }
}

fn update_match_state(state: usize) -> usize {
    if state < 7 { 7 } else { 10 }
}

fn update_rep_state(state: usize) -> usize {
    if state < 7 { 8 } else { 11 }
}

fn update_short_rep_state(state: usize) -> usize {
    if state < 7 { 9 } else { 11 }
}

fn new_probs(length: usize) -> Vec<u16> {
    vec![(PROB_TOTAL / 2) as u16; length]
}

struct Models {
    props: LzmaProps,
    pos_state_mask: usize,
    literal: Vec<u16>,
    is_match: [[u16; NUM_POS_STATES_MAX]; NUM_STATES],
    is_rep: [u16; NUM_STATES],
    is_rep_g0: [u16; NUM_STATES],
    is_rep_g1: [u16; NUM_STATES],
    is_rep_g2: [u16; NUM_STATES],
    is_rep0_long: [[u16; NUM_POS_STATES_MAX]; NUM_STATES],
    pos_slot: [[u16; 1 << NUM_POS_SLOT_BITS]; NUM_LEN_TO_POS_STATES],
    pos_decoders: Vec<u16>,
    pos_align: [u16; ALIGN_TABLE_SIZE],
    match_length: LengthModel,
    rep_length: LengthModel,
}

impl Models {
    fn new(props: LzmaProps) -> Self {
        let initial = (PROB_TOTAL / 2) as u16;
        Self {
            props,
            pos_state_mask: (1usize << props.pb) - 1,
            literal: new_probs(0x300usize << (props.lc + props.lp)),
            is_match: [[initial; NUM_POS_STATES_MAX]; NUM_STATES],
            is_rep: [initial; NUM_STATES],
            is_rep_g0: [initial; NUM_STATES],
            is_rep_g1: [initial; NUM_STATES],
            is_rep_g2: [initial; NUM_STATES],
            is_rep0_long: [[initial; NUM_POS_STATES_MAX]; NUM_STATES],
            pos_slot: [[initial; 1 << NUM_POS_SLOT_BITS]; NUM_LEN_TO_POS_STATES],
            pos_decoders: new_probs(NUM_FULL_DISTANCES - END_POS_MODEL_INDEX as usize),
            pos_align: [initial; ALIGN_TABLE_SIZE],
            match_length: LengthModel::new(),
            rep_length: LengthModel::new(),
        }
    }

    fn literal_context(&self, position: usize, previous: u8) -> usize {
        let low = (position & ((1usize << self.props.lp) - 1)) << self.props.lc;
        let high = usize::from(previous) >> (8 - self.props.lc);
        (low + high) * 0x300
    }
}

struct LengthModel {
    choice: u16,
    choice2: u16,
    low: [[u16; 8]; NUM_POS_STATES_MAX],
    mid: [[u16; 8]; NUM_POS_STATES_MAX],
    high: [u16; 256],
}

impl LengthModel {
    fn new() -> Self {
        let initial = (PROB_TOTAL / 2) as u16;
        Self {
            choice: initial,
            choice2: initial,
            low: [[initial; 8]; NUM_POS_STATES_MAX],
            mid: [[initial; 8]; NUM_POS_STATES_MAX],
            high: [initial; 256],
        }
    }

    fn decode(&mut self, decoder: &mut RangeDecoder<'_>, pos_state: usize) -> Result<usize, Error> {
        if decoder.decode_bit(&mut self.choice)? == 0 {
            return Ok(decode_bit_tree(decoder, &mut self.low[pos_state], 3)? as usize);
        }
        if decoder.decode_bit(&mut self.choice2)? == 0 {
            return Ok(8 + decode_bit_tree(decoder, &mut self.mid[pos_state], 3)? as usize);
        }
        Ok(16 + decode_bit_tree(decoder, &mut self.high, 8)? as usize)
    }

    fn encode(&mut self, encoder: &mut RangeEncoder, pos_state: usize, value: usize) {
        if value < 8 {
            encoder.encode_bit(&mut self.choice, 0);
            encode_bit_tree(encoder, &mut self.low[pos_state], value as u32, 3);
        } else if value < 16 {
            encoder.encode_bit(&mut self.choice, 1);
            encoder.encode_bit(&mut self.choice2, 0);
            encode_bit_tree(encoder, &mut self.mid[pos_state], value as u32 - 8, 3);
        } else {
            encoder.encode_bit(&mut self.choice, 1);
            encoder.encode_bit(&mut self.choice2, 1);
            encode_bit_tree(encoder, &mut self.high, value as u32 - 16, 8);
        }
    }
}

fn decode_literal(decoder: &mut RangeDecoder<'_>, probs: &mut [u16]) -> Result<u8, Error> {
    let mut symbol = 1usize;
    while symbol < 0x100 {
        symbol = symbol << 1 | decoder.decode_bit(&mut probs[symbol])? as usize;
    }
    Ok(symbol as u8)
}

fn decode_matched_literal(
    decoder: &mut RangeDecoder<'_>,
    probs: &mut [u16],
    mut match_byte: u8,
) -> Result<u8, Error> {
    let mut symbol = 1usize;
    loop {
        let match_bit = usize::from(match_byte >> 7 & 1);
        match_byte <<= 1;
        let bit = decoder.decode_bit(&mut probs[((1 + match_bit) << 8) + symbol])? as usize;
        symbol = symbol << 1 | bit;
        if match_bit != bit || symbol >= 0x100 {
            break;
        }
    }
    while symbol < 0x100 {
        symbol = symbol << 1 | decoder.decode_bit(&mut probs[symbol])? as usize;
    }
    Ok(symbol as u8)
}

fn encode_literal(encoder: &mut RangeEncoder, probs: &mut [u16], byte: u8) {
    let mut symbol = 1usize;
    for bit_index in (0..8).rev() {
        let bit = u32::from(byte >> bit_index & 1);
        encoder.encode_bit(&mut probs[symbol], bit);
        symbol = symbol << 1 | bit as usize;
    }
}

fn encode_matched_literal(
    encoder: &mut RangeEncoder,
    probs: &mut [u16],
    byte: u8,
    mut match_byte: u8,
) {
    let mut symbol = 1usize;
    for bit_index in (0..8).rev() {
        let match_bit = usize::from(match_byte >> 7 & 1);
        match_byte <<= 1;
        let bit = u32::from(byte >> bit_index & 1);
        encoder.encode_bit(&mut probs[((1 + match_bit) << 8) + symbol], bit);
        symbol = symbol << 1 | bit as usize;
        if match_bit != bit as usize {
            for remaining in (0..bit_index).rev() {
                let next = u32::from(byte >> remaining & 1);
                encoder.encode_bit(&mut probs[symbol], next);
                symbol = symbol << 1 | next as usize;
            }
            break;
        }
    }
}

fn decode_bit_tree(
    decoder: &mut RangeDecoder<'_>,
    probs: &mut [u16],
    bits: usize,
) -> Result<u32, Error> {
    let mut symbol = 1usize;
    for _ in 0..bits {
        symbol = symbol << 1 | decoder.decode_bit(&mut probs[symbol])? as usize;
    }
    Ok((symbol - (1 << bits)) as u32)
}

fn encode_bit_tree(encoder: &mut RangeEncoder, probs: &mut [u16], symbol: u32, bits: usize) {
    let mut model = 1usize;
    for bit_index in (0..bits).rev() {
        let bit = symbol >> bit_index & 1;
        encoder.encode_bit(&mut probs[model], bit);
        model = model << 1 | bit as usize;
    }
}

fn decode_reverse_bit_tree(
    decoder: &mut RangeDecoder<'_>,
    probs: &mut [u16],
    bits: usize,
) -> Result<u32, Error> {
    decode_reverse_bit_tree_at(decoder, probs, 1, bits)
}

fn decode_reverse_bit_tree_at(
    decoder: &mut RangeDecoder<'_>,
    probs: &mut [u16],
    root: usize,
    bits: usize,
) -> Result<u32, Error> {
    let mut model = 1usize;
    let mut symbol = 0u32;
    for bit_index in 0..bits {
        let index = root
            .checked_add(model - 1)
            .filter(|&index| index < probs.len())
            .ok_or_else(|| Error::new("invalid LZMA distance probability index"))?;
        let bit = decoder.decode_bit(&mut probs[index])?;
        model = model << 1 | bit as usize;
        symbol |= bit << bit_index;
    }
    Ok(symbol)
}

fn encode_reverse_bit_tree(
    encoder: &mut RangeEncoder,
    probs: &mut [u16],
    symbol: u32,
    bits: usize,
) {
    let mut model = 1usize;
    for bit_index in 0..bits {
        let bit = symbol >> bit_index & 1;
        encoder.encode_bit(&mut probs[model], bit);
        model = model << 1 | bit as usize;
    }
}

fn encode_reverse_bit_tree_at(
    encoder: &mut RangeEncoder,
    probs: &mut [u16],
    root: usize,
    symbol: u32,
    bits: usize,
) -> Result<(), Error> {
    let mut model = 1usize;
    for bit_index in 0..bits {
        let index = root
            .checked_add(model - 1)
            .filter(|&index| index < probs.len())
            .ok_or_else(|| Error::new("invalid LZMA distance probability index"))?;
        let bit = symbol >> bit_index & 1;
        encoder.encode_bit(&mut probs[index], bit);
        model = model << 1 | bit as usize;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    fn deterministic_bytes(length: usize, mut state: u32) -> Vec<u8> {
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect()
    }

    fn fixtures() -> Vec<Vec<u8>> {
        vec![
            Vec::new(),
            vec![0],
            b"literal-only streams are valid LZMA".repeat(100),
            (0..=255).cycle().take(65_537).collect(),
        ]
    }

    #[test]
    fn round_trip_boundaries() {
        let props = LzmaProps::default();
        let encoded_props = props.decoder_properties().unwrap();
        for input in fixtures() {
            let encoded = encode_checked(&input, &props).unwrap();
            assert_eq!(
                decode_raw(&encoded, &encoded_props, input.len()).unwrap(),
                input
            );
        }
    }

    #[test]
    fn large_deterministic_round_trip_matrix() {
        const SIZES: [usize; 35] = [
            0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 257, 258, 259, 511,
            512, 1023, 4095, 8191, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536,
            65_537,
        ];
        let props = LzmaProps::default();
        let encoded_props = props.decoder_properties().unwrap();
        let mut cases = 0usize;
        for &size in &SIZES {
            let inputs = [
                vec![0; size],
                (0..size).map(|index| index as u8).collect(),
                (0..size)
                    .map(|index| b"lzma-boundary-pattern"[index % 21])
                    .collect(),
                deterministic_bytes(size, 0x85eb_ca6b ^ size as u32),
            ];
            for input in inputs {
                let encoded = encode_checked(&input, &props).unwrap();
                assert_eq!(
                    decode_raw(&encoded, &encoded_props, input.len()).unwrap(),
                    input
                );
                cases += 1;
            }
        }
        assert_eq!(cases, 140);

        let input = deterministic_bytes(8193, 0x27d4_eb2f);
        for props in [
            LzmaProps {
                lc: 0,
                lp: 0,
                pb: 0,
                dict_size: 4096,
                fb: 16,
                mc: 8,
            },
            LzmaProps {
                lc: 2,
                lp: 1,
                pb: 3,
                dict_size: 32_768,
                fb: 64,
                mc: 64,
            },
            LzmaProps::default(),
        ] {
            let encoded = encode_checked(&input, &props).unwrap();
            assert_eq!(
                decode_raw(&encoded, &props.decoder_properties().unwrap(), input.len()).unwrap(),
                input
            );
        }
    }

    #[test]
    fn decodes_sdk_empty_stream() {
        let props = [0x5d, 0, 0, 1, 0];
        let stream = [0, 0x83, 0xff, 0xfb, 0xff, 0xff, 0xc0, 0, 0, 0];
        assert_eq!(decode_raw(&stream, &props, 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decodes_sdk_distance_slot_four_stream() {
        // LZMA SDK-compatible raw stream for `b"abcde".repeat(20)`. Its
        // first distance-five match uses position slot 4, whose compact
        // distance tree begins at probability index zero.
        let props = [0x5d, 0, 0, 1, 0];
        let stream = [
            0x00, 0x30, 0x98, 0x88, 0x98, 0x3e, 0xd5, 0x2d, 0x63, 0x04, 0x57, 0x7b, 0xff, 0xff,
            0x73, 0x08, 0x00, 0x00,
        ];
        assert_eq!(
            decode_raw(&stream, &props, 100).unwrap(),
            b"abcde".repeat(20)
        );
    }

    #[test]
    fn decodes_external_lzma_property_variants() {
        // Python 3 lzma.FORMAT_RAW / LZMA1 output.
        let default_props = [0x5d, 0x00, 0x00, 0x01, 0x00];
        let default_stream = [
            0x00, 0x30, 0x98, 0x88, 0x98, 0x3e, 0xd5, 0xf7, 0xc3, 0x5c, 0x87, 0xb5, 0x7d, 0xff,
            0xff, 0xfd, 0xcc, 0x20, 0x00,
        ];
        assert_eq!(
            decode_raw(&default_stream, &default_props, 500).unwrap(),
            b"abcde".repeat(100)
        );
        // The SDK treats dictionary fields 0..4095 as a 4-KiB dictionary.
        // This stream only needs distance five, so changing the field to zero
        // must not make an otherwise valid stream undecodable.
        let zero_dictionary_props = [0x5d, 0, 0, 0, 0];
        assert_eq!(
            decode_raw(&default_stream, &zero_dictionary_props, 500).unwrap(),
            b"abcde".repeat(100)
        );

        let zero_context_props = [0x00, 0x00, 0x10, 0x00, 0x00];
        let zero_context_stream = [
            0x00, 0x38, 0x1e, 0x0f, 0xaf, 0x5b, 0xb1, 0xa4, 0x20, 0x59, 0xae, 0xde, 0xbf, 0xdf,
            0x51, 0xab, 0x6c, 0x16, 0x11, 0x7f, 0x38, 0x3f, 0x49, 0xdf, 0xfe, 0x25, 0x54, 0x00,
        ];
        assert_eq!(
            decode_raw(&zero_context_stream, &zero_context_props, 560).unwrap(),
            b"property-zero ".repeat(40)
        );

        let mixed_props = [0x92, 0x00, 0x80, 0x00, 0x00];
        let mixed_stream = [
            0x00, 0x00, 0x00, 0x40, 0x4e, 0xa2, 0xfb, 0x50, 0x95, 0x53, 0x8c, 0x18, 0x1b, 0xb4,
            0x74, 0x2c, 0x59, 0xe2, 0x2c, 0xbf, 0xa9, 0x84, 0xdc, 0xb6, 0xdb, 0x1c, 0x39, 0x16,
            0xab, 0xfe, 0x73, 0xf1, 0xdd, 0x7b, 0x6b, 0x60, 0x65, 0xf5, 0x57, 0x2a, 0x67, 0x2f,
            0xef, 0x68, 0x2f, 0xfa, 0xb4, 0xa5, 0x05, 0x6e, 0x29, 0x73, 0x9c, 0xe8, 0x0c, 0xd5,
            0x79, 0xc7, 0x58, 0x54, 0xce, 0x0e, 0xa4, 0x3e, 0x8d, 0x93, 0x59, 0x7e, 0xc7, 0x9b,
            0x4c, 0x8c, 0xcb, 0xff, 0xff, 0xaf, 0x72, 0x00, 0x00,
        ];
        assert_eq!(
            decode_raw(&mixed_stream, &mixed_props, 512).unwrap(),
            (0..64).cycle().take(512).collect::<Vec<u8>>()
        );
    }

    #[test]
    fn truncations_and_bit_flips_are_bounded() {
        let input = deterministic_bytes(1025, 0x1234_5678);
        let props = LzmaProps::default();
        let encoded_props = props.decoder_properties().unwrap();
        let encoded = encode_checked(&input, &props).unwrap();

        let mut rejected_truncations = 0usize;
        for end in 0..encoded.len() {
            match decode_raw(&encoded[..end], &encoded_props, input.len()) {
                Err(_) => rejected_truncations += 1,
                Ok(decoded) => assert_eq!(decoded, input),
            }
        }
        assert!(rejected_truncations > encoded.len() / 2);

        let mut affected = 0usize;
        for index in 0..encoded.len() {
            for bit in 0..8 {
                let mut damaged = encoded.clone();
                damaged[index] ^= 1 << bit;
                match decode_raw(&damaged, &encoded_props, input.len()) {
                    Err(_) => affected += 1,
                    Ok(decoded) if decoded != input => affected += 1,
                    Ok(_) => {}
                }
            }
        }
        assert!(affected > encoded.len() * 4);
    }

    #[test]
    fn rejects_properties_and_truncation() {
        let props = LzmaProps::default();
        let mut encoded = encode_checked(b"damaged", &props).unwrap();
        encoded.truncate(4);
        assert!(decode_raw(&encoded, &props.decoder_properties().unwrap(), 7).is_err());
        assert!(decode_raw(&[0; 5], &[u8::MAX, 0, 0, 0, 0], 0).is_err());
    }
}
