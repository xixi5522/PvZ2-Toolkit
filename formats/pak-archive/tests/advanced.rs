use std::io::{Cursor, Read};
use std::ops::ControlFlow;
use std::time::{SystemTime, UNIX_EPOCH};

use pak_archive::{
    DecodeLimits, DecodeOptions, EncodeOptions, MetadataConversion, PakArchive, PakCompression,
    PakEntry, PakFormat, PakReader, PakTimestamp, PakWriteEntry, PakWriter, WritePhase,
    ZipCompression, from_bytes, to_bytes, to_path_atomic,
};
#[cfg(feature = "tv")]
use pak_archive::{PakEntryKind, PathSeparator, ZipEntryMetadata, ZipExtraField};

fn flat_archive() -> PakArchive {
    PakArchive::new(
        EncodeOptions {
            format: PakFormat::pc(PakCompression::Zlib),
            ..EncodeOptions::default()
        },
        vec![
            PakEntry::new("a.bin", b"alpha alpha alpha").with_popcap_time(7),
            PakEntry::new("nested/b.bin", b"beta"),
        ],
    )
    .unwrap()
}

#[test]
fn source_writer_streams_entries_and_reports_progress() {
    let data = vec![0xA5; 256 * 1024];
    let mut plan = PakWriter::new(EncodeOptions {
        format: PakFormat::plain(PakCompression::Zlib),
        ..EncodeOptions::default()
    });
    plan.push_entry(
        PakWriteEntry::new("large.bin", data.len() as u64, Cursor::new(&data)).with_popcap_time(19),
    )
    .unwrap();
    let mut progress_samples = 0;
    let mut output = Cursor::new(Vec::new());
    let report = plan
        .write_seekable_with_progress(&mut output, |progress| {
            if progress.phase == WritePhase::Entry {
                progress_samples += 1;
            }
            ControlFlow::Continue(())
        })
        .unwrap();

    assert!(progress_samples >= 2);
    assert_eq!(report.entries_written, 1);
    assert_eq!(report.input_bytes, data.len() as u64);
    assert_eq!(report.bytes_written, output.get_ref().len() as u64);
    let decoded = from_bytes(output.get_ref()).unwrap();
    assert_eq!(decoded.entries()[0].data(), data);
}

#[test]
fn source_writer_rejects_short_and_long_readers() {
    for (declared, source) in [(4, b"abc".as_slice()), (2, b"abc".as_slice())] {
        let mut plan = PakWriter::new(EncodeOptions::default());
        plan.push_entry(PakWriteEntry::new("x", declared, Cursor::new(source)))
            .unwrap();
        assert!(plan.write_seekable(&mut Cursor::new(Vec::new())).is_err());
    }
}

#[test]
fn progress_callback_can_cancel_before_output_is_mutated() {
    let mut plan = PakWriter::new(EncodeOptions::default());
    plan.push_entry(PakWriteEntry::new("x", 3, Cursor::new(b"abc")))
        .unwrap();
    let mut output = Cursor::new(Vec::new());
    let result = plan.write_seekable_with_progress(&mut output, |_| ControlFlow::Break(()));
    assert!(result.is_err());
    assert!(output.get_ref().is_empty());
}

#[test]
fn partial_entry_reads_can_be_explicitly_finished_and_verified() {
    let bytes = to_bytes(&flat_archive()).unwrap();
    let mut reader = PakReader::new(Cursor::new(bytes)).unwrap();
    let mut entry = reader.open_entry(0).unwrap();
    let mut prefix = [0_u8; 2];
    entry.read_exact(&mut prefix).unwrap();
    assert!(!entry.is_verified());
    entry.finish().unwrap();
}

#[test]
fn bounded_range_reader_supports_embedded_archives() {
    let bytes = to_bytes(&flat_archive()).unwrap();
    let mut container = b"PREFIX".to_vec();
    container.extend_from_slice(&bytes);
    container.extend_from_slice(b"SUFFIX");
    let mut reader = PakReader::from_range(Cursor::new(container), 6, bytes.len() as u64).unwrap();
    assert_eq!(reader.read_entry(1).unwrap(), b"beta");
}

#[test]
fn detached_index_can_be_attached_to_an_independent_handle() {
    let bytes = to_bytes(&flat_archive()).unwrap();
    let reader = PakReader::new(Cursor::new(bytes.clone())).unwrap();
    let index = reader.index_snapshot();
    assert_eq!(index.find_entry("nested/b.bin"), Some(1));
    let mut second = index.attach(Cursor::new(bytes)).unwrap();
    assert_eq!(second.read_entry(0).unwrap(), b"alpha alpha alpha");
}

#[test]
fn archive_edits_keep_indexes_and_metadata_invariants_consistent() {
    let mut archive = flat_archive();
    assert_eq!(archive.find_entry("a.bin"), Some(0));
    archive.rename_entry(0, "renamed.bin").unwrap();
    assert_eq!(archive.find_entry("a.bin"), None);
    assert_eq!(archive.find_entry("renamed.bin"), Some(0));
    assert!(
        archive
            .set_format(
                PakFormat::TvZip {
                    compression: ZipCompression::Deflated,
                },
                MetadataConversion::RejectLossy,
            )
            .is_err()
    );
    let changes = archive
        .set_format(
            PakFormat::TvZip {
                compression: ZipCompression::Deflated,
            },
            MetadataConversion::Normalize,
        )
        .unwrap();
    assert!(!changes.is_empty());
    assert_eq!(archive.entries()[0].timestamp(), PakTimestamp::None);
    archive.validate().unwrap();
}

#[test]
fn directory_and_path_byte_limits_are_enforced_during_detection() {
    let bytes = to_bytes(&flat_archive()).unwrap();
    let options = DecodeOptions {
        limits: DecodeLimits {
            max_total_path_bytes: 2,
            ..DecodeLimits::default()
        },
        ..DecodeOptions::default()
    };
    assert!(PakReader::with_options(Cursor::new(bytes), options).is_err());
}

#[cfg(feature = "tv")]
#[test]
fn tv_zip_round_trips_raw_names_directories_comments_and_extra_fields() {
    let raw_path = pak_archive::PakPath::from(vec![b'r', b'a', b'w', b'/', 0xFF, b'.', b'b']);
    let mut metadata = ZipEntryMetadata::new(ZipCompression::Deflated);
    metadata.set_unix_mode(Some(0o640));
    metadata.set_comment("entry comment");
    metadata.add_extra_field(ZipExtraField::new(0xCAFE, [1, 2, 3, 4], false));
    let file = PakEntry::new(raw_path.clone(), b"payload")
        .with_zip_metadata(Some((0x4D71, 0x54CF)), metadata);
    let mut archive = PakArchive::new(
        EncodeOptions {
            format: PakFormat::TvZip {
                compression: ZipCompression::Deflated,
            },
            path_separator: PathSeparator::Preserve,
            ..EncodeOptions::default()
        },
        vec![
            PakEntry::directory("raw"),
            file,
            PakEntry::symlink("raw/link", b"../target.bin"),
        ],
    )
    .unwrap();
    archive
        .set_zip_comment(vec![0xFF, 0, b'P', b'A', b'K'])
        .unwrap();

    let decoded = from_bytes(&to_bytes(&archive).unwrap()).unwrap();
    assert_eq!(decoded.zip_comment(), [0xFF, 0, b'P', b'A', b'K']);
    assert_eq!(decoded.entries()[0].kind(), PakEntryKind::Directory);
    assert_eq!(decoded.entries()[1].path(), &raw_path);
    assert_eq!(decoded.entries()[2].kind(), PakEntryKind::Symlink);
    assert_eq!(decoded.entries()[2].data(), b"../target.bin");
    let decoded_metadata = decoded.entries()[1].zip_metadata().unwrap();
    assert_eq!(decoded_metadata.comment(), "entry comment");
    assert!(
        decoded_metadata
            .extra_fields()
            .iter()
            .any(|field| { field.header_id() == 0xCAFE && field.data() == [1, 2, 3, 4] })
    );

    let second = from_bytes(&to_bytes(&decoded).unwrap()).unwrap();
    assert_eq!(second.entries()[1].path(), &raw_path);

    let limited = DecodeOptions {
        limits: DecodeLimits {
            max_directory_bytes: 1,
            ..DecodeLimits::default()
        },
        ..DecodeOptions::default()
    };
    assert!(PakReader::with_options(Cursor::new(to_bytes(&archive).unwrap()), limited).is_err());
}

#[cfg(feature = "tv")]
#[test]
fn tv_zip_empty_and_zip64_entry_count_archives_reopen() {
    let options = EncodeOptions {
        format: PakFormat::TvZip {
            compression: ZipCompression::Stored,
        },
        ..EncodeOptions::default()
    };
    let empty = PakArchive::empty(options);
    assert!(from_bytes(&to_bytes(&empty).unwrap()).unwrap().is_empty());

    let entries = (0..=u16::MAX)
        .map(|index| PakEntry::new(format!("f/{index}"), Vec::new()))
        .collect();
    let archive = PakArchive::new(options, entries).unwrap();
    let bytes = to_bytes(&archive).unwrap();
    let reader = PakReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.entries().len(), u16::MAX as usize + 1);
    assert_eq!(reader.find_entry("f/65535"), Some(u16::MAX as usize));
}

#[test]
fn atomic_path_write_replaces_only_after_success() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pak-archive-atomic-{}-{unique}.pak",
        std::process::id()
    ));
    let report = to_path_atomic(&flat_archive(), &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(report.bytes_written, bytes.len() as u64);
    assert_eq!(from_bytes(&bytes).unwrap().len(), 2);
    std::fs::remove_file(path).unwrap();
}

#[cfg(feature = "serde")]
#[test]
fn serde_rebuilds_the_archive_index_lazily_and_writer_revalidates_state() {
    let archive = flat_archive();
    let json = serde_json::to_string(&archive).unwrap();
    let decoded: PakArchive = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.find_entry("nested/b.bin"), Some(1));
    decoded.validate().unwrap();
}
