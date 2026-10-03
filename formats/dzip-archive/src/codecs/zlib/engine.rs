use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::codecs::zlib::Error;
use crate::codecs::zlib::bitstream::{BitReader, BitWriter};
use crate::codecs::zlib::checksum::adler32;
use crate::codecs::zlib::matchfinder::{find_match, insert_position};

const MAX_BITS: usize = 15;
const INVALID_ENTRY: u32 = u32::MAX;

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12_289, 16_385, 24_577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Encode a raw RFC 1951 DEFLATE stream.
pub(crate) fn encode_raw(input: &[u8]) -> Vec<u8> {
    encode_raw_with_buffer(input, Vec::new())
}

pub(crate) fn encode_raw_with_buffer(input: &[u8], mut output: Vec<u8>) -> Vec<u8> {
    output.clear();
    encode_raw_appending(input, output)
}

fn encode_raw_appending(input: &[u8], output: Vec<u8>) -> Vec<u8> {
    let litlen = fixed_literal_lengths();
    let distance = vec![5; 32];
    let lit_codes = canonical_codes(&litlen).expect("the fixed tree is valid");
    let dist_codes = canonical_codes(&distance).expect("the fixed tree is valid");
    let mut writer = BitWriter::with_output(output);

    // One final fixed-Huffman block.
    writer.write_bits(1, 1);
    writer.write_bits(1, 2);

    let mut head = vec![usize::MAX; 1 << 16];
    let mut previous = vec![usize::MAX; input.len()];
    let mut position = 0usize;
    while position < input.len() {
        let (length, distance_value) = find_match(input, position, &head, &previous);
        insert_position(input, position, &mut head, &mut previous);
        if length >= 3 {
            write_length(&mut writer, length, &lit_codes, &litlen);
            write_distance(&mut writer, distance_value, &dist_codes, &distance);
            let end = (position + length).min(input.len());
            for next in position + 1..end {
                insert_position(input, next, &mut head, &mut previous);
            }
            position = end;
        } else {
            write_symbol(&mut writer, input[position] as usize, &lit_codes, &litlen);
            position += 1;
        }
    }
    write_symbol(&mut writer, 256, &lit_codes, &litlen);
    writer.finish()
}

/// Decode a raw RFC 1951 DEFLATE stream to exactly `expected_length` bytes.
#[cfg(test)]
pub(crate) fn decode_raw(input: &[u8], expected_length: usize) -> Result<Vec<u8>, Error> {
    decode_raw_with_buffer(input, expected_length, Vec::new())
}

pub(crate) fn decode_raw_with_buffer(
    input: &[u8],
    expected_length: usize,
    mut output: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let mut reader = BitReader::new(input);
    output.clear();
    if output.capacity() < expected_length {
        output.reserve(expected_length);
    }
    loop {
        let final_block = reader.read_bits(1)? != 0;
        match reader.read_bits(2)? {
            0 => decode_stored(&mut reader, &mut output, expected_length)?,
            1 => {
                let literal = Huffman::new(&fixed_literal_lengths())?;
                let distance = Huffman::new(&[5; 32])?;
                decode_huffman_block(
                    &mut reader,
                    &mut output,
                    expected_length,
                    &literal,
                    Some(&distance),
                )?;
            }
            2 => {
                let (literal, distance) = read_dynamic_trees(&mut reader)?;
                decode_huffman_block(
                    &mut reader,
                    &mut output,
                    expected_length,
                    &literal,
                    distance.as_ref(),
                )?;
            }
            _ => return Err(Error::new("reserved DEFLATE block type")),
        }
        if final_block {
            break;
        }
    }
    if output.len() != expected_length {
        return Err(Error::new(format!(
            "DEFLATE length mismatch: expected {expected_length}, got {}",
            output.len()
        )));
    }
    Ok(output)
}

/// Encode a complete RFC 1950 zlib stream.
pub(crate) fn encode_zlib(input: &[u8]) -> Vec<u8> {
    encode_zlib_with_buffer(input, Vec::new())
}

pub(crate) fn encode_zlib_with_buffer(input: &[u8], mut output: Vec<u8>) -> Vec<u8> {
    output.clear();
    // CM=8, CINFO=7 (32 KiB); level hint=2; header is divisible by 31.
    output.extend_from_slice(&[0x78, 0x9c]);
    output = encode_raw_appending(input, output);
    output.extend_from_slice(&adler32(input).to_be_bytes());
    output
}

/// Decode and verify a complete RFC 1950 zlib stream.
#[cfg(test)]
pub(crate) fn decode_zlib(input: &[u8], expected_length: usize) -> Result<Vec<u8>, Error> {
    decode_zlib_with_buffer(input, expected_length, Vec::new())
}

pub(crate) fn decode_zlib_with_buffer(
    input: &[u8],
    expected_length: usize,
    output: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    if input.len() < 6 {
        return Err(Error::new("truncated zlib stream"));
    }
    let cmf = input[0];
    let flg = input[1];
    if cmf & 0x0f != 8 || cmf >> 4 > 7 {
        return Err(Error::new("unsupported zlib compression method or window"));
    }
    if (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 {
        return Err(Error::new("invalid zlib header checksum"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::new("preset zlib dictionaries are unsupported"));
    }
    let payload_end = input.len() - 4;
    let output = decode_raw_with_buffer(&input[2..payload_end], expected_length, output)?;
    let declared = u32::from_be_bytes(
        input[payload_end..]
            .try_into()
            .expect("the checksum slice has four bytes"),
    );
    let actual = adler32(&output);
    if actual != declared {
        return Err(Error::new("zlib Adler-32 mismatch"));
    }
    Ok(output)
}

fn write_length(writer: &mut BitWriter, length: usize, codes: &[u16], lengths: &[u8]) {
    let index = LENGTH_BASE
        .iter()
        .enumerate()
        .find(|&(index, &base)| {
            let extra = LENGTH_EXTRA[index];
            length <= base + ((1usize << extra) - 1)
        })
        .map(|(index, _)| index)
        .expect("a DEFLATE match is at most 258 bytes");
    write_symbol(writer, 257 + index, codes, lengths);
    let extra = LENGTH_EXTRA[index];
    writer.write_bits((length - LENGTH_BASE[index]) as u32, extra);
}

fn write_distance(writer: &mut BitWriter, distance: usize, codes: &[u16], lengths: &[u8]) {
    let index = DIST_BASE
        .iter()
        .enumerate()
        .find(|&(index, &base)| {
            let extra = DIST_EXTRA[index];
            distance <= base + ((1usize << extra) - 1)
        })
        .map(|(index, _)| index)
        .expect("a DEFLATE distance is at most 32768 bytes");
    write_symbol(writer, index, codes, lengths);
    let extra = DIST_EXTRA[index];
    writer.write_bits((distance - DIST_BASE[index]) as u32, extra);
}

fn write_symbol(writer: &mut BitWriter, symbol: usize, codes: &[u16], lengths: &[u8]) {
    writer.write_bits(
        u32::from(reverse_code(codes[symbol], lengths[symbol])),
        lengths[symbol],
    );
}

fn decode_stored(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    limit: usize,
) -> Result<(), Error> {
    reader.align_to_byte();
    let length = reader.read_bits(16)? as u16;
    let complement = reader.read_bits(16)? as u16;
    if length != !complement {
        return Err(Error::new("invalid stored-block length complement"));
    }
    let length = usize::from(length);
    if output.len().saturating_add(length) > limit {
        return Err(Error::new("DEFLATE output exceeds declared length"));
    }
    for _ in 0..length {
        output.push(reader.read_bits(8)? as u8);
    }
    Ok(())
}

fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    output: &mut Vec<u8>,
    limit: usize,
    literal: &Huffman,
    distance: Option<&Huffman>,
) -> Result<(), Error> {
    loop {
        let symbol = literal.decode(reader)?;
        match symbol {
            0..=255 => {
                if output.len() == limit {
                    return Err(Error::new("DEFLATE output exceeds declared length"));
                }
                output.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = symbol as usize - 257;
                let length = LENGTH_BASE[index] + reader.read_bits(LENGTH_EXTRA[index])? as usize;
                let distance_symbol = distance
                    .ok_or_else(|| Error::new("DEFLATE length uses an empty distance tree"))?
                    .decode(reader)? as usize;
                if distance_symbol >= DIST_BASE.len() {
                    return Err(Error::new("reserved DEFLATE distance symbol"));
                }
                let distance_value = DIST_BASE[distance_symbol]
                    + reader.read_bits(DIST_EXTRA[distance_symbol])? as usize;
                if distance_value == 0 || distance_value > output.len() {
                    return Err(Error::new("DEFLATE match precedes output window"));
                }
                if output.len().saturating_add(length) > limit {
                    return Err(Error::new("DEFLATE output exceeds declared length"));
                }
                for _ in 0..length {
                    let byte = output[output.len() - distance_value];
                    output.push(byte);
                }
            }
            _ => return Err(Error::new("reserved DEFLATE literal/length symbol")),
        }
    }
}

fn read_dynamic_trees(reader: &mut BitReader<'_>) -> Result<(Huffman, Option<Huffman>), Error> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let literal_count = reader.read_bits(5)? as usize + 257;
    let distance_count = reader.read_bits(5)? as usize + 1;
    let code_count = reader.read_bits(4)? as usize + 4;
    let mut code_lengths = vec![0u8; 19];
    for &symbol in &ORDER[..code_count] {
        code_lengths[symbol] = reader.read_bits(3)? as u8;
    }
    let code_tree = Huffman::new(&code_lengths)?;
    let total = literal_count + distance_count;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        match code_tree.decode(reader)? {
            symbol @ 0..=15 => lengths.push(symbol as u8),
            16 => {
                let previous = *lengths
                    .last()
                    .ok_or_else(|| Error::new("code-length repeat has no predecessor"))?;
                let count = reader.read_bits(2)? as usize + 3;
                extend_lengths(&mut lengths, previous, count, total)?;
            }
            17 => {
                let count = reader.read_bits(3)? as usize + 3;
                extend_lengths(&mut lengths, 0, count, total)?;
            }
            18 => {
                let count = reader.read_bits(7)? as usize + 11;
                extend_lengths(&mut lengths, 0, count, total)?;
            }
            _ => return Err(Error::new("invalid code-length symbol")),
        }
    }
    let literal_lengths = &lengths[..literal_count];
    if literal_lengths[256] == 0 {
        return Err(Error::new("dynamic tree omits end-of-block symbol"));
    }
    let distance_lengths = &lengths[literal_count..];
    // RFC 1951 permits a single zero distance-code length when a dynamic
    // block contains only literals.  Keep the absent tree explicit so a
    // malformed length symbol still fails at the point it requests distance.
    let distance = if distance_lengths.iter().all(|&length| length == 0) {
        if distance_lengths.len() != 1 {
            return Err(Error::new(
                "empty DEFLATE distance tree declares more than one code",
            ));
        }
        None
    } else {
        Some(Huffman::new(distance_lengths)?)
    };
    Ok((Huffman::new(literal_lengths)?, distance))
}

fn extend_lengths(
    lengths: &mut Vec<u8>,
    value: u8,
    count: usize,
    total: usize,
) -> Result<(), Error> {
    if lengths.len().saturating_add(count) > total {
        return Err(Error::new("code-length repeat exceeds dynamic tree"));
    }
    lengths.resize(lengths.len() + count, value);
    Ok(())
}

fn fixed_literal_lengths() -> Vec<u8> {
    let mut lengths = vec![0u8; 288];
    lengths[..=143].fill(8);
    lengths[144..=255].fill(9);
    lengths[256..=279].fill(7);
    lengths[280..=287].fill(8);
    lengths
}

fn canonical_codes(lengths: &[u8]) -> Result<Vec<u16>, Error> {
    let mut counts = [0u16; MAX_BITS + 1];
    for &length in lengths {
        if usize::from(length) > MAX_BITS {
            return Err(Error::new("Huffman code exceeds 15 bits"));
        }
        if length != 0 {
            counts[usize::from(length)] += 1;
        }
    }
    let mut left = 1i32;
    for &count in &counts[1..] {
        left = (left << 1) - i32::from(count);
        if left < 0 {
            return Err(Error::new("oversubscribed Huffman tree"));
        }
    }
    let mut next = [0u16; MAX_BITS + 1];
    let mut code = 0u16;
    for bits in 1..=MAX_BITS {
        code = (code + counts[bits - 1]) << 1;
        next[bits] = code;
    }
    let mut codes = vec![0u16; lengths.len()];
    for (symbol, &length) in lengths.iter().enumerate() {
        if length != 0 {
            codes[symbol] = next[usize::from(length)];
            next[usize::from(length)] += 1;
        }
    }
    Ok(codes)
}

fn reverse_code(mut code: u16, length: u8) -> u16 {
    let mut reversed = 0u16;
    for _ in 0..length {
        reversed = reversed << 1 | code & 1;
        code >>= 1;
    }
    reversed
}

struct Huffman {
    table: Vec<u32>,
    max_bits: u8,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, Error> {
        let max_bits = lengths.iter().copied().max().unwrap_or(0);
        if max_bits == 0 {
            return Err(Error::new("empty Huffman tree"));
        }
        let codes = canonical_codes(lengths)?;
        let mut table = vec![INVALID_ENTRY; 1usize << max_bits];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length == 0 {
                continue;
            }
            let code = usize::from(reverse_code(codes[symbol], length));
            let repetitions = 1usize << (max_bits - length);
            for suffix in 0..repetitions {
                let index = code | suffix << length;
                table[index] = (symbol as u32) << 5 | u32::from(length);
            }
        }
        Ok(Self { table, max_bits })
    }

    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, Error> {
        let index = reader.peek_padded(self.max_bits)? as usize;
        let entry = self.table[index];
        if entry == INVALID_ENTRY {
            return Err(Error::new("invalid Huffman code"));
        }
        let length = (entry & 0x1f) as u8;
        reader.drop_bits(length)?;
        Ok((entry >> 5) as u16)
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
            b"dependency-free deflate".repeat(100),
            (0..=255).cycle().take(65_537).collect(),
            (0..100_000).map(|index| (index * 37) as u8).collect(),
        ]
    }

    #[test]
    fn raw_round_trip_boundaries() {
        for input in fixtures() {
            let encoded = encode_raw(&input);
            assert_eq!(decode_raw(&encoded, input.len()).unwrap(), input);
        }
    }

    #[test]
    fn zlib_round_trip_and_checksum() {
        for input in fixtures() {
            let encoded = encode_zlib(&input);
            assert_eq!(decode_zlib(&encoded, input.len()).unwrap(), input);
        }
    }

    #[test]
    fn decodes_dynamic_literal_only_block_without_a_distance_tree() {
        // A final dynamic block containing one literal ('A') and EOB.  Its
        // sole declared distance-code length is zero, as allowed by RFC 1951.
        let encoded = [
            0x05, 0xc0, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00, 0x90, 0x6d, 0xfe, 0x9f, 0x92,
        ];
        assert_eq!(decode_raw(&encoded, 1).unwrap(), b"A");
    }

    #[test]
    fn large_deterministic_round_trip_matrix() {
        const SIZES: [usize; 35] = [
            0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 257, 258, 259, 511,
            512, 1023, 4095, 8191, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536,
            65_537,
        ];
        let mut cases = 0usize;
        for &size in &SIZES {
            let inputs = [
                vec![0; size],
                (0..size).map(|index| index as u8).collect(),
                (0..size)
                    .map(|index| b"deflate-boundary-pattern"[index % 24])
                    .collect(),
                deterministic_bytes(size, 0x9e37_79b9 ^ size as u32),
            ];
            for input in inputs {
                let raw = encode_raw(&input);
                assert_eq!(decode_raw(&raw, input.len()).unwrap(), input);

                let wrapped = encode_zlib(&input);
                assert_eq!(decode_zlib(&wrapped, input.len()).unwrap(), input);
                cases += 1;
            }
        }
        assert_eq!(cases, 140);
    }

    #[test]
    fn rejects_truncation_and_corruption() {
        let input = b"corruption must be reported".repeat(100);
        let mut raw = encode_raw(&input);
        raw.truncate(raw.len() / 2);
        assert!(decode_raw(&raw, input.len()).is_err());

        let mut wrapped = encode_zlib(&input);
        *wrapped.last_mut().unwrap() ^= 1;
        assert!(decode_zlib(&wrapped, input.len()).is_err());
    }

    #[test]
    fn rejects_all_truncations_and_never_silently_corrupts_bit_flips() {
        let input = deterministic_bytes(8193, 0x1234_5678);
        let encoded = encode_zlib(&input);

        for end in 0..encoded.len() {
            assert!(
                decode_zlib(&encoded[..end], input.len()).is_err(),
                "accepted truncation at {end}/{}",
                encoded.len()
            );
        }

        let mut rejected = 0usize;
        for index in 0..encoded.len() {
            for bit in 0..8 {
                let mut damaged = encoded.clone();
                damaged[index] ^= 1 << bit;
                match decode_zlib(&damaged, input.len()) {
                    Err(_) => rejected += 1,
                    Ok(decoded) => assert_eq!(
                        decoded, input,
                        "bit flip {bit} in byte {index} silently changed output"
                    ),
                }
            }
        }
        assert!(rejected > encoded.len() * 7);

        assert!(decode_raw(&[0b0000_0111], 0).is_err());
        assert!(decode_raw(&[0x01, 0x01, 0x00, 0x00, 0x00, 0x00], 1).is_err());
        assert!(decode_zlib(&[0x78, 0x00, 0, 0, 0, 1], 0).is_err());
        assert!(decode_zlib(&[0x78, 0x20, 0, 0, 0, 1], 0).is_err());
    }

    #[test]
    fn decodes_external_stored_fixed_dynamic_and_multiblock_streams() {
        // Python/zlib level 0 output: one stored block.
        let stored = [
            0x78, 0x01, 0x01, 0x1a, 0x00, 0xe5, 0xff, 0x73, 0x74, 0x6f, 0x72, 0x65, 0x64, 0x2d,
            0x62, 0x6c, 0x6f, 0x63, 0x6b, 0x2d, 0x63, 0x6f, 0x6d, 0x70, 0x61, 0x74, 0x69, 0x62,
            0x69, 0x6c, 0x69, 0x74, 0x79, 0x8b, 0x27, 0x0a, 0x71,
        ];
        assert_eq!(
            decode_zlib(&stored, 26).unwrap(),
            b"stored-block-compatibility"
        );

        // Python/zlib Z_FIXED output.
        let fixed = [
            0x78, 0x01, 0x4b, 0xcb, 0xac, 0x48, 0x4d, 0xd1, 0xcd, 0x28, 0x4d, 0x4b, 0xcb, 0x4d,
            0xcc, 0xd3, 0x4d, 0xce, 0xcf, 0x2d, 0x48, 0x2c, 0xc9, 0x4c, 0xca, 0xcc, 0xc9, 0x2c,
            0xa9, 0x54, 0x48, 0x1b, 0x95, 0x1b, 0x95, 0x43, 0x93, 0x03, 0x00, 0x32, 0xdd, 0xda,
            0x35,
        ];
        let expected_fixed = b"fixed-huffman-compatibility ".repeat(20);
        let decoded_fixed = decode_raw(&fixed[2..fixed.len() - 4], 560).unwrap();
        assert_eq!(decoded_fixed, expected_fixed);
        assert_eq!(decode_zlib(&fixed, 560).unwrap(), expected_fixed);

        // Python/zlib level 6 output with a dynamic Huffman block.
        let dynamic = [
            0x78, 0x9c, 0xed, 0xca, 0xc7, 0x01, 0x80, 0x20, 0x10, 0x45, 0xc1, 0x56, 0x7e, 0x05,
            0x56, 0x43, 0x03, 0x06, 0x14, 0x03, 0xac, 0xa2, 0x98, 0xaa, 0xb7, 0x09, 0x8f, 0x6f,
            0xce, 0xe3, 0x82, 0xd7, 0x56, 0xc6, 0x76, 0x56, 0x93, 0xed, 0x4a, 0xea, 0xed, 0xd6,
            0x54, 0xe2, 0xba, 0xcb, 0x4e, 0x9f, 0x75, 0x04, 0xaf, 0xa5, 0x7e, 0x1f, 0x75, 0x36,
            0x54, 0x72, 0x64, 0x32, 0x99, 0x4c, 0x26, 0x93, 0xc9, 0x64, 0x32, 0x99, 0x4c, 0x26,
            0x93, 0xc9, 0xe4, 0xf8, 0x43, 0xfe, 0x00, 0x1d, 0xc8, 0x4f, 0x97,
        ];
        assert_eq!(
            decode_zlib(&dynamic, 4500).unwrap(),
            b"The quick brown fox jumps over the lazy dog. ".repeat(100)
        );

        // Hand-authored RFC 1951 stream with two stored blocks.
        let multiblock = [
            0x78, 0x01, 0x00, 0x03, 0x00, 0xfc, 0xff, 0x61, 0x62, 0x63, 0x01, 0x03, 0x00, 0xfc,
            0xff, 0x64, 0x65, 0x66, 0x08, 0x1e, 0x02, 0x56,
        ];
        assert_eq!(decode_zlib(&multiblock, 6).unwrap(), b"abcdef");

        // Python/zlib level 6 output for b"hello hello hello" is fixed,
        // despite its default compression strategy.
        let small_fixed = [
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e,
            0x06, 0x7d,
        ];
        assert_eq!(decode_zlib(&small_fixed, 17).unwrap(), b"hello hello hello");
    }
}
