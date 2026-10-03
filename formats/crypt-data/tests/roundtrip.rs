use crypt_data::{
    CryptDataError, DEFAULT_LIMIT, HEADER_SIZE, MAGIC, decrypt, decrypt_with_limit,
    decrypt_wrapped, encrypt, encrypt_with_limit, inspect,
};

#[test]
fn small_payloads_pass_through_without_a_wrapper() {
    let raw = b"small resource";
    assert_eq!(encrypt(raw, b"key").unwrap(), raw);
    assert_eq!(decrypt(raw, b"key").unwrap(), raw);

    let metadata = inspect(raw, DEFAULT_LIMIT).unwrap();
    assert!(!metadata.wrapped);
    assert_eq!(metadata.raw_size, raw.len());
}

#[test]
fn default_limit_roundtrip_matches_the_wire_layout() {
    let raw = (0..600)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let key = b"resource-key";
    let encoded = encrypt(&raw, key).unwrap();

    assert_eq!(&encoded[..MAGIC.len()], &MAGIC);
    assert_eq!(
        u64::from_le_bytes(encoded[MAGIC.len()..HEADER_SIZE].try_into().unwrap()),
        raw.len() as u64
    );
    for index in 0..DEFAULT_LIMIT {
        assert_eq!(
            encoded[HEADER_SIZE + index],
            raw[index] ^ key[index % key.len()]
        );
    }
    assert_eq!(
        &encoded[HEADER_SIZE + DEFAULT_LIMIT..],
        &raw[DEFAULT_LIMIT..]
    );
    assert_eq!(decrypt(&encoded, key).unwrap(), raw);
}

#[test]
fn custom_limit_roundtrips_at_the_boundary() {
    let raw = b"abcdefgh";
    let encoded = encrypt_with_limit(raw, b"xy", raw.len()).unwrap();
    assert_eq!(encoded.len(), HEADER_SIZE + raw.len());
    assert_eq!(decrypt_with_limit(&encoded, b"xy", raw.len()).unwrap(), raw);
}

#[test]
fn malformed_containers_are_rejected() {
    let large_plain = vec![0; DEFAULT_LIMIT + HEADER_SIZE];
    assert_eq!(
        decrypt(&large_plain, b"key").unwrap_err(),
        CryptDataError::InvalidMagic
    );

    let raw = vec![7; DEFAULT_LIMIT];
    let mut truncated = encrypt(&raw, b"key").unwrap();
    truncated.pop();
    assert_eq!(
        decrypt_wrapped(&truncated, b"key", DEFAULT_LIMIT).unwrap_err(),
        CryptDataError::PayloadSizeMismatch {
            expected: raw.len(),
            actual: raw.len() - 1,
        }
    );
}

#[test]
fn empty_key_is_an_error_instead_of_a_panic() {
    let raw = vec![0; DEFAULT_LIMIT];
    assert_eq!(encrypt(&raw, b"").unwrap_err(), CryptDataError::EmptyKey);
}

#[test]
fn zero_limit_wraps_without_xor_or_a_key() {
    let raw = b"plain";
    let encoded = encrypt_with_limit(raw, b"", 0).unwrap();
    assert_eq!(&encoded[HEADER_SIZE..], raw);
    assert_eq!(decrypt_with_limit(&encoded, b"", 0).unwrap(), raw);
}
