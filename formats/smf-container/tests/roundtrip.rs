use std::io::Cursor;

use smf_container::{
    EncodeOptions, SMF_MAGIC_BYTES, SmfError, SmfVariant, decode, encode, inspect, tag_contents,
};

fn fixture() -> Vec<u8> {
    (0_u32..16_384)
        .flat_map(|value| value.rotate_left(7).to_le_bytes())
        .collect()
}

#[test]
fn compact_and_extended_headers_round_trip() {
    let source = fixture();
    for variant in [SmfVariant::Compact32, SmfVariant::Extended64] {
        let encoded = encode(&source, EncodeOptions::new(variant, 9)).unwrap();
        assert_eq!(&encoded[..4], &SMF_MAGIC_BYTES);
        let metadata = inspect(&mut Cursor::new(&encoded)).unwrap();
        assert_eq!(metadata.variant, variant);
        assert_eq!(metadata.uncompressed_size, source.len() as u64);
        assert_eq!(metadata.total_size, encoded.len() as u64);
        assert_eq!(decode(Cursor::new(encoded)).unwrap(), source);
    }
}

#[test]
fn empty_payload_is_not_confused_with_the_extended_header() {
    for variant in [SmfVariant::Compact32, SmfVariant::Extended64] {
        let encoded = encode(&[], EncodeOptions::new(variant, 6)).unwrap();
        let metadata = inspect(&mut Cursor::new(&encoded)).unwrap();
        assert_eq!(metadata.variant, variant);
        assert_eq!(decode(Cursor::new(encoded)).unwrap(), Vec::<u8>::new());
    }
}

#[test]
fn invalid_magic_and_size_mismatch_are_reported() {
    let mut encoded = encode(b"payload", EncodeOptions::compact()).unwrap();
    encoded[0] ^= 0xFF;
    assert!(matches!(
        decode(Cursor::new(encoded)),
        Err(SmfError::InvalidMagic { .. })
    ));

    let mut encoded = encode(b"payload", EncodeOptions::compact()).unwrap();
    encoded[4..8].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        decode(Cursor::new(encoded)),
        Err(SmfError::SizeMismatch {
            declared: 99,
            actual: 7
        })
    ));
}

#[test]
fn tag_sidecar_uses_uppercase_md5_and_crlf() {
    assert_eq!(tag_contents(b"abc"), "900150983CD24FB0D6963F7D28E17F72\r\n");
}
