use bnk_archive::{DecodeLimits, DecodeOptions, ValidationMode, from_bytes_with_options, to_bytes};

#[test]
fn bounded_randomized_inputs_never_panic_or_escape_limits() {
    let options = DecodeOptions {
        limits: DecodeLimits {
            max_file_bytes: 4096,
            max_chunk_bytes: 2048,
            max_chunks: 32,
            max_entries: 128,
            max_string_bytes: 512,
            max_hierarchy_object_bytes: 1024,
            max_hierarchy_fields: 128,
            max_hierarchy_path_bytes: 4096,
        },
        validation: ValidationMode::Permissive,
    };

    let mut state = 0x8f4d_13a7_9b02_6ce1_u64;
    for length in 0..512 {
        let mut bytes = vec![0_u8; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        if length >= 20 {
            bytes[..4].copy_from_slice(b"BKHD");
            bytes[4..8].copy_from_slice(&12_u32.to_le_bytes());
            bytes[8..12].copy_from_slice(&140_u32.to_le_bytes());
        }
        if let Ok(bank) = from_bytes_with_options(&bytes, options) {
            let _ = to_bytes(&bank);
        }
    }
}
