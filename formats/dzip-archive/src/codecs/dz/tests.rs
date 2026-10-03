use super::*;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

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

#[test]
fn dz_round_trip_literals_and_matches() {
    let data = b"the quick brown fox jumps over the quick brown fox\n".repeat(32);
    let settings = RangeSettings::default();
    let compressed = compress_chunk(&data, settings).unwrap();
    let decompressed = decompress_chunk(&compressed, data.len(), settings).unwrap();
    assert_eq!(decompressed, data);
    assert!(compressed.len() < data.len());
}

#[test]
fn dz_round_trip_small_inputs() {
    let settings = RangeSettings::default();
    for data in [
        Vec::new(),
        vec![0],
        vec![0, 0],
        vec![0, 1, 2],
        vec![255; 32],
    ] {
        let compressed = compress_chunk(&data, settings).unwrap();
        let decompressed = decompress_chunk(&compressed, data.len(), settings).unwrap();
        assert_eq!(decompressed, data);
    }
}

#[test]
fn large_deterministic_round_trip_matrix() {
    const SIZES: [usize; 35] = [
        0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 257, 258, 259, 511,
        512, 1023, 4095, 8191, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536,
        65_537,
    ];
    let settings = RangeSettings::default();
    let mut cases = 0usize;
    for &size in &SIZES {
        let inputs = [
            vec![0; size],
            (0..size).map(|index| index as u8).collect(),
            (0..size)
                .map(|index| b"dz-boundary-pattern"[index % 19])
                .collect(),
            deterministic_bytes(size, 0x1656_67b1 ^ size as u32),
        ];
        for input in inputs {
            let encoded = compress_chunk(&input, settings).unwrap();
            assert_eq!(
                decompress_chunk(&encoded, input.len(), settings).unwrap(),
                input
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 140);
}

#[test]
fn range_settings_matrix_round_trips() {
    let input = deterministic_bytes(8193, 0xd3a2_646c);
    let settings = [
        RangeSettings::default(),
        RangeSettings {
            win_size: 0,
            flags: 0,
            offset_table_size: 1,
            offset_tables: 1,
            offset_contexts: 1,
            ref_length_table_size: 0,
            ref_length_tables: 0,
            ref_offset_table_size: 0,
            ref_offset_tables: 0,
            big_min_match: 2,
        },
        RangeSettings {
            win_size: 12,
            offset_table_size: 5,
            offset_tables: 2,
            offset_contexts: 8,
            big_min_match: 3,
            ..RangeSettings::default()
        },
        RangeSettings {
            win_size: 20,
            flags: 0,
            offset_table_size: 10,
            offset_tables: 4,
            offset_contexts: 4,
            big_min_match: 32,
            ..RangeSettings::default()
        },
    ];
    for settings in settings {
        let encoded = compress_chunk(&input, settings).unwrap();
        assert_eq!(
            decompress_chunk(&encoded, input.len(), settings).unwrap(),
            input,
            "{settings:?}"
        );
    }
}

#[test]
fn truncations_and_bit_flips_are_bounded() {
    let input = deterministic_bytes(513, 0x1234_5678);
    let settings = RangeSettings::default();
    let encoded = compress_chunk(&input, settings).unwrap();

    let mut affected_truncations = 0usize;
    for end in 0..encoded.len() {
        match decompress_chunk(&encoded[..end], input.len(), settings) {
            Err(_) => affected_truncations += 1,
            Ok(decoded) if decoded != input => affected_truncations += 1,
            Ok(_) => {}
        }
    }
    assert!(affected_truncations > encoded.len() / 2);

    let mut affected_flips = 0usize;
    for index in 0..encoded.len() {
        for bit in 0..8 {
            let mut damaged = encoded.clone();
            damaged[index] ^= 1 << bit;
            match decompress_chunk(&damaged, input.len(), settings) {
                Err(_) => affected_flips += 1,
                Ok(decoded) if decoded != input => affected_flips += 1,
                Ok(_) => {}
            }
        }
    }
    assert!(affected_flips > encoded.len() * 4);
}

#[test]
fn empty_dz_stream_matches_dzip_original() {
    assert_eq!(
        compress_chunk(&[], RangeSettings::default()).unwrap(),
        [0xff, 0xc0]
    );
}

#[test]
fn dz_without_common_buffer_allows_zero_reference_settings() {
    let settings = RangeSettings {
        ref_length_table_size: 0,
        ref_length_tables: 0,
        ref_offset_table_size: 0,
        ref_offset_tables: 0,
        ..RangeSettings::default()
    };
    let data = b"local matches still work without external-reference models".repeat(8);
    let compressed = compress_chunk(&data, settings).unwrap();
    let decompressed = decompress_chunk(&compressed, data.len(), settings).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn dz_rejects_original_memory_estimate_overflow() {
    let inputs = vec![vec![0u8; 1024]];
    let options = DzEncoderOptions {
        max_mem_usage: 1,
        ..DzEncoderOptions::default()
    };
    let error = compress_archive(&inputs, &options).unwrap_err();
    assert!(error.to_string().contains("max_mem_usage"));
}

#[test]
fn combuf_option_without_references_emits_original_placeholder() {
    let inputs = vec![b"alpha".to_vec(), b"omega".to_vec()];
    let options = DzEncoderOptions {
        use_combuf: true,
        ..DzEncoderOptions::default()
    };
    let encoded = compress_archive(&inputs, &options).unwrap();
    assert_eq!(encoded.common_buffer, Some(Vec::new()));
    let common = DzCommonBuffer::new(options.settings, vec![Vec::new()]).unwrap();
    for (input, compressed) in inputs.iter().zip(&encoded.chunks) {
        let decoded = decompress_chunk_with_common_buffer(
            compressed,
            input.len(),
            options.settings,
            Some(&common),
        )
        .unwrap();
        assert_eq!(&decoded, input);
    }
}

#[test]
fn dz_archive_round_trip_with_common_buffer() {
    let mut state = 0x9e37_79b9u32;
    let shared: Vec<u8> = (0..8192)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    let mut shifted_once = vec![0x13];
    shifted_once.extend_from_slice(&shared);
    let mut shifted_twice = vec![0x37, 0x42];
    shifted_twice.extend_from_slice(&shared);
    let inputs = vec![shared, shifted_once, shifted_twice];
    let options = DzEncoderOptions {
        use_combuf: true,
        preprocess: false,
        ..DzEncoderOptions::default()
    };
    let encoded = compress_archive(&inputs, &options).unwrap();
    let common_bytes = encoded.common_buffer.as_ref().unwrap();
    assert!(!common_bytes.is_empty());
    let prefix_size = options.settings.combuf_static_prefix_size();
    assert!(
        common_bytes[..prefix_size]
            .iter()
            .any(|&frequency| frequency != 1),
        "COMBUF static model should contain analyzed frequencies"
    );
    let common = DzCommonBuffer::new(options.settings, vec![common_bytes.clone()]).unwrap();

    for (input, compressed) in inputs.iter().zip(&encoded.chunks) {
        let decoded = decompress_chunk_with_common_buffer(
            compressed,
            input.len(),
            options.settings,
            Some(&common),
        )
        .unwrap();
        assert_eq!(&decoded, input);
    }
}

#[test]
fn common_buffer_configuration_matrix_round_trips() {
    let configurations = [
        (false, 0, 32usize),
        (false, 20, 258),
        (true, 0, 64),
        (true, 20, usize::MAX),
    ];
    let mut cases = 0usize;
    for size in [32usize, 257, 1024, 4096] {
        let shared = deterministic_bytes(size, 0x9e37_79b9 ^ size as u32);
        let mut prefixed = vec![0x13, 0x37];
        prefixed.extend_from_slice(&shared);
        let mut patched = shared.clone();
        for index in (0..patched.len()).step_by(97) {
            patched[index] ^= 0xa5;
        }
        let inputs = vec![shared, prefixed, patched];

        for (preprocess, trim_reference_factor, max_common_match) in configurations {
            let options = DzEncoderOptions {
                use_combuf: true,
                preprocess,
                trim_reference_factor,
                max_common_match,
                ..DzEncoderOptions::default()
            };
            let encoded = compress_archive(&inputs, &options).unwrap();
            let common_bytes = encoded.common_buffer.as_ref().unwrap();
            let common = DzCommonBuffer::new(options.settings, vec![common_bytes.clone()]).unwrap();
            for (input, compressed) in inputs.iter().zip(&encoded.chunks) {
                assert_eq!(
                    decompress_chunk_with_common_buffer(
                        compressed,
                        input.len(),
                        options.settings,
                        Some(&common),
                    )
                    .unwrap(),
                    *input
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 48);
}

#[test]
fn combuf_recent_bases_round_trip_with_and_without_reference_trimming() {
    let mut state = 0xa341_316cu32;
    let source: Vec<u8> = (0..4096)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    let mut target = Vec::new();
    for index in 0..24usize {
        target.extend_from_slice(&[0xd3, index as u8, 0x71, (index as u8).wrapping_mul(29)]);
        let start = index * 137 % (source.len() - 48);
        target.extend_from_slice(&source[start..start + 48]);
    }
    let inputs = vec![source, target];

    for trim_reference_factor in [0, 20] {
        let options = DzEncoderOptions {
            use_combuf: true,
            preprocess: false,
            trim_reference_factor,
            max_common_match: 48,
            ..DzEncoderOptions::default()
        };
        let encoded = compress_archive(&inputs, &options).unwrap();
        let common_bytes = encoded.common_buffer.as_ref().unwrap();
        assert!(!common_bytes.is_empty());
        let common = DzCommonBuffer::new(options.settings, vec![common_bytes.clone()]).unwrap();

        for (input, compressed) in inputs.iter().zip(&encoded.chunks) {
            let decoded = decompress_chunk_with_common_buffer(
                compressed,
                input.len(),
                options.settings,
                Some(&common),
            )
            .unwrap();
            assert_eq!(&decoded, input);
        }
    }
}

#[test]
fn tiny_match_tokenization_matches_dzip_original() {
    let data = b"// Some text file...\r\n";
    let settings = RangeSettings::default();
    let expected = [
        0x17, 0x74, 0x23, 0xa8, 0x86, 0x9d, 0x5b, 0x36, 0x1e, 0xf0, 0xdc, 0xde, 0x86, 0x7a, 0xc0,
        0xde, 0xae, 0x88, 0x23, 0xf2, 0x28, 0x85, 0xe9, 0xb4,
    ];
    assert_eq!(compress_chunk(data, settings).unwrap(), expected);
}
