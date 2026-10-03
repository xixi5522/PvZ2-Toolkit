use dzip::raw::{
    CHUNK_COMBUF, CHUNK_COPYCOMP, CHUNK_DZ, CHUNK_JPEG, CHUNK_LZMA, CHUNK_MP3, CHUNK_RANDOMACCESS,
    CHUNK_ZERO, CHUNK_ZLIB,
};
use dzip::reader::DzipReader;
use dzip::volume::MemoryVolumeSource;
use dzip::{
    Archive, ArchiveBuilder, ArchiveImage, Codec, Compression, EntryOptions, ExtractOptions,
    MemoryVolumeSink, PackOptions, RawArchive, ReadOptions,
};
use std::io::Cursor;

#[test]
fn codecs_are_distinct_from_storage_strategies() {
    assert_eq!(Compression::Copy.codec(), None);
    assert_eq!(Compression::Zero.codec(), None);
    assert_eq!(Compression::Bzip.codec(), Some(Codec::Bzip));
    assert_eq!(Compression::Zlib.codec(), Some(Codec::Zlib));
    assert_eq!(Compression::Lzma.codec(), Some(Codec::Lzma));
    assert_eq!(Compression::Dz.codec(), Some(Codec::Dz));
    assert_eq!("bzip2".parse(), Ok(Compression::Bzip));
    assert!("mp3".parse::<Compression>().is_err());
}
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn public_builder_and_archive_round_trip_all_codecs() {
    let mut builder = ArchiveBuilder::new();
    let fixtures = [
        ("copy.bin", Compression::Copy, b"copy payload".repeat(10)),
        ("empty-bzip.bin", Compression::Bzip, Vec::new()),
        ("bzip.bin", Compression::Bzip, b"bzip payload".repeat(100)),
        ("zlib.bin", Compression::Zlib, b"zlib payload".repeat(100)),
        ("lzma.bin", Compression::Lzma, b"lzma payload".repeat(100)),
        ("dz.bin", Compression::Dz, b"native dz payload".repeat(100)),
        ("zero.bin", Compression::Zero, vec![0; 1024]),
    ];
    for (path, codec, bytes) in &fixtures {
        builder
            .add_bytes(path, bytes.clone(), EntryOptions::new().compression(*codec))
            .unwrap();
    }

    let mut sink = MemoryVolumeSink::default();
    let report = builder.write_to_sink(&mut sink).unwrap();
    assert_eq!(report.entries, fixtures.len());
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();

    for (path, _, expected) in fixtures {
        assert_eq!(archive.read_entry_by_path(path).unwrap(), expected);
    }
}

#[test]
fn raw_flags_select_the_original_packer_coder() {
    let fixtures = [
        (
            "dz-mp3.bin",
            CHUNK_DZ | CHUNK_MP3,
            b"DZ data carrying an MP3 hint".repeat(100),
        ),
        (
            "zlib-jpeg.bin",
            CHUNK_ZLIB | CHUNK_JPEG | CHUNK_RANDOMACCESS,
            b"Zlib data carrying JPEG and random-access metadata".repeat(100),
        ),
        (
            "copy-jpeg.bin",
            CHUNK_COPYCOMP | CHUNK_JPEG | CHUNK_RANDOMACCESS,
            b"raw copy data".repeat(100),
        ),
    ];
    let mut builder = ArchiveBuilder::new();
    for (path, flags, data) in &fixtures {
        builder
            .add_bytes(
                path,
                data.clone(),
                EntryOptions::new()
                    .compression(Compression::Copy)
                    .raw_flags(*flags),
            )
            .unwrap();
    }

    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();

    for (path, _, expected) in fixtures {
        assert_eq!(archive.read_entry_by_path(path).unwrap(), expected);
    }
}

#[test]
fn combined_dz_flags_preserve_original_writer_reader_asymmetry() {
    let data = b"combined flags retain every registered decoder".repeat(100);
    let mut builder = ArchiveBuilder::new();
    builder
        .add_bytes(
            "combined.bin",
            data.clone(),
            EntryOptions::new().raw_flags(CHUNK_ZLIB | CHUNK_DZ),
        )
        .unwrap();

    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut reader = DzipReader::new(Cursor::new(main.clone()));
    let settings = reader.read_archive_settings().unwrap();
    reader
        .read_raw_strings(
            usize::from(settings.num_user_files) + usize::from(settings.num_directories) - 1,
        )
        .unwrap();
    reader
        .read_file_chunk_map(usize::from(settings.num_user_files))
        .unwrap();
    let chunk_settings = reader.read_chunk_settings().unwrap();
    let chunks = reader
        .read_chunks(usize::from(chunk_settings.num_chunks))
        .unwrap();
    reader
        .read_file_list(usize::from(
            chunk_settings.num_archive_files.saturating_sub(1),
        ))
        .unwrap();

    // Zlib wins while every DCL bit remains stored. Just like dzip.exe, the
    // writer does not append DZ settings, so its reader rejects this invalid
    // combined form when it sees the retained DZ bit.
    assert_eq!(chunks[0].flags, CHUNK_ZLIB | CHUNK_DZ);
    assert_eq!(u64::from(chunks[0].offset), reader.position().unwrap());
    assert!(Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).is_err());
}

#[test]
fn metadata_only_raw_flags_do_not_create_copy_chunks() {
    let mut builder = ArchiveBuilder::new();
    builder
        .add_bytes(
            "hint-only.bin",
            b"not implicitly copied".to_vec(),
            EntryOptions::new().raw_flags(CHUNK_MP3 | CHUNK_RANDOMACCESS),
        )
        .unwrap();

    assert!(
        builder
            .write_to_sink(&mut MemoryVolumeSink::default())
            .is_err()
    );
}

#[test]
fn split_volumes_and_segmented_entries_use_public_api() {
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "main1.dz".to_string()],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "joined.bin",
            b"first ".to_vec(),
            EntryOptions::new().compression(Compression::Copy).volume(1),
        )
        .unwrap()
        .add_bytes(
            "joined.bin",
            b"second".to_vec(),
            EntryOptions::new().compression(Compression::Copy).volume(1),
        )
        .unwrap();

    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let mut volumes = sink.into_volumes();
    let main = volumes.remove(&0).unwrap();
    let auxiliary = volumes.remove(&1).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([(1, auxiliary)]))
            .unwrap();
    assert_eq!(
        archive.read_entry_by_path("joined.bin").unwrap(),
        b"first second"
    );
    let entry = archive.find_entry("joined.bin").unwrap();
    assert_eq!(entry.raw_path().as_bytes(), b"joined.bin");
    assert_eq!(entry.segments().len(), 2);
    assert_eq!(entry.segments()[0].decoded_range(), &(0..6));
    assert_eq!(entry.segments()[1].decoded_range(), &(6..12));
    assert_eq!(
        archive.find_entry_raw(b"JOINED.BIN").unwrap().id(),
        entry.id()
    );
}

#[test]
fn auxiliary_volumes_are_opened_only_when_their_payload_is_read() {
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "unused.dz".to_string()],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "main.bin",
            b"main payload".to_vec(),
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();

    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();
    assert_eq!(archive.entries().len(), 1);
    assert_eq!(
        archive.read_entry_by_path("main.bin").unwrap(),
        b"main payload"
    );
}

#[test]
fn missing_used_volume_fails_on_access_not_archive_open() {
    let data = b"late auxiliary LZMA payload".repeat(100);
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "payload.dz".to_string()],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "payload.bin",
            data,
            EntryOptions::new().compression(Compression::Lzma).volume(1),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();

    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();
    assert_eq!(archive.entries().len(), 1);
    assert!(archive.read_entry_by_path("payload.bin").is_err());
}

#[test]
fn auxiliary_placeholder_lengths_resolve_on_first_access() {
    let data = b"late auxiliary LZMA payload".repeat(100);
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "payload.dz".to_string()],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "payload.bin",
            data.clone(),
            EntryOptions::new().compression(Compression::Lzma).volume(1),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let mut volumes = sink.into_volumes();
    let main = volumes.remove(&0).unwrap();
    let auxiliary = volumes.remove(&1).unwrap();

    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([(1, auxiliary)]))
            .unwrap();
    assert_eq!(
        archive.index().chunk(0).unwrap().compressed_length,
        data.len() as u32
    );
    assert_eq!(archive.read_entry_by_path("payload.bin").unwrap(), data);
    assert!(archive.index().chunk(0).unwrap().compressed_length < data.len() as u32);
}

#[test]
fn original_non_dz_combuf_combinations_are_readable() {
    let fixtures = [
        (
            "copy.bin",
            CHUNK_COPYCOMP | CHUNK_COMBUF,
            b"copy combuf payload".repeat(20),
        ),
        (
            "zlib.bin",
            CHUNK_ZLIB | CHUNK_COMBUF,
            b"zlib combuf payload".repeat(100),
        ),
        (
            "lzma.bin",
            CHUNK_LZMA | CHUNK_COMBUF,
            b"lzma combuf payload".repeat(100),
        ),
    ];
    let mut builder = ArchiveBuilder::new();
    for (path, flags, data) in &fixtures {
        builder
            .add_bytes(path, data.clone(), EntryOptions::new().raw_flags(*flags))
            .unwrap();
    }
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();
    for (path, _, expected) in fixtures {
        assert_eq!(archive.read_entry_by_path(path).unwrap(), expected);
    }
}

#[test]
fn unsafe_dz_combuf_and_unknown_flags_are_rejected() {
    for flags in [CHUNK_DZ | CHUNK_COMBUF, CHUNK_COPYCOMP | 0x8000] {
        let mut builder = ArchiveBuilder::new();
        builder
            .add_bytes(
                "payload.bin",
                b"payload".repeat(100),
                EntryOptions::new().raw_flags(flags),
            )
            .unwrap();
        let mut sink = MemoryVolumeSink::default();
        builder.write_to_sink(&mut sink).unwrap();
        let main = sink.into_volumes().remove(&0).unwrap();
        assert!(
            Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).is_err()
        );
    }
}

#[test]
fn builder_normalizes_windows_archive_paths_on_every_host() {
    let mut builder = ArchiveBuilder::new();
    builder
        .add_bytes(
            r"folder\child.bin",
            b"payload".to_vec(),
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();
    let entry = archive.find_entry("folder/child.bin").unwrap();
    assert_eq!(entry.raw_path().as_bytes(), br"folder\child.bin");

    assert!(
        ArchiveBuilder::new()
            .add_bytes("bad\0name", Vec::new(), EntryOptions::new())
            .is_err()
    );
    let builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["bad\0volume.dz".to_string()],
        ..PackOptions::default()
    });
    assert!(
        builder
            .write_to_sink(&mut MemoryVolumeSink::default())
            .is_err()
    );
}

#[test]
fn archive_image_rewrites_split_volumes_byte_for_byte() {
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "parts/main1.dz".to_string()],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "main.bin",
            b"main image data".to_vec(),
            EntryOptions::new().compression(Compression::Zlib),
        )
        .unwrap()
        .add_bytes(
            "aux.bin",
            b"auxiliary image data".to_vec(),
            EntryOptions::new().compression(Compression::Copy).volume(1),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let mut built = sink.into_volumes();
    let original = vec![built.remove(&0).unwrap(), built.remove(&1).unwrap()];

    let image = ArchiveImage::from_volumes(original.clone()).unwrap();
    assert_eq!(
        image.metadata().volume_files[0].as_bytes(),
        b"parts/main1.dz"
    );
    assert_eq!(image.volume(1).unwrap(), original[1]);

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "dzip-archive-image-{}-{unique}",
        std::process::id()
    ));
    let main_path = output.join("renamed.dz");
    image.write_to_path(&main_path).unwrap();
    assert_eq!(std::fs::read(&main_path).unwrap(), original[0]);
    assert_eq!(
        std::fs::read(output.join("parts/main1.dz")).unwrap(),
        original[1]
    );
    assert_eq!(ArchiveImage::open_path(&main_path).unwrap(), image);
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn raw_chunk_records_remain_distinct_from_resolved_physical_lengths() {
    let data = b"physical length placeholder".repeat(1000);
    let mut builder = ArchiveBuilder::new();
    builder
        .add_bytes(
            "payload.bin",
            data.clone(),
            EntryOptions::new().compression(Compression::Lzma),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();

    assert_eq!(
        archive.index().stored_chunks()[0].compressed_length,
        data.len() as u32
    );
    assert!(archive.index().chunk(0).unwrap().compressed_length < data.len() as u32);
    assert_eq!(
        archive.index().resolved_chunks()[0].physical_length,
        archive.index().chunk(0).unwrap().compressed_length
    );
}

#[test]
fn metadata_limit_includes_auxiliary_volume_names() {
    let long_name = format!("{}.dz", "v".repeat(1024));
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), long_name],
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "payload.bin",
            b"x".to_vec(),
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut options = ReadOptions::default();
    options.limits.max_metadata_bytes = 256;
    assert!(
        Archive::open_with_options(
            Cursor::new(main),
            MemoryVolumeSource::new([(1, Vec::new())]),
            options,
        )
        .is_err()
    );
}

#[test]
fn reusable_memory_sink_starts_each_archive_from_empty_volumes() {
    let mut sink = MemoryVolumeSink::default();
    let mut first = ArchiveBuilder::new();
    first
        .add_bytes(
            "first.bin",
            vec![1; 4096],
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap();
    first.write_to_sink(&mut sink).unwrap();
    let first_len = sink.volume(0).unwrap().len();

    let mut second = ArchiveBuilder::new();
    second
        .add_bytes(
            "second.bin",
            b"small".to_vec(),
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap();
    second.write_to_sink(&mut sink).unwrap();
    assert!(sink.volume(0).unwrap().len() < first_len);

    let main = sink.into_volumes().remove(&0).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();
    assert_eq!(archive.read_entry_by_path("second.bin").unwrap(), b"small");
}

#[test]
fn lossless_metadata_parser_accepts_names_the_semantic_path_api_cannot_map() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x5a52_5444u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&[0xff, 0]);
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0xffffu16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&37u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&CHUNK_COPYCOMP.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(b'x');

    let raw = RawArchive::read_from(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(raw.strings[0].as_bytes(), &[0xff]);
    let mut metadata = Vec::new();
    raw.write_metadata_to(&mut metadata).unwrap();
    assert_eq!(metadata, bytes[..37]);
    assert!(Archive::open_with_volumes(Cursor::new(bytes), MemoryVolumeSource::new([])).is_err());
}

#[test]
fn alignment_applies_once_per_volume_and_zero_uses_the_payload_origin() {
    let mut builder = ArchiveBuilder::with_options(PackOptions {
        volume_names: vec!["main.dz".to_string(), "main1.dz".to_string()],
        alignment: 256,
        ..PackOptions::default()
    });
    builder
        .add_bytes(
            "zero.bin",
            vec![0; 16],
            EntryOptions::new().compression(Compression::Zero),
        )
        .unwrap()
        .add_bytes(
            "main.bin",
            b"main".to_vec(),
            EntryOptions::new().compression(Compression::Copy),
        )
        .unwrap()
        .add_bytes(
            "aux.bin",
            b"abc".to_vec(),
            EntryOptions::new().compression(Compression::Copy).volume(1),
        )
        .unwrap()
        .add_bytes(
            "aux-2.bin",
            b"defg".to_vec(),
            EntryOptions::new().compression(Compression::Copy).volume(1),
        )
        .unwrap();

    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let mut volumes = sink.into_volumes();
    let main = volumes.remove(&0).unwrap();
    let auxiliary = volumes.remove(&1).unwrap();
    let archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([(1, auxiliary)]))
            .unwrap();
    let chunks = archive.index().chunks().collect::<Vec<_>>();

    assert_eq!(chunks[0].flags & CHUNK_ZERO, CHUNK_ZERO);
    assert_ne!(chunks[0].offset, 0);
    assert_eq!(chunks[0].offset % 256, 0);
    assert_eq!(chunks[1].offset, chunks[0].offset);
    assert_eq!(chunks[2].offset, 0);
    assert_eq!(chunks[3].offset, 3);
}

#[test]
fn extraction_is_safe_and_zero_uses_original_behavior() {
    let mut unsafe_builder = ArchiveBuilder::new();
    assert!(
        unsafe_builder
            .add_bytes("../escape.bin", b"x".to_vec(), EntryOptions::new())
            .is_err()
    );

    let mut zero_builder = ArchiveBuilder::new();
    zero_builder
        .add_bytes(
            "not-zero.bin",
            b"x".to_vec(),
            EntryOptions::new().compression(Compression::Zero),
        )
        .unwrap();
    let mut zero_sink = MemoryVolumeSink::default();
    zero_builder.write_to_sink(&mut zero_sink).unwrap();
    let zero_main = zero_sink.into_volumes().remove(&0).unwrap();
    let mut zero_archive =
        Archive::open_with_volumes(Cursor::new(zero_main), MemoryVolumeSource::new([])).unwrap();
    assert_eq!(
        zero_archive.read_entry_by_path("not-zero.bin").unwrap(),
        [0]
    );

    let mut builder = ArchiveBuilder::new();
    builder
        .add_bytes(
            "safe/sub/file.bin",
            b"public extraction".to_vec(),
            EntryOptions::new().compression(Compression::Zlib),
        )
        .unwrap();
    let mut sink = MemoryVolumeSink::default();
    builder.write_to_sink(&mut sink).unwrap();
    let main = sink.into_volumes().remove(&0).unwrap();
    let mut archive =
        Archive::open_with_volumes(Cursor::new(main), MemoryVolumeSource::new([])).unwrap();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output =
        std::env::temp_dir().join(format!("dzip-public-api-{}-{unique}", std::process::id()));
    let report = archive
        .extract_to(&output, ExtractOptions::default())
        .unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(
        std::fs::read(output.join("safe/sub/file.bin")).unwrap(),
        b"public extraction"
    );
    std::fs::remove_dir_all(output).unwrap();
}
