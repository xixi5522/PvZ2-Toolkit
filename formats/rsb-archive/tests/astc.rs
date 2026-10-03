use rsb_archive::{
    ASTC_BLOCK_SIZES, AstcQuality, PtxFormat, RsbError, astc_data_size, decode_astc_rgba8,
    is_valid_astc_block_size,
};

#[test]
fn uses_twinning_pvz2_astc_format_codes() {
    let expected = [(160, (4, 4)), (161, (5, 5)), (162, (6, 6)), (163, (8, 8))];

    for (code, (block_width, block_height)) in expected {
        assert_eq!(
            PtxFormat::from(code),
            PtxFormat::Astc {
                block_width,
                block_height,
            }
        );
    }
    assert_eq!(PtxFormat::from(159), PtxFormat::Unknown(159));
}

#[test]
fn validates_astc_footprints_and_payload_sizes() {
    for (block_width, block_height) in ASTC_BLOCK_SIZES {
        assert!(is_valid_astc_block_size(block_width, block_height));
        assert_eq!(
            astc_data_size(block_width, block_height, block_width, block_height).unwrap(),
            16
        );
    }

    assert_eq!(astc_data_size(13, 9, 5, 4).unwrap(), 144);
    assert!(matches!(
        astc_data_size(8, 8, 7, 7),
        Err(RsbError::InvalidAstcBlockSize {
            width: 7,
            height: 7
        })
    ));
    assert!(matches!(
        decode_astc_rgba8(&[0; 15], 4, 4, 4, 4),
        Err(RsbError::InvalidAstcDataSize {
            expected: 16,
            actual: 15
        })
    ));
}

#[test]
fn validates_twinning_quality_range() {
    assert_eq!(AstcQuality::FASTEST.get(), 0);
    assert_eq!(AstcQuality::MEDIUM.get(), 60);
    assert_eq!(AstcQuality::EXHAUSTIVE.get(), 100);
    assert_eq!(AstcQuality::new(37).unwrap().get(), 37);
    assert!(matches!(
        AstcQuality::new(101),
        Err(RsbError::InvalidAstcQuality(101))
    ));
}

#[test]
fn rgba8_astc_entry_matches_ptx_encoder_dispatch() {
    use image::{Rgba, RgbaImage};
    use rsb_archive::{
        PtxEncodeOptions, PtxEncoder, Rgba8Surface, decode_astc_rgba8, encode_astc_rgba8,
    };

    let rgba8 = RgbaImage::from_fn(9, 7, |x, y| {
        Rgba([
            (x * 21) as u8,
            (y * 29) as u8,
            ((x + y) * 13) as u8,
            (x * 17 + y * 9) as u8,
        ])
    });
    let canonical =
        encode_astc_rgba8(Rgba8Surface::from_image(&rgba8), 5, 4, AstcQuality::FASTEST).unwrap();
    let mut options = PtxEncodeOptions::new(PtxFormat::Astc {
        block_width: 5,
        block_height: 4,
    });
    options.astc_quality = AstcQuality::FASTEST;
    assert_eq!(
        canonical,
        PtxEncoder::encode_image(&rgba8, options).unwrap()
    );
    assert_eq!(
        decode_astc_rgba8(&canonical, 9, 7, 5, 4)
            .unwrap()
            .dimensions(),
        (9, 7)
    );
}

#[test]
fn pure_rust_encoder_output_decodes_for_every_astc_footprint() {
    use image::{Rgba, RgbaImage};
    use rsb_archive::{Rgba8Surface, encode_astc_rgba8};

    let expected = [63_u8, 127, 191, 223];

    for (block_width, block_height) in ASTC_BLOCK_SIZES {
        let image = RgbaImage::from_pixel(block_width, block_height, Rgba(expected));
        let encoded = encode_astc_rgba8(
            Rgba8Surface::from_image(&image),
            block_width,
            block_height,
            AstcQuality::FASTEST,
        )
        .unwrap();
        assert_eq!(encoded.len(), 16, "{block_width}x{block_height}");

        let decoded = decode_astc_rgba8(
            &encoded,
            block_width,
            block_height,
            block_width,
            block_height,
        )
        .unwrap();
        let actual = decoded.get_pixel(0, 0).0;
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 8,
                "{block_width}x{block_height}: expected {expected}, found {actual}"
            );
        }
    }
}

#[test]
fn ptx_encoder_and_decoder_dispatch_pvz2_astc_codes() {
    use image::{Rgba, RgbaImage};
    use rsb_archive::{PtxDecoder, PtxEncodeOptions, PtxEncoder};

    for (code, block_width, block_height) in [(160, 4, 4), (161, 5, 5), (162, 6, 6), (163, 8, 8)] {
        let format = PtxFormat::from(code);
        let image = RgbaImage::from_pixel(block_width, block_height, Rgba([32, 96, 160, 224]));
        let encoded = PtxEncoder::encode_image(&image, PtxEncodeOptions::new(format)).unwrap();
        assert_eq!(encoded.len(), 16, "format code {code}");

        let decoded = PtxDecoder::decode_rgba8(
            &encoded,
            block_width,
            block_height,
            code,
            None,
            None,
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (block_width, block_height)
        );
    }
}

#[test]
fn preserves_exact_constant_colors() {
    use image::{Rgba, RgbaImage};
    use rsb_archive::{Rgba8Surface, encode_astc_rgba8};

    let expected = [17, 83, 149, 211];
    let image = RgbaImage::from_pixel(13, 9, Rgba(expected));
    let encoded =
        encode_astc_rgba8(Rgba8Surface::from_image(&image), 5, 4, AstcQuality::FASTEST).unwrap();
    let decoded = decode_astc_rgba8(&encoded, 13, 9, 5, 4).unwrap();
    assert!(decoded.pixels().all(|pixel| pixel.0 == expected));
}

#[test]
fn encodes_color_and_independent_alpha_gradients() {
    use image::{Rgba, RgbaImage};
    use rsb_archive::{Rgba8Surface, encode_astc_rgba8};

    let image = RgbaImage::from_fn(16, 16, |x, y| {
        Rgba([
            (x * 17) as u8,
            (y * 17) as u8,
            ((x + y) * 8) as u8,
            if (x / 4 + y / 4) % 2 == 0 { 32 } else { 224 },
        ])
    });
    let encoded =
        encode_astc_rgba8(Rgba8Surface::from_image(&image), 4, 4, AstcQuality::MEDIUM).unwrap();
    let decoded = decode_astc_rgba8(&encoded, 16, 16, 4, 4).unwrap();

    let mean_absolute_error = image
        .pixels()
        .zip(decoded.pixels())
        .flat_map(|(expected, actual)| {
            expected
                .0
                .into_iter()
                .zip(actual.0)
                .map(|(expected, actual)| u64::from(expected.abs_diff(actual)))
        })
        .sum::<u64>() as f64
        / (16.0 * 16.0 * 4.0);
    assert!(
        mean_absolute_error < 24.0,
        "unexpected ASTC error: {mean_absolute_error}"
    );
}

#[test]
fn rejects_illegal_blocks_without_panicking() {
    assert!(decode_astc_rgba8(&[0; 16], 4, 4, 4, 4).is_err());

    let mut state = 0x8D26_5A4C_91E3_77B1u64;
    for (block_width, block_height) in ASTC_BLOCK_SIZES {
        for _ in 0..256 {
            let mut block = [0u8; 16];
            for byte in &mut block {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            let _ = decode_astc_rgba8(&block, block_width, block_height, block_width, block_height);
        }
    }
}
