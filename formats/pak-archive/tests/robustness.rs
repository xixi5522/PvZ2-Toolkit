use pak_archive::{
    DecodeLimits, DecodeOptions, EncodeOptions, PakArchive, PakCompression, PakEntry, PakFormat,
    PakPath, PakPathError, PakReader, PathSeparator, ValidationMode, from_bytes,
    from_bytes_with_options, to_bytes,
};
use std::io::Cursor;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

#[test]
fn malformed_and_truncated_inputs_are_rejected() {
    assert!(from_bytes(b"").is_err());
    assert!(from_bytes(b"NOPE").is_err());
    assert!(from_bytes(&0xBAC0_4AC0_u32.to_le_bytes()).is_err());
}

#[test]
fn archive_and_entry_count_limits_are_enforced() {
    let archive = PakArchive::new(
        EncodeOptions::default(),
        vec![PakEntry::new("file.bin", [1, 2, 3])],
    )
    .unwrap();
    let bytes = to_bytes(&archive).unwrap();
    let options = DecodeOptions {
        limits: DecodeLimits {
            max_entries: 0,
            ..DecodeLimits::default()
        },
        ..DecodeOptions::default()
    };
    assert!(from_bytes_with_options(&bytes, options).is_err());

    let options = DecodeOptions {
        limits: DecodeLimits {
            max_archive_bytes: bytes.len() as u64 - 1,
            ..DecodeLimits::default()
        },
        ..DecodeOptions::default()
    };
    assert!(PakReader::with_options(Cursor::new(&bytes), options).is_err());
}

#[test]
fn writer_rejects_paths_larger_than_the_wire_field() {
    let archive = PakArchive::new(
        EncodeOptions {
            format: PakFormat::pc(PakCompression::None),
            path_separator: PathSeparator::Preserve,
            ..EncodeOptions::default()
        },
        vec![PakEntry::new("x".repeat(256), Vec::new())],
    )
    .unwrap();
    assert!(to_bytes(&archive).is_err());
}

#[test]
fn safe_path_conversion_rejects_traversal_and_absolute_paths() {
    assert_eq!(
        PakPath::from("../outside").to_safe_relative_path(),
        Err(PakPathError::Traversal)
    );
    assert_eq!(
        PakPath::from("C:\\outside").to_safe_relative_path(),
        Err(PakPathError::Absolute)
    );
    assert_eq!(
        PakPath::from("/outside").to_safe_relative_path(),
        Err(PakPathError::Absolute)
    );
    assert_eq!(
        PakPath::from("folder\\file.bin")
            .to_safe_relative_path()
            .unwrap(),
        std::path::PathBuf::from("folder/file.bin")
    );
}

#[test]
fn strict_xbox_validation_rejects_nonzero_padding() {
    let archive = PakArchive::new(
        EncodeOptions {
            format: PakFormat::xbox360(PakCompression::None),
            ..EncodeOptions::default()
        },
        vec![PakEntry::new("file.bin", b"payload")],
    )
    .unwrap();
    let mut bytes = to_bytes(&archive).unwrap();
    let directory_end = 8 + 1 + 1 + "file.bin".len() + 4 + 8 + 1;
    bytes[directory_end + 2] = 1;
    assert!(from_bytes(&bytes).is_err());

    let permissive = from_bytes_with_options(
        &bytes,
        DecodeOptions {
            validation: ValidationMode::Permissive,
            ..DecodeOptions::default()
        },
    );
    assert!(permissive.is_ok());
}

#[test]
fn duplicate_paths_remain_addressable() {
    let archive = PakArchive::new(
        EncodeOptions::default(),
        vec![
            PakEntry::new("same.bin", b"first"),
            PakEntry::new("same.bin", b"second"),
        ],
    )
    .unwrap();
    let bytes = to_bytes(&archive).unwrap();
    let reader = PakReader::new(Cursor::new(bytes)).unwrap();
    let indices = reader
        .entries_by_path(b"same.bin")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(indices, [0, 1]);
}

#[test]
fn web_limits_are_stricter_than_desktop_defaults() {
    assert!(DecodeLimits::web().max_archive_bytes < DecodeLimits::default().max_archive_bytes);
    assert!(
        DecodeLimits::web().max_total_uncompressed_bytes
            < DecodeLimits::default().max_total_uncompressed_bytes
    );
}

#[test]
fn declared_zlib_sizes_are_enforced_while_streaming() {
    let data = b"this output is deliberately longer than declared";
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0xBAC0_4AC0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 1, b'a']);
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes.push(0x80);
    bytes.extend_from_slice(&compressed);

    assert!(from_bytes(&bytes).is_err());
}
