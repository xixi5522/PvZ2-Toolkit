use image::{Rgba, RgbaImage};
use rsb_archive::{
    ChannelOrder, PtxDecoder, PtxDescriptor, PtxEncodeOptions, PtxEncoder, PtxFormat,
    PtxFormatCode, PtxPayload, PtxPayloadLayout, PtxRsbMetadata, resolve_rsb_format,
};

#[test]
fn resolves_ambiguous_rsb_formats_once() {
    assert_eq!(
        resolve_rsb_format(PtxFormatCode(30), None, None).unwrap(),
        PtxFormat::Pvrtc4BppRgba
    );
    assert_eq!(
        resolve_rsb_format(PtxFormatCode(30), None, Some(1)).unwrap(),
        PtxFormat::Pvrtc4BppRgba
    );
    assert_eq!(
        resolve_rsb_format(PtxFormatCode(30), Some(49), Some(100)).unwrap(),
        PtxFormat::Etc1Palette
    );
    assert_eq!(
        resolve_rsb_format(PtxFormatCode(147), None, None).unwrap(),
        PtxFormat::Etc1
    );
    assert_eq!(
        resolve_rsb_format(PtxFormatCode(147), Some(64), Some(100)).unwrap(),
        PtxFormat::Etc1Palette
    );
    assert!(resolve_rsb_format(PtxFormatCode(-1), None, None).is_err());
}

#[test]
fn resolves_ambiguous_rsb_formats_from_payload_layout() {
    let metadata = |code, additional_byte_count| PtxRsbMetadata {
        format_code: PtxFormatCode(code),
        alpha_size: additional_byte_count,
        alpha_format: Some(100),
        row_pitch: None,
        channel_order: ChannelOrder::Rgba,
    };

    let etc1 = PtxDescriptor::from_rsb_payload(4, 4, metadata(147, None), &[0; 8]).unwrap();
    assert_eq!(etc1.format, PtxFormat::Etc1);

    let etc1_a8 = PtxDescriptor::from_rsb_payload(4, 4, metadata(147, None), &[0; 24]).unwrap();
    assert_eq!(etc1_a8.format, PtxFormat::Etc1A8);

    let etc1_compressed_alpha =
        PtxDescriptor::from_rsb_payload(4, 4, metadata(147, None), &[0; 16]).unwrap();
    assert_eq!(etc1_compressed_alpha.format, PtxFormat::Etc1CompressedAlpha);

    let mut palette = vec![0; 8];
    palette.push(16);
    palette.extend(0..16);
    palette.extend([0; 8]);
    let etc1_palette =
        PtxDescriptor::from_rsb_payload(4, 4, metadata(147, Some(25)), &palette).unwrap();
    assert_eq!(etc1_palette.format, PtxFormat::Etc1Palette);

    let pvrtc = PtxDescriptor::from_rsb_payload(8, 8, metadata(30, None), &[0; 32]).unwrap();
    assert_eq!(pvrtc.format, PtxFormat::Pvrtc4BppRgba);
}

#[test]
fn computes_block_and_alpha_ranges_for_odd_dimensions() {
    let raw = PtxDescriptor::new(5, 7, PtxFormat::Etc1A8).unwrap();
    let raw_layout = PtxPayloadLayout::for_encode(raw).unwrap();
    assert_eq!(raw_layout.color, 0..32);
    assert_eq!(raw_layout.alpha.unwrap().range, 32..67);

    let compressed = PtxDescriptor::new(5, 7, PtxFormat::Etc1CompressedAlpha).unwrap();
    let compressed_layout = PtxPayloadLayout::for_encode(compressed).unwrap();
    assert_eq!(compressed_layout.color, 0..32);
    assert_eq!(compressed_layout.alpha.unwrap().range, 32..64);

    let palette = PtxDescriptor::new(5, 7, PtxFormat::Etc1Palette).unwrap();
    assert_eq!(PtxPayloadLayout::for_encode(palette).unwrap().total_len, 67);
}

#[test]
fn packed_formats_have_consistent_byte_order() {
    let image = RgbaImage::from_pixel(1, 1, Rgba([18, 107, 212, 149]));
    let cases = [
        (PtxFormat::La88, vec![112, 149], [112, 112, 112, 149]),
        (PtxFormat::Al88, vec![149, 112], [112, 112, 112, 149]),
        (PtxFormat::Rgb332, vec![15], [0, 109, 255, 255]),
        (
            PtxFormat::Argb8888,
            vec![212, 107, 18, 149],
            [18, 107, 212, 149],
        ),
    ];
    for (format, expected_bytes, expected_pixel) in cases {
        let encoded = PtxEncoder::encode_image(&image, PtxEncodeOptions::new(format)).unwrap();
        assert_eq!(encoded, expected_bytes, "{format:?}");
        let descriptor = PtxDescriptor::new(1, 1, format).unwrap();
        let decoded = PtxDecoder::decode_with_descriptor(&encoded, descriptor).unwrap();
        assert_eq!(decoded.get_pixel(0, 0).0, expected_pixel, "{format:?}");
    }
}

#[test]
fn linear_pitch_and_apple_channel_order_are_explicit() {
    let descriptor = PtxDescriptor::new(1, 2, PtxFormat::Rgba8888)
        .unwrap()
        .with_row_pitch(Some(8))
        .with_channel_order(ChannelOrder::Bgra);
    let bytes = [30, 20, 10, 40, 0, 0, 0, 0, 70, 60, 50, 80, 0, 0, 0, 0];
    let decoded = PtxDecoder::decode_with_descriptor(&bytes, descriptor).unwrap();
    assert_eq!(decoded.get_pixel(0, 0).0, [10, 20, 30, 40]);
    assert_eq!(decoded.get_pixel(0, 1).0, [50, 60, 70, 80]);
}

#[test]
fn tiled_payloads_include_complete_edge_tiles() {
    let image = RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 255]));
    let bytes =
        PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Rgb565Block)).unwrap();
    assert_eq!(bytes.len(), 32 * 32 * 2);
    let descriptor = PtxDescriptor::new(1, 1, PtxFormat::Rgb565Block).unwrap();
    let decoded = PtxDecoder::decode_with_descriptor(&bytes, descriptor).unwrap();
    assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
}

#[test]
fn every_cpu_entry_rejects_truncated_payloads_without_panicking() {
    let formats = [
        PtxFormat::Rgba8888,
        PtxFormat::Rgba4444,
        PtxFormat::A8,
        PtxFormat::Rgb888,
        PtxFormat::Argb8888,
        PtxFormat::Rgba4444Block,
        PtxFormat::Etc1,
        PtxFormat::Etc1A8,
        PtxFormat::Etc1CompressedAlpha,
        PtxFormat::Etc1Palette,
        PtxFormat::Astc {
            block_width: 4,
            block_height: 4,
        },
    ];
    for format in formats {
        let descriptor = PtxDescriptor::new(4, 4, format).unwrap();
        let layout = PtxPayloadLayout::for_encode(descriptor).unwrap();
        let truncated = vec![0; layout.total_len.saturating_sub(1)];
        assert!(
            PtxPayload::parse(&truncated, descriptor).is_err(),
            "{format:?}"
        );
    }
}

#[test]
fn manifest_decoder_delegates_to_resolved_descriptor() {
    let image = RgbaImage::from_fn(4, 4, |x, y| {
        Rgba([x as u8 * 40, y as u8 * 50, 90, (x + y) as u8 * 30])
    });
    let encoded =
        PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Etc1A8)).unwrap();
    let decoded =
        PtxDecoder::decode_rgba8(&encoded, 4, 4, 147, None, Some(100), None, false).unwrap();
    for (actual, source) in decoded.pixels().zip(image.pixels()) {
        assert_eq!(actual[3], source[3]);
    }

    let metadata = PtxRsbMetadata {
        format_code: PtxFormatCode(147),
        alpha_size: None,
        alpha_format: Some(100),
        row_pitch: None,
        channel_order: ChannelOrder::Rgba,
    };
    assert_eq!(
        PtxDescriptor::from_rsb_payload(4, 4, metadata, &encoded)
            .unwrap()
            .format,
        PtxFormat::Etc1A8
    );
}

#[test]
fn rgba8_etc1_palette_entry_matches_ptx_encoder_dispatch() {
    use rsb_archive::Rgba8Surface;
    use rsb_archive::ptx::codec::etc1::{decode_palette_alpha_rgba8, encode_palette_alpha_rgba8};

    let rgba8 = RgbaImage::from_fn(4, 4, |x, y| {
        Rgba([
            (x * 55) as u8,
            (y * 63) as u8,
            ((x + y) * 29) as u8,
            ((x + y) * 31) as u8,
        ])
    });
    let canonical = encode_palette_alpha_rgba8(Rgba8Surface::from_image(&rgba8)).unwrap();
    let dispatched =
        PtxEncoder::encode_image(&rgba8, PtxEncodeOptions::new(PtxFormat::Etc1Palette)).unwrap();
    assert_eq!(canonical, dispatched);
    let decoded = decode_palette_alpha_rgba8(&canonical, 4, 4).unwrap();
    let descriptor = PtxDescriptor::new(4, 4, PtxFormat::Etc1Palette).unwrap();
    let dispatched_decoded = PtxDecoder::decode_with_descriptor(&canonical, descriptor).unwrap();
    assert_eq!(decoded, dispatched_decoded);
}
