use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::codecs::bzip::Error;
use crate::codecs::bzip::bitstream::{BitReader, BitWriter};
use crate::codecs::bzip::checksum::crc32;
use crate::codecs::bzip::options::decode_workspace;
use crate::codecs::bzip::transform::{
    bwt, decode_rle1, derandomize, encode_rle1, inverse_bwt, mtf_rle2,
};

const BLOCK_MAGIC: u64 = 0x3141_5926_5359;
const STREAM_END_MAGIC: u64 = 0x1772_4538_5090;
const GROUP_SIZE: usize = 50;
const MAX_CODE_LENGTH: usize = 20;
const BLOCK_OVERSHOOT_GUARD: usize = 19;
const RUNA: usize = 0;
const RUNB: usize = 1;

/// Encode a standard BZip2 stream using block size 100 KiB.
#[cfg(test)]
pub(crate) fn encode(input: &[u8]) -> Result<Vec<u8>, Error> {
    encode_with_block_size(input, 1)
}

/// Encode a BZip2 stream with block size `1..=9` times 100 KiB.
#[cfg(test)]
pub(crate) fn encode_with_block_size(input: &[u8], block_size: u8) -> Result<Vec<u8>, Error> {
    encode_with_buffer(input, block_size, Vec::new())
}

pub(crate) fn encode_with_buffer(
    input: &[u8],
    block_size: u8,
    output: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    if !(1..=9).contains(&block_size) {
        return Err(Error::new("BZip2 block size must be in 1..=9"));
    }
    let mut writer = BitWriter::with_output(output);
    writer.write_bits(u64::from(b'B'), 8);
    writer.write_bits(u64::from(b'Z'), 8);
    writer.write_bits(u64::from(b'h'), 8);
    writer.write_bits(u64::from(b'0' + block_size), 8);

    let block_capacity = usize::from(block_size) * 100_000;
    // BZip2's block limit applies after the first run-length transform, not
    // to the caller's input.  The reference encoder keeps a 19-byte guard so
    // a pending RLE1 run can never overshoot the declared block capacity.
    let rle1_limit = block_capacity - BLOCK_OVERSHOOT_GUARD;
    let mut combined_crc = 0u32;
    let mut block_start = 0usize;
    while block_start < input.len() {
        let block_end = rle1_block_end(input, block_start, rle1_limit);
        let block = &input[block_start..block_end];
        let block_crc = crc32(block);
        combined_crc = combined_crc.rotate_left(1) ^ block_crc;
        encode_block(&mut writer, block, block_crc)?;
        block_start = block_end;
    }
    writer.write_bits(STREAM_END_MAGIC, 48);
    writer.write_bits(u64::from(combined_crc), 32);
    Ok(writer.finish())
}

fn rle1_block_end(input: &[u8], start: usize, encoded_limit: usize) -> usize {
    let mut position = start;
    let mut encoded_length = 0usize;
    while position < input.len() {
        let byte = input[position];
        let mut run = 1usize;
        while position + run < input.len() && input[position + run] == byte && run < 259 {
            run += 1;
        }
        let encoded_run_length = if run < 4 { run } else { 5 };
        if encoded_length + encoded_run_length > encoded_limit {
            break;
        }
        encoded_length += encoded_run_length;
        position += run;
    }
    debug_assert!(
        position > start,
        "a BZip2 block can always hold one RLE1 run"
    );
    position
}

/// Decode a BZip2 stream to exactly `expected_length` bytes.
#[cfg(test)]
pub(crate) fn decode(input: &[u8], expected_length: usize) -> Result<Vec<u8>, Error> {
    decode_with_buffer(input, expected_length, Vec::new(), usize::MAX)
}

pub(crate) fn decode_with_buffer(
    input: &[u8],
    expected_length: usize,
    mut output: Vec<u8>,
    max_workspace_size: usize,
) -> Result<Vec<u8>, Error> {
    if input.len() < 4 {
        return Err(Error::new("invalid BZip2 stream header"));
    }
    output.clear();
    if output.capacity() < expected_length {
        output.reserve(expected_length);
    }

    let mut stream_offset = 0usize;
    while stream_offset < input.len() {
        let stream = &input[stream_offset..];
        if stream.len() < 4 || &stream[..3] != b"BZh" || !(b'1'..=b'9').contains(&stream[3]) {
            return Err(Error::new("invalid BZip2 stream header"));
        }
        let block_size = stream[3] - b'0';
        let workspace = decode_workspace(block_size)
            .ok_or_else(|| Error::workspace_limit("BZip2 workspace estimate overflow"))?;
        if workspace > max_workspace_size {
            return Err(Error::workspace_limit(
                "BZip2 workspace exceeds configured limit",
            ));
        }
        let block_capacity = usize::from(block_size) * 100_000;
        let mut reader = BitReader::new(&stream[4..]);
        let mut combined_crc = 0u32;
        loop {
            let marker = reader.read_bits(48)?;
            if marker == STREAM_END_MAGIC {
                let declared = reader.read_bits(32)? as u32;
                if declared != combined_crc {
                    return Err(Error::new("BZip2 combined CRC mismatch"));
                }
                break;
            }
            if marker != BLOCK_MAGIC {
                return Err(Error::new("invalid BZip2 block marker"));
            }
            let declared_crc = reader.read_bits(32)? as u32;
            let randomized = reader.read_bits(1)? != 0;
            let original_pointer = reader.read_bits(24)? as usize;
            let encoded = decode_block_symbols(&mut reader, block_capacity)?;
            if encoded.is_empty() || original_pointer >= encoded.len() {
                return Err(Error::new("invalid BZip2 original pointer"));
            }
            let mut rle1 = inverse_bwt(&encoded, original_pointer)?;
            if randomized {
                derandomize(&mut rle1);
            }
            let block = decode_rle1(&rle1, expected_length.saturating_sub(output.len()))?;
            let actual_crc = crc32(&block);
            if actual_crc != declared_crc {
                return Err(Error::new("BZip2 block CRC mismatch"));
            }
            combined_crc = combined_crc.rotate_left(1) ^ actual_crc;
            if output.len().saturating_add(block.len()) > expected_length {
                return Err(Error::new("BZip2 output exceeds declared length"));
            }
            output.extend_from_slice(&block);
        }
        stream_offset += 4 + reader.consumed_bytes();
    }
    if output.len() != expected_length {
        return Err(Error::new(format!(
            "BZip2 length mismatch: expected {expected_length}, got {}",
            output.len()
        )));
    }
    Ok(output)
}

fn encode_block(writer: &mut BitWriter, input: &[u8], block_crc: u32) -> Result<(), Error> {
    let rle1 = encode_rle1(input);
    let (bwt, original_pointer) = bwt(&rle1);
    let (used, symbols) = mtf_rle2(&bwt);
    let alphabet_size = used.len() + 2;
    let lengths = balanced_code_lengths(alphabet_size);
    let codes = canonical_codes(&lengths)?;
    let selector_count = symbols.len().div_ceil(GROUP_SIZE).max(1);
    let group_count = match symbols.len() {
        0..=199 => 2,
        200..=599 => 3,
        600..=1_199 => 4,
        1_200..=2_399 => 5,
        _ => 6,
    };
    if selector_count > 0x7fff {
        return Err(Error::new("BZip2 selector count exceeds format limit"));
    }

    writer.write_bits(BLOCK_MAGIC, 48);
    writer.write_bits(u64::from(block_crc), 32);
    writer.write_bits(0, 1); // modern encoders never randomize blocks
    writer.write_bits(original_pointer as u64, 24);
    write_used_map(writer, &used);
    writer.write_bits(group_count as u64, 3);
    writer.write_bits(selector_count as u64, 15);
    let mut selector_mtf: Vec<usize> = (0..group_count).collect();
    for selector in (0..selector_count).map(|index| index % group_count) {
        let mtf_index = selector_mtf
            .iter()
            .position(|&value| value == selector)
            .expect("both selector values are present");
        for _ in 0..mtf_index {
            writer.write_bits(1, 1);
        }
        writer.write_bits(0, 1);
        let value = selector_mtf.remove(mtf_index);
        selector_mtf.insert(0, value);
    }
    for _ in 0..group_count {
        write_code_lengths(writer, &lengths);
    }
    for &symbol in &symbols {
        writer.write_bits(u64::from(codes[symbol]), usize::from(lengths[symbol]));
    }
    Ok(())
}

fn write_used_map(writer: &mut BitWriter, used: &[u8]) {
    let mut present = [false; 256];
    for &byte in used {
        present[usize::from(byte)] = true;
    }
    for group in 0..16 {
        writer.write_bits(
            u64::from(present[group * 16..group * 16 + 16].contains(&true)),
            1,
        );
    }
    for group in 0..16 {
        if present[group * 16..group * 16 + 16].contains(&true) {
            for &value in &present[group * 16..group * 16 + 16] {
                writer.write_bits(u64::from(value), 1);
            }
        }
    }
}

fn write_code_lengths(writer: &mut BitWriter, lengths: &[u8]) {
    let mut current = lengths[0];
    writer.write_bits(u64::from(current), 5);
    for &target in lengths {
        while current < target {
            writer.write_bits(0b10, 2);
            current += 1;
        }
        while current > target {
            writer.write_bits(0b11, 2);
            current -= 1;
        }
        writer.write_bits(0, 1);
    }
}

fn decode_block_symbols(reader: &mut BitReader<'_>, block_limit: usize) -> Result<Vec<u8>, Error> {
    let used = read_used_map(reader)?;
    if used.is_empty() {
        return Err(Error::new("BZip2 block has an empty symbol map"));
    }
    let alphabet_size = used.len() + 2;
    let group_count = reader.read_bits(3)? as usize;
    if !(2..=6).contains(&group_count) {
        return Err(Error::new("invalid BZip2 Huffman group count"));
    }
    let selector_count = reader.read_bits(15)? as usize;
    if selector_count == 0 {
        return Err(Error::new("BZip2 block has no selectors"));
    }
    let mut selector_mtf: Vec<usize> = (0..group_count).collect();
    let mut selectors = Vec::with_capacity(selector_count);
    for _ in 0..selector_count {
        let mut index = 0usize;
        while reader.read_bits(1)? != 0 {
            index += 1;
            if index >= group_count {
                return Err(Error::new("invalid BZip2 selector MTF value"));
            }
        }
        let value = selector_mtf.remove(index);
        selector_mtf.insert(0, value);
        selectors.push(value);
    }
    let mut trees = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        let mut current = reader.read_bits(5)? as i32;
        let mut lengths = Vec::with_capacity(alphabet_size);
        for _ in 0..alphabet_size {
            while reader.read_bits(1)? != 0 {
                if reader.read_bits(1)? == 0 {
                    current += 1;
                } else {
                    current -= 1;
                }
                if !(1..=MAX_CODE_LENGTH as i32).contains(&current) {
                    return Err(Error::new("invalid BZip2 Huffman code length"));
                }
            }
            lengths.push(current as u8);
        }
        trees.push(Huffman::new(&lengths)?);
    }

    let end_symbol = used.len() + 1;
    let mut mtf = used.clone();
    let mut bwt = Vec::new();
    let mut selector_index = 0usize;
    let mut group_remaining = 0usize;
    let mut run_value = 0usize;
    let mut run_weight = 1usize;
    loop {
        if group_remaining == 0 {
            if selector_index >= selectors.len() {
                return Err(Error::new("BZip2 selector list ended before block"));
            }
            group_remaining = GROUP_SIZE;
        }
        let tree = &trees[selectors[selector_index]];
        let symbol = tree.decode(reader)? as usize;
        group_remaining -= 1;
        if group_remaining == 0 {
            selector_index += 1;
        }

        if symbol == RUNA || symbol == RUNB {
            let addition = if symbol == RUNA {
                run_weight
            } else {
                run_weight * 2
            };
            run_value = run_value
                .checked_add(addition)
                .ok_or_else(|| Error::new("BZip2 run length overflow"))?;
            run_weight = run_weight
                .checked_mul(2)
                .ok_or_else(|| Error::new("BZip2 run weight overflow"))?;
            continue;
        }
        if run_value != 0 {
            if bwt.len().saturating_add(run_value) > block_limit {
                return Err(Error::new("BZip2 block exceeds declared block size"));
            }
            bwt.resize(bwt.len() + run_value, mtf[0]);
            run_value = 0;
            run_weight = 1;
        }
        if symbol == end_symbol {
            break;
        }
        if symbol < 2 || symbol > used.len() {
            return Err(Error::new("invalid BZip2 MTF symbol"));
        }
        let index = symbol - 1;
        let byte = mtf.remove(index);
        mtf.insert(0, byte);
        if bwt.len() == block_limit {
            return Err(Error::new("BZip2 block exceeds declared block size"));
        }
        bwt.push(byte);
    }
    Ok(bwt)
}

fn read_used_map(reader: &mut BitReader<'_>) -> Result<Vec<u8>, Error> {
    let mut groups = [false; 16];
    for group in &mut groups {
        *group = reader.read_bits(1)? != 0;
    }
    let mut used = Vec::new();
    for (group, &present) in groups.iter().enumerate() {
        if present {
            for low in 0..16 {
                if reader.read_bits(1)? != 0 {
                    used.push((group * 16 + low) as u8);
                }
            }
        }
    }
    Ok(used)
}

fn balanced_code_lengths(symbol_count: usize) -> Vec<u8> {
    let bits = (usize::BITS - (symbol_count - 1).leading_zeros()) as u8;
    let short_count = (1usize << bits) - symbol_count;
    let mut lengths = vec![bits; symbol_count];
    if bits > 1 {
        lengths[..short_count].fill(bits - 1);
    }
    lengths
}

fn canonical_codes(lengths: &[u8]) -> Result<Vec<u32>, Error> {
    let mut counts = [0u32; MAX_CODE_LENGTH + 1];
    for &length in lengths {
        if length == 0 || usize::from(length) > MAX_CODE_LENGTH {
            return Err(Error::new("invalid BZip2 Huffman length"));
        }
        counts[usize::from(length)] += 1;
    }
    let mut left = 1i64;
    for &count in &counts[1..] {
        left = (left << 1) - i64::from(count);
        if left < 0 {
            return Err(Error::new("oversubscribed BZip2 Huffman tree"));
        }
    }
    let mut next = [0u32; MAX_CODE_LENGTH + 1];
    let mut code = 0u32;
    for bits in 1..=MAX_CODE_LENGTH {
        code = (code + counts[bits - 1]) << 1;
        next[bits] = code;
    }
    let mut codes = vec![0u32; lengths.len()];
    for (symbol, &length) in lengths.iter().enumerate() {
        codes[symbol] = next[usize::from(length)];
        next[usize::from(length)] += 1;
    }
    Ok(codes)
}

struct Huffman {
    by_length: Vec<Vec<(u32, u16)>>,
    max_length: usize,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, Error> {
        let codes = canonical_codes(lengths)?;
        let max_length = usize::from(*lengths.iter().max().unwrap());
        let mut by_length = vec![Vec::new(); max_length + 1];
        for (symbol, (&code, &length)) in codes.iter().zip(lengths).enumerate() {
            by_length[usize::from(length)].push((code, symbol as u16));
        }
        Ok(Self {
            by_length,
            max_length,
        })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, Error> {
        let mut code = 0u32;
        for length in 1..=self.max_length {
            code = code << 1 | reader.read_bits(1)? as u32;
            if let Ok(index) =
                self.by_length[length].binary_search_by_key(&code, |&(value, _)| value)
            {
                return Ok(self.by_length[length][index].1);
            }
        }
        Err(Error::new("invalid BZip2 Huffman code"))
    }
}

#[cfg(test)]
mod tests {
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
            vec![b'a'; 259],
            vec![b'a'; 260],
            b"from-scratch bzip2 block coding".repeat(100),
            (0..=255).cycle().take(100_001).collect(),
        ]
    }

    #[test]
    fn rle1_round_trip_boundaries() {
        for input in fixtures() {
            let encoded = encode_rle1(&input);
            assert_eq!(decode_rle1(&encoded, input.len()).unwrap(), input);
        }
    }

    #[test]
    fn bwt_round_trip() {
        for input in [
            b"banana".as_slice(),
            b"mississippi",
            b"abcdefg",
            b"aaaaaaaa",
        ] {
            let (encoded, pointer) = bwt(input);
            assert_eq!(inverse_bwt(&encoded, pointer).unwrap(), input);
        }
    }

    #[test]
    fn stream_round_trip_boundaries() {
        for input in fixtures() {
            let encoded = encode(&input).unwrap();
            assert_eq!(&encoded[..4], b"BZh1");
            assert_eq!(decode(&encoded, input.len()).unwrap(), input);
        }
    }

    #[test]
    fn rle1_expansion_is_split_at_the_encoded_block_limit() {
        for size in [80_000, 80_001, 99_999, 100_000, 100_001, 200_001] {
            let input: Vec<u8> = (0..size).map(|index| (index / 4 % 251) as u8).collect();
            let encoded = encode(&input).unwrap();
            assert_eq!(decode(&encoded, input.len()).unwrap(), input, "size {size}");

            let mut start = 0usize;
            while start < input.len() {
                let end = rle1_block_end(&input, start, 100_000 - BLOCK_OVERSHOOT_GUARD);
                assert!(encode_rle1(&input[start..end]).len() <= 100_000 - BLOCK_OVERSHOOT_GUARD);
                start = end;
            }
        }
    }

    #[test]
    fn decodes_concatenated_bzip2_streams() {
        let first = b"first concatenated stream".repeat(100);
        let second = deterministic_bytes(4097, 0x9e37_79b9);
        let mut encoded = encode(&first).unwrap();
        encoded.extend_from_slice(&encode_with_block_size(&second, 9).unwrap());

        let mut expected = first;
        expected.extend_from_slice(&second);
        assert_eq!(decode(&encoded, expected.len()).unwrap(), expected);

        encoded.push(0);
        assert!(decode(&encoded, expected.len()).is_err());
    }

    #[test]
    fn large_deterministic_round_trip_matrix() {
        const SIZES: [usize; 41] = [
            0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 257, 258, 259, 511,
            512, 1023, 4095, 8191, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536,
            65_537, 99_999, 100_000, 100_001, 199_999, 200_000, 200_001,
        ];
        let mut cases = 0usize;
        for &size in &SIZES {
            let inputs = [
                vec![0; size],
                (0..size).map(|index| index as u8).collect(),
                deterministic_bytes(size, 0x517c_c1b7 ^ size as u32),
            ];
            for input in inputs {
                let encoded = encode(&input).unwrap();
                assert_eq!(decode(&encoded, input.len()).unwrap(), input);
                cases += 1;
            }
        }
        assert_eq!(cases, 123);

        let input = deterministic_bytes(200_001, 0xc2b2_ae35);
        for block_size in [1, 2, 9] {
            let encoded = encode_with_block_size(&input, block_size).unwrap();
            assert_eq!(encoded[3], b'0' + block_size);
            assert_eq!(decode(&encoded, input.len()).unwrap(), input);
        }
    }

    #[test]
    fn rejects_corruption_and_truncation() {
        let input = b"corrupt bzip streams must fail".repeat(100);
        let mut encoded = encode(&input).unwrap();
        encoded.truncate(encoded.len() / 2);
        assert!(decode(&encoded, input.len()).is_err());

        let mut encoded = encode(&input).unwrap();
        encoded[10] ^= 1;
        assert!(decode(&encoded, input.len()).is_err());
    }

    #[test]
    fn rejects_all_truncations_and_never_silently_corrupts_bit_flips() {
        let input = deterministic_bytes(257, 0x1234_5678);
        let encoded = encode(&input).unwrap();
        for end in 0..encoded.len() {
            assert!(
                decode(&encoded[..end], input.len()).is_err(),
                "accepted truncation at {end}/{}",
                encoded.len()
            );
        }

        let mut rejected = 0usize;
        for index in 0..encoded.len() {
            for bit in 0..8 {
                let mut damaged = encoded.clone();
                damaged[index] ^= 1 << bit;
                match decode(&damaged, input.len()) {
                    Err(_) => rejected += 1,
                    Ok(decoded) => assert_eq!(
                        decoded, input,
                        "bit flip {bit} in byte {index} silently changed output"
                    ),
                }
            }
        }
        assert!(rejected > encoded.len() * 7);
    }

    #[test]
    fn decodes_external_bzip2_streams() {
        // Python 3 bz2.compress(..., compresslevel=1) output.
        let empty = [
            0x42, 0x5a, 0x68, 0x31, 0x17, 0x72, 0x45, 0x38, 0x50, 0x90, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(decode(&empty, 0).unwrap(), Vec::<u8>::new());

        let fixed_text = [
            0x42, 0x5a, 0x68, 0x31, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x9e, 0x62, 0x5b, 0xfe,
            0x00, 0x00, 0x02, 0x91, 0x00, 0x40, 0x00, 0x02, 0x44, 0xa0, 0x00, 0x21, 0x14, 0x60,
            0x66, 0x82, 0x91, 0xef, 0x23, 0x47, 0x0b, 0xb9, 0x22, 0x9c, 0x28, 0x48, 0x4f, 0x31,
            0x2d, 0xff, 0x00,
        ];
        assert_eq!(decode(&fixed_text, 17).unwrap(), b"hello hello hello");

        let repeated = [
            0x42, 0x5a, 0x68, 0x31, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0xaf, 0xe7, 0x12, 0xf2,
            0x00, 0x00, 0x31, 0x81, 0x00, 0x3e, 0x00, 0x20, 0x00, 0x30, 0xcd, 0x00, 0x52, 0x89,
            0xa7, 0x11, 0x61, 0x16, 0x91, 0x78, 0x8b, 0x88, 0xbe, 0x2e, 0xe4, 0x8a, 0x70, 0xa1,
            0x21, 0x5f, 0xce, 0x25, 0xe4,
        ];
        assert_eq!(decode(&repeated, 500).unwrap(), b"abcde".repeat(100));

        let mut concatenated = fixed_text.to_vec();
        concatenated.extend_from_slice(&repeated);
        let mut expected = b"hello hello hello".to_vec();
        expected.extend_from_slice(&b"abcde".repeat(100));
        assert_eq!(decode(&concatenated, expected.len()).unwrap(), expected);
    }

    #[test]
    fn crc_known_value() {
        assert_eq!(crc32(b"123456789"), 0xfc89_1918);
    }

    #[test]
    fn legacy_randomization_is_reversible() {
        let original: Vec<u8> = (0..=255).cycle().take(20_000).collect();
        let mut randomized = original.clone();
        derandomize(&mut randomized);
        assert_ne!(randomized, original);
        derandomize(&mut randomized);
        assert_eq!(randomized, original);
    }
}
