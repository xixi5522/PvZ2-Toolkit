use std::io::{Cursor, Read, Write};

use flate2::Compression;
use flate2::write::ZlibEncoder;
#[cfg(feature = "tv")]
use pak_archive::ZipCompression;
use pak_archive::{
    CompatibilityMode, DecodeOptions, EncodeOptions, PakArchive, PakCompression,
    PakDirectoryLayout, PakEntry, PakFormat, PakFormatHint, PakPath, PakReader, PakTimestamp,
    PathSeparator, from_bytes, from_bytes_with_options, to_bytes, to_seekable_writer,
};

const MAGIC: u32 = 0xBAC0_4AC0;

fn archive(format: PakFormat) -> PakArchive {
    let first = PakEntry::new(
        "properties\\example.rton",
        b"repeated data repeated data repeated data repeated data",
    );
    let second = PakEntry::new("images\\icon.ptx", vec![0, 1, 2, 3, 4, 5, 6, 7]);
    let entries = if matches!(format, PakFormat::Flat { .. }) {
        vec![
            first.with_popcap_time(129_146_222_018_596_744),
            second.with_popcap_time(42),
        ]
    } else {
        vec![first, second]
    };
    PakArchive::new(
        EncodeOptions {
            format,
            path_separator: PathSeparator::Backslash,
            ..EncodeOptions::default()
        },
        entries,
    )
    .unwrap()
}

fn assert_entries_equal(expected: &PakArchive, actual: &PakArchive) {
    assert_eq!(actual.entries().len(), expected.entries().len());
    for (actual, expected) in actual.entries().iter().zip(expected.entries()) {
        assert_eq!(actual.path(), expected.path());
        assert_eq!(actual.data(), expected.data());
        if !matches!(actual.timestamp(), PakTimestamp::ZipMsDos { .. }) {
            assert_eq!(actual.timestamp(), expected.timestamp());
        }
    }
}

#[test]
fn pc_uncompressed_round_trip_preserves_metadata_and_data() {
    let archive = archive(PakFormat::pc(PakCompression::None));
    let bytes = to_bytes(&archive).unwrap();
    assert_eq!(
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        MAGIC ^ 0xF7F7_F7F7
    );
    let decoded = from_bytes(&bytes).unwrap();
    assert_eq!(decoded.options().format, archive.options().format);
    assert_entries_equal(&archive, &decoded);
    assert_eq!(to_bytes(&decoded).unwrap(), bytes);
}

#[test]
fn plain_container_matches_the_twinning_layout() {
    let archive = archive(PakFormat::plain(PakCompression::Zlib));
    let bytes = to_bytes(&archive).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), MAGIC);
    let decoded = from_bytes(&bytes).unwrap();
    assert_eq!(decoded.options().format, archive.options().format);
    assert_entries_equal(&archive, &decoded);
    assert_eq!(
        decoded.source_info().unwrap().directory_layout,
        Some(PakDirectoryLayout::CanonicalZlib)
    );
}

#[test]
fn canonical_zlib_directory_uses_twinning_size_order() {
    let archive = archive(PakFormat::pc(PakCompression::Zlib));
    let bytes = to_bytes(&archive).unwrap();
    let decoded = from_bytes(&bytes).unwrap();
    assert_entries_equal(&archive, &decoded);

    let decrypted = bytes.iter().map(|byte| byte ^ 0xF7).collect::<Vec<_>>();
    let path_length = decrypted[9] as usize;
    let size_offset = 10 + path_length;
    let stored = u32::from_le_bytes(decrypted[size_offset..size_offset + 4].try_into().unwrap());
    let original = u32::from_le_bytes(
        decrypted[size_offset + 4..size_offset + 8]
            .try_into()
            .unwrap(),
    );
    assert!(stored < original);
    assert_eq!(original as usize, archive.entries()[0].data().len());
}

#[test]
fn decoder_accepts_and_normalizes_legacy_toolkit_size_fields() {
    let path = b"legacy/file.bin";
    let data = b"legacy legacy legacy legacy legacy";
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(0);
    bytes.push(path.len() as u8);
    bytes.extend_from_slice(path);
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&7_u64.to_le_bytes());
    bytes.push(0x80);
    bytes.extend_from_slice(&compressed);
    for byte in &mut bytes {
        *byte ^= 0xF7;
    }

    let exact = PakFormat::pc(PakCompression::Zlib);
    let decoded = from_bytes_with_options(
        &bytes,
        DecodeOptions {
            format_hint: PakFormatHint::Exact(exact),
            ..DecodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(decoded.options().format, exact);
    assert_eq!(decoded.entries()[0].path(), &PakPath::from(path.to_vec()));
    assert_eq!(decoded.entries()[0].timestamp(), PakTimestamp::PopCap(7));
    assert_eq!(decoded.entries()[0].data(), data);
    assert_eq!(
        decoded.source_info().unwrap().directory_layout,
        Some(PakDirectoryLayout::LegacyToolkitZlib)
    );

    let canonical = to_bytes(&decoded).unwrap();
    assert_ne!(canonical, bytes);
    assert_eq!(
        from_bytes(&canonical)
            .unwrap()
            .source_info()
            .unwrap()
            .directory_layout,
        Some(PakDirectoryLayout::CanonicalZlib)
    );

    let strict = from_bytes_with_options(
        &bytes,
        DecodeOptions {
            format_hint: PakFormatHint::Exact(exact),
            compatibility: CompatibilityMode::CanonicalOnly,
            ..DecodeOptions::default()
        },
    );
    assert!(strict.is_err());
}

#[test]
fn xbox_payloads_are_aligned_and_round_trip() {
    let archive = archive(PakFormat::xbox360(PakCompression::Zlib));
    let bytes = to_bytes(&archive).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), MAGIC);
    let decoded = from_bytes(&bytes).unwrap();
    assert_eq!(decoded.options().format, archive.options().format);
    assert_entries_equal(&archive, &decoded);
}

#[test]
fn indexed_reader_materializes_only_requested_entries() {
    let archive = archive(PakFormat::pc(PakCompression::Zlib));
    let bytes = to_bytes(&archive).unwrap();
    let mut reader = PakReader::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.entries().len(), 2);
    assert_eq!(reader.find_entry("images\\icon.ptx"), Some(1));
    assert_eq!(reader.read_entry(1).unwrap(), archive.entries()[1].data());

    let mut stream = reader.open_entry(0).unwrap();
    let mut chunks = Vec::new();
    let mut buffer = [0_u8; 3];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        chunks.extend_from_slice(&buffer[..read]);
    }
    assert_eq!(chunks, archive.entries()[0].data());
}

#[test]
fn seekable_writer_preserves_an_existing_prefix() {
    let archive = archive(PakFormat::pc(PakCompression::Zlib));
    let expected = to_bytes(&archive).unwrap();
    let mut cursor = Cursor::new(b"PREFIX".to_vec());
    cursor.set_position(6);
    to_seekable_writer(&archive, &mut cursor).unwrap();
    assert_eq!(&cursor.get_ref()[..6], b"PREFIX");
    assert_eq!(&cursor.get_ref()[6..], expected);
}

#[cfg(feature = "tv")]
#[test]
fn tv_zip_round_trip_preserves_payloads_and_zip_metadata() {
    let mut archive = archive(PakFormat::TvZip {
        compression: ZipCompression::Deflated,
    });
    archive
        .set_entry_timestamp(
            0,
            PakTimestamp::ZipMsDos {
                date: 0x4D71,
                time: 0x54CF,
            },
        )
        .unwrap();
    let mut metadata = pak_archive::ZipEntryMetadata::new(ZipCompression::Deflated);
    metadata.set_unix_mode(Some(0o644));
    metadata.set_comment("resource");
    archive.set_entry_zip_metadata(0, Some(metadata)).unwrap();
    let bytes = to_bytes(&archive).unwrap();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    let decoded = from_bytes(&bytes).unwrap();
    assert_eq!(decoded.options().format, archive.options().format);
    assert_entries_equal(&archive, &decoded);
    assert_eq!(
        decoded.entries()[0].zip_metadata().unwrap().comment(),
        "resource"
    );
    assert_eq!(
        decoded.entries()[0].zip_metadata().unwrap().compression(),
        ZipCompression::Deflated
    );
}

#[test]
fn raw_non_utf8_flat_paths_round_trip() {
    let path = PakPath::from(vec![b'a', b'/', 0xFF, b'.', b'b', b'i', b'n']);
    let archive = PakArchive::new(
        EncodeOptions {
            format: PakFormat::plain(PakCompression::None),
            ..EncodeOptions::default()
        },
        vec![PakEntry::new(path.clone(), b"payload")],
    )
    .unwrap();
    let decoded = from_bytes(&to_bytes(&archive).unwrap()).unwrap();
    assert_eq!(decoded.entries()[0].path(), &path);
    assert_eq!(decoded.entries()[0].data(), b"payload");
}

#[test]
fn committed_canonical_plain_fixture_is_byte_exact() {
    const FIXTURE: &[u8] = &[
        0xC0, 0x4A, 0xC0, 0xBA, // magic
        0, 0, 0, 0, // version
        0, 1, b'a', // marker and path
        3, 0, 0, 0, // stored size
        7, 0, 0, 0, 0, 0, 0, 0, // time
        0x80, b'x', b'y', b'z', // end marker and payload
    ];
    let archive = from_bytes(FIXTURE).unwrap();
    assert_eq!(
        archive.options().format,
        PakFormat::plain(PakCompression::None)
    );
    assert_eq!(archive.entries()[0].path(), &PakPath::from("a"));
    assert_eq!(archive.entries()[0].timestamp(), PakTimestamp::PopCap(7));
    assert_eq!(archive.entries()[0].data(), b"xyz");
    assert_eq!(to_bytes(&archive).unwrap(), FIXTURE);
}

#[cfg(feature = "serde")]
#[test]
fn serde_preserves_utf8_and_raw_paths() {
    let utf8 = PakPath::from("images/icon.ptx");
    let raw = PakPath::from(vec![b'x', 0xFF]);
    let utf8_json = serde_json::to_string(&utf8).unwrap();
    let raw_json = serde_json::to_string(&raw).unwrap();
    assert_eq!(serde_json::from_str::<PakPath>(&utf8_json).unwrap(), utf8);
    assert_eq!(serde_json::from_str::<PakPath>(&raw_json).unwrap(), raw);
}
