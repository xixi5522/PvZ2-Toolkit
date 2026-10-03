use image::{Rgba, RgbaImage};
use rsb_archive::{
    PtxDecoder, PtxEncodeOptions, PtxEncoder, PtxFormat, Rgba8Surface, decode_pvrtc_4bpp_rgba8,
    encode_pvrtc_4bpp_rgba8,
};

#[test]
fn rgba8_pvrtc_entry_matches_ptx_encoder_dispatch() {
    let rgba8 = RgbaImage::from_fn(8, 16, |x, y| {
        Rgba([
            (x * 31) as u8,
            (y * 15) as u8,
            ((x + y) * 11) as u8,
            (x * 19 + y * 7) as u8,
        ])
    });
    let canonical = encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&rgba8), true).unwrap();
    assert_eq!(
        canonical,
        PtxEncoder::encode_image(&rgba8, PtxEncodeOptions::new(PtxFormat::Pvrtc4BppRgba),).unwrap()
    );
    assert_eq!(
        decode_pvrtc_4bpp_rgba8(&canonical, 8, 16)
            .unwrap()
            .dimensions(),
        (8, 16)
    );
}

#[test]
fn encodes_nonzero_pvrtc_packets_and_round_trips_constant_color() {
    let expected = [72, 136, 200, 255];
    let image = RgbaImage::from_pixel(8, 8, Rgba(expected));
    let encoded = encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&image), true).unwrap();

    assert_eq!(encoded.len(), 32);
    assert!(encoded.iter().any(|&byte| byte != 0));

    let decoded = decode_pvrtc_4bpp_rgba8(&encoded, 8, 8).unwrap();
    for pixel in decoded.pixels() {
        for (actual, expected) in pixel.0.into_iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 9,
                "expected {expected}, found {actual}"
            );
        }
    }
}

#[test]
fn round_trips_rgba_gradient_with_bounded_error() {
    let image = RgbaImage::from_fn(16, 16, |x, y| {
        Rgba([
            (x * 17) as u8,
            (y * 17) as u8,
            ((x + y) * 8) as u8,
            ((x * 11 + y * 5) & 0xFF) as u8,
        ])
    });
    let encoded = encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&image), true).unwrap();
    let decoded = decode_pvrtc_4bpp_rgba8(&encoded, 16, 16).unwrap();
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
        mean_absolute_error < 35.0,
        "unexpected PVRTC error: {mean_absolute_error}"
    );
}

#[test]
fn ptx_dispatch_encodes_pvrtc_and_separate_alpha() {
    let image = RgbaImage::from_fn(8, 8, |x, y| {
        Rgba([x as u8 * 31, y as u8 * 31, 127, (x * 32 + y) as u8])
    });

    let rgba =
        PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Pvrtc4BppRgba)).unwrap();
    assert_eq!(rgba.len(), 32);
    let rgba_a8 =
        PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Pvrtc4BppRgbaA8))
            .unwrap();
    assert_eq!(rgba_a8.len(), 32 + 64);

    let decoded =
        PtxDecoder::decode_rgba8(&rgba_a8, 8, 8, 148, None, Some(64), None, false).unwrap();
    for (expected, actual) in image.pixels().zip(decoded.pixels()) {
        assert_eq!(expected[3], actual[3]);
    }
}

#[test]
fn rejects_dimensions_outside_the_supported_pvrtc_layout() {
    let non_power_of_two = RgbaImage::new(12, 12);
    assert!(encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&non_power_of_two), true).is_err());
}

#[test]
fn supports_rectangular_power_of_two_textures() {
    let image = RgbaImage::from_fn(8, 16, |x, y| {
        Rgba([(x * 31) as u8, (y * 15) as u8, ((x + y) * 11) as u8, 255])
    });
    let encoded = encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&image), true).unwrap();
    assert_eq!(encoded.len(), 64);
    let decoded = decode_pvrtc_4bpp_rgba8(&encoded, 8, 16).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (8, 16));
}

#[test]
fn matches_twinning_pvrtc_reference_packets() {
    let image = RgbaImage::from_fn(8, 8, |x, y| {
        Rgba([
            (x * 31) as u8,
            (y * 29) as u8,
            ((x + y) * 13) as u8,
            (x * 17 + y * 7) as u8,
        ])
    });
    let expected = [
        0x00, 0x50, 0xA4, 0xA4, 0x00, 0x00, 0x65, 0x26, 0xA4, 0xA4, 0xA4, 0xF9, 0x62, 0x00, 0xC8,
        0x36, 0x54, 0x95, 0xEA, 0xEA, 0x02, 0x17, 0x68, 0x4D, 0xEA, 0xEA, 0xEA, 0xFF, 0x64, 0x27,
        0xCB, 0x5D,
    ];

    assert_eq!(
        encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&image), true).unwrap(),
        expected
    );
}
