use rsb_archive::{
    RSG_DATA_ALIGNMENT, RsbError, compress_rsg_zlib, compress_rsg_zlib_with_level, compress_zlib,
    compress_zlib_with_level, decompress_zlib, decompress_zlib_exact, is_zlib_stream,
};

#[test]
fn zlib_streams_round_trip_at_every_level() {
    let input = b"PopCap RSG packet data".repeat(128);

    for level in 0..=9 {
        let compressed = compress_zlib_with_level(&input, level).unwrap();
        assert!(is_zlib_stream(&compressed));
        assert_eq!(decompress_zlib(&compressed).unwrap(), input);
    }

    let default_compressed = compress_zlib(&input).unwrap();
    assert_eq!(
        decompress_zlib_exact(&default_compressed, input.len()).unwrap(),
        input
    );
}

#[test]
fn decodes_an_independent_stored_zlib_stream() {
    // zlib header + one uncompressed DEFLATE block containing "hello" +
    // Adler-32 checksum.
    let stream = [
        0x78, 0x01, 0x01, 0x05, 0x00, 0xfa, 0xff, b'h', b'e', b'l', b'l', b'o', 0x06, 0x2c, 0x02,
        0x15,
    ];

    assert!(is_zlib_stream(&stream));
    assert_eq!(decompress_zlib_exact(&stream, 5).unwrap(), b"hello");
}

#[test]
fn rsg_zlib_output_is_zero_padded_to_4096_bytes() {
    let input = b"texture payload".repeat(300);

    for compressed in [
        compress_rsg_zlib(&input).unwrap(),
        compress_rsg_zlib_with_level(&input, 9).unwrap(),
    ] {
        assert_eq!(compressed.len() % RSG_DATA_ALIGNMENT, 0);
        assert!(is_zlib_stream(&compressed));
        assert_eq!(
            decompress_zlib_exact(&compressed, input.len()).unwrap(),
            input
        );
    }
}

#[test]
fn reports_invalid_levels_sizes_and_streams() {
    assert!(matches!(
        compress_zlib_with_level(b"data", 10),
        Err(RsbError::InvalidZlibLevel { level: 10 })
    ));

    let compressed = compress_zlib(b"data").unwrap();
    assert!(matches!(
        decompress_zlib_exact(&compressed, 5),
        Err(RsbError::ZlibSizeMismatch {
            expected: 5,
            actual: 4
        })
    ));
    assert!(matches!(
        decompress_zlib(b"not zlib"),
        Err(RsbError::ZlibDecompression { .. })
    ));
    assert!(!is_zlib_stream(&[]));
    assert!(!is_zlib_stream(&[0x78]));
    assert!(!is_zlib_stream(b"not zlib"));
}
