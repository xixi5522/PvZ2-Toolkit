use dzip::format::{
    CHUNK_BZIP, CHUNK_COMBUF, CHUNK_COPYCOMP, CHUNK_DZ, CHUNK_JPEG, CHUNK_LZMA, CHUNK_MP3,
    CHUNK_RANDOMACCESS, CHUNK_ZERO, CHUNK_ZLIB, Chunk,
};
use dzip::reader::DzipReader;
use std::fs::File;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data/native")
}

fn inspect_archive(
    name: &str,
    expected_user_files: u16,
    expected_volume_names: &[&str],
) -> Vec<Chunk> {
    let path = fixture_root().join(name);
    let mut reader = DzipReader::new(File::open(&path).expect("native fixture must exist"));
    let archive = reader
        .read_archive_settings()
        .expect("failed to read archive settings");

    assert_eq!(archive.header, 0x5A525444);
    assert_eq!(archive.num_user_files, expected_user_files);

    let strings_count = (archive.num_user_files + archive.num_directories - 1) as usize;
    assert_eq!(
        reader.read_strings(strings_count).unwrap().len(),
        strings_count
    );
    assert_eq!(
        reader
            .read_file_chunk_map(archive.num_user_files as usize)
            .unwrap()
            .len(),
        archive.num_user_files as usize
    );

    let chunk_settings = reader.read_chunk_settings().unwrap();
    assert_eq!(
        chunk_settings.num_archive_files,
        expected_volume_names.len() as u16 + 1
    );
    let chunks = reader
        .read_chunks(chunk_settings.num_chunks as usize)
        .unwrap();
    assert_eq!(chunks.len(), chunk_settings.num_chunks as usize);
    assert_eq!(
        reader.read_file_list(expected_volume_names.len()).unwrap(),
        expected_volume_names
    );
    reader.read_global_settings().unwrap();
    chunks
}

#[test]
fn parses_native_mixed_codec_archive() {
    let chunks = inspect_archive("codecs.dz", 9, &["codecs-1.dz"]);
    for expected_flag in [
        CHUNK_DZ,
        CHUNK_ZLIB,
        CHUNK_BZIP,
        CHUNK_LZMA,
        CHUNK_COPYCOMP,
        CHUNK_ZERO,
        CHUNK_JPEG,
        CHUNK_MP3,
        CHUNK_RANDOMACCESS,
    ] {
        assert!(
            chunks.iter().any(|chunk| chunk.flags & expected_flag != 0),
            "missing chunk flag {expected_flag:#x}"
        );
    }
}

#[test]
fn parses_native_aligned_split_archive() {
    let chunks = inspect_archive("ranges.dz", 3, &["ranges-1.dz", "ranges-2.dz"]);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.flags & (CHUNK_DZ | CHUNK_COMBUF) != 0)
    );
    assert!(chunks.iter().any(|chunk| chunk.file == 0));
    assert!(chunks.iter().any(|chunk| chunk.file == 1));
    assert!(chunks.iter().any(|chunk| chunk.file == 2));
}

#[test]
fn parses_native_tiny_dz_archive() {
    let chunks = inspect_archive("tiny.dz", 3, &[]);
    assert_eq!(chunks.len(), 3);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.decompressed_length)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}
