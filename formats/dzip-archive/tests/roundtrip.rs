use dzip::format::*;
use dzip::reader::DzipReader;
use dzip::writer::DzipWriter;
use std::io::Cursor;

#[test]
fn test_roundtrip() {
    let mut buffer = Vec::new();
    let archive_settings = ArchiveSettings {
        header: 0x5A525444,
        num_user_files: 2,
        num_directories: 1,
        version: 0,
    };
    let strings = vec!["file1.txt".to_string(), "file2.txt".to_string()];
    let map = vec![(0, vec![0]), (0, vec![1])];
    let chunk_settings = ChunkSettings {
        num_archive_files: 2, // Means 1 file in file list
        num_chunks: 2,
    };
    let chunks = vec![
        Chunk {
            offset: 0,
            compressed_length: 10,
            decompressed_length: 10,
            flags: 0,
            file: 0,
        },
        Chunk {
            offset: 10,
            compressed_length: 20,
            decompressed_length: 20,
            flags: 0,
            file: 0,
        },
    ];
    let file_list = vec!["archive.dzip".to_string()];
    let global_settings = RangeSettings {
        win_size: 0,
        flags: 0,
        offset_table_size: 0,
        offset_tables: 0,
        offset_contexts: 0,
        ref_length_table_size: 0,
        ref_length_tables: 0,
        ref_offset_table_size: 0,
        ref_offset_tables: 0,
        big_min_match: 0,
    };

    // Pack
    {
        let mut writer = DzipWriter::new(Cursor::new(&mut buffer));
        writer.write_archive_settings(&archive_settings).unwrap();
        writer.write_strings(&strings).unwrap();
        writer.write_file_chunk_map(&map).unwrap();
        writer.write_chunk_settings(&chunk_settings).unwrap();
        writer.write_chunks(&chunks).unwrap();
        writer.write_strings(&file_list).unwrap(); // File list is just strings
        writer.write_global_settings(&global_settings).unwrap();
    }

    // Unpack
    let mut reader = DzipReader::new(Cursor::new(&buffer));
    let read_archive_settings = reader.read_archive_settings().unwrap();
    assert_eq!(archive_settings, read_archive_settings);

    let read_strings = reader
        .read_strings(
            (archive_settings.num_user_files + archive_settings.num_directories - 1) as usize,
        )
        .unwrap();
    assert_eq!(strings, read_strings);

    let read_map = reader
        .read_file_chunk_map(archive_settings.num_user_files as usize)
        .unwrap();
    assert_eq!(map, read_map);

    let read_chunk_settings = reader.read_chunk_settings().unwrap();
    assert_eq!(chunk_settings, read_chunk_settings);

    let read_chunks = reader
        .read_chunks(chunk_settings.num_chunks as usize)
        .unwrap();
    assert_eq!(chunks, read_chunks);

    // Spec: File List (ChunkSettings.NumArchiveFiles -1 list of null-terminated files)
    let read_file_list = reader
        .read_file_list((chunk_settings.num_archive_files - 1) as usize)
        .unwrap();
    assert_eq!(file_list, read_file_list);

    let read_global_settings = reader.read_global_settings().unwrap();
    assert_eq!(global_settings, read_global_settings);
}

#[test]
fn rejects_nonzero_version_empty_archives_and_missing_root_directory() {
    let mut nonzero_version = Vec::new();
    DzipWriter::new(Cursor::new(&mut nonzero_version))
        .write_archive_settings(&ArchiveSettings {
            header: 0x5A525444,
            num_user_files: 0,
            num_directories: 1,
            version: 1,
        })
        .unwrap();
    assert!(matches!(
        DzipReader::new(Cursor::new(nonzero_version)).read_archive_settings(),
        Err(dzip::DzipError::UnsupportedVersion(1))
    ));

    let mut empty_archive = Vec::new();
    DzipWriter::new(Cursor::new(&mut empty_archive))
        .write_archive_settings(&ArchiveSettings {
            header: 0x5A525444,
            num_user_files: 0,
            num_directories: 1,
            version: 0,
        })
        .unwrap();
    assert!(matches!(
        DzipReader::new(Cursor::new(empty_archive)).read_archive_settings(),
        Err(dzip::DzipError::InvalidArchive(_))
    ));

    let mut missing_root = Vec::new();
    DzipWriter::new(Cursor::new(&mut missing_root))
        .write_archive_settings(&ArchiveSettings {
            header: 0x5A525444,
            num_user_files: 1,
            num_directories: 0,
            version: 0,
        })
        .unwrap();
    assert!(matches!(
        DzipReader::new(Cursor::new(missing_root)).read_archive_settings(),
        Err(dzip::DzipError::InvalidArchive(_))
    ));
}

#[test]
fn test_lzma_round_trip() {
    use dzip::Compression;
    use dzip::format::Chunk;
    use dzip::writer::compress_data;

    // Generate some compressible data
    let original_data: Vec<u8> = (0..1000).map(|i| (i % 255) as u8).collect();

    // Compress
    let (flags, compressed_data) =
        compress_data(&original_data, Compression::Lzma).expect("Compression failed");

    // Manually construct a Chunk for decompression
    let chunk = Chunk {
        offset: 0,
        compressed_length: compressed_data.len() as u32,
        decompressed_length: original_data.len() as u32,
        flags,
        file: 0,
    };

    let mut reader = DzipReader::new(Cursor::new(compressed_data));
    let decompressed_data = reader
        .read_chunk_data(&chunk)
        .expect("Decompression failed");

    assert_eq!(
        original_data, decompressed_data,
        "Decompressed data should match original"
    );
}

#[test]
fn external_codec_headers_match_dzip_original() {
    use dzip::Compression;
    use dzip::writer::compress_data;

    let data = b"dzip external codec framing";
    let (_, zlib) = compress_data(data, Compression::Zlib).unwrap();
    assert_eq!(&zlib[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 11]);

    let (_, bzip) = compress_data(data, Compression::Bzip).unwrap();
    assert_eq!(&bzip[..4], b"BZh1");

    let (_, lzma) = compress_data(data, Compression::Lzma).unwrap();
    assert_eq!(&lzma[..5], &[0x5d, 0, 0, 1, 0]);
    assert_eq!(
        &lzma[5..13],
        &(data.len() as u64).to_le_bytes(),
        "LZMA-alone header stores the original size"
    );
}
