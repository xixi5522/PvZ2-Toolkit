use dzip::format::Chunk;
#[cfg(all(windows, target_arch = "x86_64"))]
use dzip::format::{CHUNK_DZ, CHUNK_ZLIB};
use dzip::reader::DzipReader;
use dzip::writer::compress_data;
use dzip::{Archive, Compression};
#[cfg(all(windows, target_arch = "x86_64"))]
use dzip::{ArchiveBuilder, EntryOptions};
use std::io::Cursor;
use std::path::PathBuf;
#[cfg(all(windows, target_arch = "x86_64"))]
use std::process::Command;
#[cfg(all(windows, target_arch = "x86_64"))]
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data/native")
}

fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(fixture_root().join("corpus").join(name)).unwrap()
}

fn random_bytes(length: usize, mut state: u32) -> Vec<u8> {
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn reference_codec_corpus() -> Vec<(String, Vec<u8>)> {
    const SIZES: [usize; 35] = [
        0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 257, 258, 259, 511,
        512, 1023, 4095, 8191, 16_383, 16_384, 16_385, 32_767, 32_768, 32_769, 65_535, 65_536,
        65_537,
    ];
    const DISTANCES: [usize; 57] = [
        1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 16, 17, 24, 25, 32, 33, 48, 49, 64, 65, 96, 97, 128,
        129, 192, 193, 256, 257, 384, 385, 512, 513, 768, 769, 1024, 1025, 1536, 1537, 2048, 2049,
        3072, 3073, 4096, 4097, 6144, 6145, 8192, 8193, 12_288, 12_289, 16_384, 16_385, 24_576,
        24_577, 32_767, 32_768,
    ];

    let mut corpus = Vec::with_capacity(SIZES.len() * 3 + DISTANCES.len());
    for &size in &SIZES {
        corpus.push((format!("size-{size:05}-zeros.bin"), vec![0; size]));
        corpus.push((
            format!("size-{size:05}-periodic.bin"),
            (0..size).map(|index| index as u8).collect(),
        ));
        corpus.push((
            format!("size-{size:05}-random.bin"),
            random_bytes(size, 0xa5a5_5a5a ^ size as u32),
        ));
    }
    for &distance in &DISTANCES {
        let prefix = random_bytes(distance, 0x6d2b_79f5 ^ distance as u32);
        let mut data = prefix.clone();
        data.extend((0..258).map(|index| prefix[index % prefix.len()]));
        corpus.push((format!("distance-{distance:05}.bin"), data));
    }
    corpus
}

fn decode_chunk(encoded: Vec<u8>, input_len: usize, flags: u16) -> dzip::Result<Vec<u8>> {
    let chunk = Chunk {
        offset: 0,
        compressed_length: encoded.len() as u32,
        decompressed_length: input_len as u32,
        flags,
        file: 0,
    };
    DzipReader::new(Cursor::new(encoded)).read_chunk_data(&chunk)
}

#[test]
fn decodes_original_dzip_exe_mixed_codec_archive() {
    let root = fixture_root();
    let mut archive = Archive::open_path(root.join("codecs.dz")).unwrap();
    let entries = archive
        .entries()
        .iter()
        .map(|entry| (entry.id(), entry.path().to_path_buf()))
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 9);
    for (id, path) in entries {
        let expected = std::fs::read(root.join("corpus").join(&path)).unwrap();
        assert_eq!(
            archive.read_entry(id).unwrap(),
            expected,
            "{}",
            path.display()
        );
    }
}

#[test]
fn new_encoders_round_trip_without_bitstream_identity() {
    let fixtures = [
        Vec::new(),
        vec![0],
        vec![0; 100_000],
        b"standards-compatible codec payload\n".repeat(1024),
        (0..=255).cycle().take(200_000).collect(),
        (0..100_001).map(|index| (index / 4 % 251) as u8).collect(),
        random_bytes(65_535, 0x1234_5678),
    ];
    for input in fixtures {
        for method in [Compression::Zlib, Compression::Bzip, Compression::Lzma] {
            let (flags, encoded) = compress_data(&input, method).unwrap();
            assert_eq!(decode_chunk(encoded, input.len(), flags).unwrap(), input);
        }
    }
}

#[test]
fn original_individual_codec_payloads_match_corpus() {
    // These paths in codecs.dz were written by the checked-in dzip.exe and
    // exercise dynamic DEFLATE, BZip2, and LZMA SDK streams respectively.
    let root = fixture_root();
    let mut archive = Archive::open_path(root.join("codecs.dz")).unwrap();
    for path in ["local/random.bin", "local/text.txt", "local/periodic.bin"] {
        assert_eq!(archive.read_entry_by_path(path).unwrap(), corpus(path));
    }
}

#[test]
fn truncated_and_corrupted_payloads_are_rejected() {
    let input = b"damaged compressed data must never decode silently".repeat(200);
    for method in [Compression::Zlib, Compression::Bzip, Compression::Lzma] {
        let (flags, mut encoded) = compress_data(&input, method).unwrap();
        encoded.truncate((encoded.len() / 2).max(1));
        assert!(
            decode_chunk(encoded, input.len(), flags).is_err(),
            "{method:?}"
        );
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn reference_dzip_exe_extracts_new_zlib_and_lzma_streams() {
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference/dzip/dzip.exe");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dzip-archive-reference-codecs-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();

    for (name, method) in [("zlib", Compression::Zlib), ("lzma", Compression::Lzma)] {
        let data = b"new Rust encoder decoded by the reference dzip executable\n".repeat(2048);
        let archive_name = format!("{name}.dz");
        let mut builder = ArchiveBuilder::new();
        builder
            .add_bytes(
                "payload.bin",
                data.clone(),
                EntryOptions::new().compression(method),
            )
            .unwrap();
        builder.write_to_path(root.join(&archive_name)).unwrap();

        let output = Command::new(&reference)
            .current_dir(&root)
            .args(["-d", &archive_name])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "reference dzip failed for {name}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            std::fs::read(root.join(name).join("payload.bin")).unwrap(),
            data
        );
    }

    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn reference_combined_flags_preserve_registered_order_and_dz_asymmetry() {
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference/dzip/dzip.exe");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dzip-archive-reference-combined-flags-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let data = b"combined zlib and dz flags".repeat(4096);
    std::fs::write(root.join("payload.bin"), &data).unwrap();
    std::fs::write(
        root.join("combined.dcl"),
        "archive combined.dz\nbasedir .\nfile payload.bin 1 zlib dz\n",
    )
    .unwrap();

    let packed = Command::new(&reference)
        .current_dir(&root)
        .args(["-q", "combined.dcl"])
        .output()
        .unwrap();
    assert!(
        packed.status.success(),
        "reference dzip failed: stdout={} stderr={}",
        String::from_utf8_lossy(&packed.stdout),
        String::from_utf8_lossy(&packed.stderr)
    );
    let mut reader = DzipReader::new(std::fs::File::open(root.join("combined.dz")).unwrap());
    let settings = reader.read_archive_settings().unwrap();
    let string_count =
        usize::from(settings.num_user_files) + usize::from(settings.num_directories) - 1;
    reader.read_raw_strings(string_count).unwrap();
    reader
        .read_file_chunk_map(usize::from(settings.num_user_files))
        .unwrap();
    let chunk_settings = reader.read_chunk_settings().unwrap();
    let chunks = reader
        .read_chunks(usize::from(chunk_settings.num_chunks))
        .unwrap();
    reader
        .read_raw_file_list(usize::from(
            chunk_settings.num_archive_files.saturating_sub(1),
        ))
        .unwrap();
    let metadata_end = reader.position().unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].flags, CHUNK_ZLIB | CHUNK_DZ);
    assert_eq!(u64::from(chunks[0].offset), metadata_end);

    let extracted = Command::new(&reference)
        .current_dir(&root)
        .args(["-q", "-d", "combined.dz"])
        .output()
        .unwrap();
    assert!(!extracted.status.success());
    assert!(!root.join("combined").join("payload.bin").exists());

    std::fs::write(
        root.join("combined-lzma.dcl"),
        "archive combined-lzma.dz\nbasedir .\nfile payload.bin 1 zlib lzma\n",
    )
    .unwrap();
    let packed = Command::new(&reference)
        .current_dir(&root)
        .args(["-q", "combined-lzma.dcl"])
        .output()
        .unwrap();
    assert!(packed.status.success());
    let extracted = Command::new(&reference)
        .current_dir(&root)
        .args(["-q", "-d", "combined-lzma.dz"])
        .output()
        .unwrap();
    let extracted_payload = std::fs::read(root.join("combined-lzma").join("payload.bin")).ok();
    assert!(
        extracted.status.success(),
        "reference dzip failed: stdout={} stderr={}",
        String::from_utf8_lossy(&extracted.stdout),
        String::from_utf8_lossy(&extracted.stderr)
    );
    assert_eq!(extracted_payload.as_deref(), Some(data.as_slice()));
    let mut archive = Archive::open_path(root.join("combined-lzma.dz")).unwrap();
    assert_eq!(archive.read_entry_by_path("payload.bin").unwrap(), data);

    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn reference_codec_matrices_are_bidirectionally_compatible() {
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference/dzip/dzip.exe");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "dzip-archive-reference-codec-matrices-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();

    let corpus = reference_codec_corpus();
    for (name, data) in &corpus {
        std::fs::write(root.join(name), data).unwrap();
    }

    for (tag, flag, compression) in [
        ("zlib", "zlib", Compression::Zlib),
        ("bzip", "bzip", Compression::Bzip),
        ("lzma", "lzma", Compression::Lzma),
        ("dz", "dz", Compression::Dz),
    ] {
        let method_corpus = corpus
            .iter()
            .filter(|(name, data)| {
                compression != Compression::Bzip
                    || (data.len() >= 1023
                        && (name.ends_with("-zeros.bin") || name.ends_with("-periodic.bin")))
            })
            .collect::<Vec<_>>();
        let reference_archive_name = format!("reference-{tag}.dz");
        let reference_config_name = format!("reference-{tag}.dcl");
        let mut config = format!("archive {reference_archive_name}\nbasedir .\n");
        for (name, _) in &method_corpus {
            config.push_str(&format!("file {name} 1 {flag}\n"));
        }
        if compression == Compression::Dz {
            config.push_str(
                "options dz\nmax_mem_usage -1\nuse_combuf 0\npreprocess 1\n\
                 WinSize 16\nOffsetTableSize 8\nOffsetTables 3\nOffsetContexts 3\n\
                 RefLengthTableSize 7\nRefLengthTables 1\nRefOffsetTableSize 7\n\
                 RefOffsetTables 3\nBigMinMatch 15\n",
            );
        }
        std::fs::write(root.join(&reference_config_name), config).unwrap();

        let packed = Command::new(&reference)
            .current_dir(&root)
            .args(["-q", &reference_config_name])
            .output()
            .unwrap();
        assert!(
            packed.status.success(),
            "reference dzip failed to create the {tag} matrix: stdout={} stderr={}",
            String::from_utf8_lossy(&packed.stdout),
            String::from_utf8_lossy(&packed.stderr)
        );

        let mut reference_archive = Archive::open_path(root.join(&reference_archive_name)).unwrap();
        assert_eq!(
            reference_archive.entries().len(),
            method_corpus.len(),
            "{tag}"
        );
        for (name, expected) in &method_corpus {
            let (id, actual_compression) = reference_archive
                .entries()
                .iter()
                .find(|entry| entry.path() == std::path::Path::new(name))
                .map(|entry| (entry.id(), entry.compression()))
                .unwrap_or_else(|| panic!("reference {tag} archive omitted {name}"));
            assert_eq!(actual_compression, compression, "{tag}: {name}");
            assert_eq!(
                reference_archive.read_entry(id).unwrap(),
                *expected,
                "{tag}: {name}"
            );
        }

        let rust_archive_name = format!("rust-{tag}.dz");
        let mut builder = ArchiveBuilder::new();
        for (name, data) in &method_corpus {
            builder
                .add_bytes(
                    name,
                    data.clone(),
                    EntryOptions::new().compression(compression),
                )
                .unwrap();
        }
        builder
            .write_to_path(root.join(&rust_archive_name))
            .unwrap();
        // Marmalade 8.6 dzip.exe cannot extract even the BZip-only archive it
        // just created above. Keep the independently produced BZip streams as
        // decoder fixtures, while the crate-level tests validate Rust output
        // against standard BZip2 vectors and CRCs.
        if compression == Compression::Bzip {
            continue;
        }
        let extracted = Command::new(&reference)
            .current_dir(&root)
            .args(["-q", "-d", &rust_archive_name])
            .output()
            .unwrap();
        assert!(
            extracted.status.success(),
            "reference dzip failed to extract the Rust {tag} matrix: stdout={} stderr={}",
            String::from_utf8_lossy(&extracted.stdout),
            String::from_utf8_lossy(&extracted.stderr)
        );
        let extracted_root = root.join(format!("rust-{tag}"));
        for (name, expected) in &method_corpus {
            assert_eq!(
                std::fs::read(extracted_root.join(name)).unwrap(),
                *expected,
                "{tag}: {name}"
            );
        }
    }

    std::fs::remove_dir_all(root).unwrap();
}
