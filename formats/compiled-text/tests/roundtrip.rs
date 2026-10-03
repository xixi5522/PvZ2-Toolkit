use base64::{Engine as _, engine::general_purpose::STANDARD};
use compiled_text::{
    CompiledTextError, DecodeOptions, EncodeOptions, SmfVariant, decode, decode_detailed,
    decode_with_options, encode_with_options, inspect,
};

const SEED: &str = "com_popcap_pvz2_magento_product_2013_05_05";

#[test]
fn compact_header_round_trips() {
    let original = b"Hello from a compact Compiled Text file.";
    let encoded = encode_with_options(original, SEED, EncodeOptions::compact()).unwrap();
    let decoded = decode_detailed(&encoded, SEED, DecodeOptions::default()).unwrap();
    assert_eq!(decoded.data, original);
    assert_eq!(decoded.metadata.variant, SmfVariant::Compact32);
    assert_eq!(decoded.metadata.decoded_size, original.len() as u64);
}

#[test]
fn extended_header_is_detected_without_a_decode_flag() {
    let original = b"Extended Compiled Text payload";
    let encoded = encode_with_options(original, SEED, EncodeOptions::extended()).unwrap();
    let decoded = decode_detailed(&encoded, SEED, DecodeOptions::default()).unwrap();
    assert_eq!(decoded.data, original);
    assert_eq!(decoded.metadata.variant, SmfVariant::Extended64);
}

#[test]
fn accepts_wrapped_base64() {
    let encoded = encode_with_options(b"wrapped", SEED, EncodeOptions::compact()).unwrap();
    let wrapped = encoded
        .chunks(12)
        .flat_map(|chunk| chunk.iter().copied().chain(*b"\n"))
        .collect::<Vec<_>>();
    assert_eq!(decode(&wrapped, SEED).unwrap(), b"wrapped");
}

#[test]
fn rejects_unaligned_ciphertext_before_decryption() {
    let malformed = STANDARD.encode([1_u8, 2, 3]);
    assert!(matches!(
        decode(malformed.as_bytes(), SEED),
        Err(CompiledTextError::CiphertextAlignment {
            actual: 3,
            block_size: 24
        })
    ));
}

#[test]
fn checks_declared_size_before_inflating() {
    let encoded = encode_with_options(b"too large", SEED, EncodeOptions::compact()).unwrap();
    let options = DecodeOptions {
        max_output_size: 4,
        ..DecodeOptions::default()
    };
    assert!(matches!(
        inspect(&encoded, SEED, options),
        Err(CompiledTextError::OutputLimitExceeded {
            declared: 9,
            limit: 4
        })
    ));
}

#[test]
fn strict_base64_mode_rejects_whitespace() {
    let encoded = encode_with_options(b"strict", SEED, EncodeOptions::compact()).unwrap();
    let mut wrapped = encoded;
    wrapped.insert(4, b'\n');
    let options = DecodeOptions {
        allow_base64_whitespace: false,
        ..DecodeOptions::default()
    };
    assert!(matches!(
        decode_with_options(&wrapped, SEED, options),
        Err(CompiledTextError::Base64(_))
    ));
}

#[test]
fn wrong_seed_cannot_produce_a_valid_container() {
    let encoded = encode_with_options(b"secret", SEED, EncodeOptions::compact()).unwrap();
    assert!(decode(&encoded, "wrong-seed").is_err());
}
