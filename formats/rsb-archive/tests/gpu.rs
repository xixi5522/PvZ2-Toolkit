#![cfg(all(feature = "gpu", not(target_arch = "wasm32")))]

use image::{Rgba, RgbaImage};
use rsb_archive::{
    AstcQuality, DecodeBackend, EncodeBackend, GpuEncodeOptions, PtxDecoder, PtxDescriptor,
    PtxEncodeOptions, PtxEncoder, PtxFormat, PtxGpuCodec, PtxGpuFormat, PtxTextureDescriptor,
    Rgba8Surface, decode_astc_rgba8, decode_pvrtc_4bpp_rgba8, encode_astc_rgba8,
    encode_pvrtc_4bpp_rgba8, supported_optional_features,
};

#[test]
fn compute_codecs_create_valid_payloads_and_textures() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
        {
            Ok(adapter) => adapter,
            Err(_) => return,
        };
        let Ok((device, queue)) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rsb-archive GPU tests"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
        else {
            return;
        };

        let pipeline_errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let codec = PtxGpuCodec::new(&device, &queue);
        let pipeline_error = pipeline_errors.pop().await;
        assert!(
            pipeline_error.is_none(),
            "all bundled WGSL pipelines must validate: {pipeline_error:?}"
        );

        let image = test_image(8, 8);
        let rgba = image.as_raw();

        let astc = codec
            .encode_fast_rgba8(
                rgba,
                8,
                8,
                GpuEncodeOptions::new(PtxGpuFormat::Astc {
                    block_width: 4,
                    block_height: 4,
                }),
            )
            .await
            .unwrap();
        assert_eq!(astc.data.len(), 64);
        assert!(decode_astc_rgba8(&astc.data, 8, 8, 4, 4).is_ok());

        let etc1 = codec
            .encode_fast_rgba8(rgba, 8, 8, GpuEncodeOptions::new(PtxGpuFormat::Etc1))
            .await
            .unwrap();
        assert_eq!(etc1.data.len(), 32);
        assert!(PtxDecoder::decode_rgba8(&etc1.data, 8, 8, 147, None, None, None, false).is_ok());

        let pvrtc = codec
            .encode_fast_rgba8(
                rgba,
                8,
                8,
                GpuEncodeOptions::new(PtxGpuFormat::Pvrtc4BppRgba),
            )
            .await
            .unwrap();
        assert_eq!(pvrtc.data.len(), 32);
        assert!(decode_pvrtc_4bpp_rgba8(&pvrtc.data, 8, 8).is_ok());

        let pvrtc_a8 = codec
            .encode_fast_rgba8(
                rgba,
                8,
                8,
                GpuEncodeOptions::new(PtxGpuFormat::Pvrtc4BppRgbaA8),
            )
            .await
            .unwrap();
        assert_eq!(pvrtc_a8.data.len(), 96);

        let etc1_a8 = codec
            .encode_fast_rgba8(rgba, 8, 8, GpuEncodeOptions::new(PtxGpuFormat::Etc1A8))
            .await
            .unwrap();
        assert_eq!(etc1_a8.data.len(), 96);

        let etc1_compressed_alpha = codec
            .encode_fast_rgba8(
                rgba,
                8,
                8,
                GpuEncodeOptions::new(PtxGpuFormat::Etc1CompressedAlpha),
            )
            .await
            .unwrap();
        assert_eq!(etc1_compressed_alpha.data.len(), 64);

        let etc1_palette = codec
            .encode_fast_rgba8(rgba, 8, 8, GpuEncodeOptions::new(PtxGpuFormat::Etc1Palette))
            .await
            .unwrap();
        assert_eq!(etc1_palette.data.len(), 81);

        let upload_errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let etc_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &etc1.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Etc1,
                label: Some("ETC1 GPU readback"),
            })
            .await
            .unwrap();
        let etc_cpu_rgba = PtxDecoder::decode_rgba8(&etc1.data, 8, 8, 147, None, None, None, false)
            .unwrap()
            .into_raw();
        assert_pixels_close(&etc_gpu_rgba, &etc_cpu_rgba, 1);

        let pvrtc_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &pvrtc.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Pvrtc4BppRgba,
                label: Some("PVRTC GPU readback"),
            })
            .await
            .unwrap();
        let pvrtc_cpu_rgba = decode_pvrtc_4bpp_rgba8(&pvrtc.data, 8, 8)
            .unwrap()
            .into_raw();
        assert_pixels_close(&pvrtc_gpu_rgba, &pvrtc_cpu_rgba, 1);

        let pvrtc_a8_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &pvrtc_a8.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Pvrtc4BppRgbaA8,
                label: Some("PVRTC+A8 GPU readback"),
            })
            .await
            .unwrap();
        let pvrtc_a8_cpu_rgba =
            PtxDecoder::decode_rgba8(&pvrtc_a8.data, 8, 8, 148, None, None, None, false)
                .unwrap()
                .into_raw();
        assert_pixels_close(&pvrtc_a8_gpu_rgba, &pvrtc_a8_cpu_rgba, 1);

        let etc1_a8_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &etc1_a8.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Etc1A8,
                label: Some("ETC1+A8 GPU readback"),
            })
            .await
            .unwrap();
        let etc1_a8_cpu_rgba =
            PtxDecoder::decode_rgba8(&etc1_a8.data, 8, 8, 147, Some(64), Some(100), None, false)
                .unwrap()
                .into_raw();
        assert_pixels_close(&etc1_a8_gpu_rgba, &etc1_a8_cpu_rgba, 1);

        let etc1_compressed_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &etc1_compressed_alpha.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Etc1CompressedAlpha,
                label: Some("ETC1 compressed alpha GPU readback"),
            })
            .await
            .unwrap();
        let etc1_compressed_cpu_rgba = PtxDecoder::decode_rgba8(
            &etc1_compressed_alpha.data,
            8,
            8,
            147,
            Some(32),
            Some(1),
            None,
            false,
        )
        .unwrap()
        .into_raw();
        assert_pixels_close(&etc1_compressed_gpu_rgba, &etc1_compressed_cpu_rgba, 1);

        let etc1_palette_gpu_rgba = codec
            .decode_rgba8(PtxTextureDescriptor {
                data: &etc1_palette.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Etc1Palette,
                label: Some("ETC1 palette GPU readback"),
            })
            .await
            .unwrap();
        let etc1_palette_cpu_rgba =
            PtxDecoder::decode_rgba8(&etc1_palette.data, 8, 8, 30, None, Some(1), None, false)
                .unwrap()
                .into_raw();
        assert_pixels_close(&etc1_palette_gpu_rgba, &etc1_palette_cpu_rgba, 1);

        let odd = test_image(5, 7);
        for (format, expected_len) in [
            (PtxFormat::Etc1A8, 67),
            (PtxFormat::Etc1CompressedAlpha, 64),
            (PtxFormat::Etc1Palette, 67),
        ] {
            let encoded = codec
                .encode_fast_rgba8(odd.as_raw(), 5, 7, GpuEncodeOptions::new(format))
                .await
                .unwrap();
            assert_eq!(encoded.data.len(), expected_len, "{format:?}");
            let actual = codec
                .decode_rgba8(PtxTextureDescriptor {
                    data: &encoded.data,
                    width: 5,
                    height: 7,
                    format,
                    label: Some("odd-size ETC1 GPU readback"),
                })
                .await
                .unwrap();
            let expected = PtxDecoder::decode_with_descriptor(
                &encoded.data,
                PtxDescriptor::new(5, 7, format).unwrap(),
            )
            .unwrap()
            .into_raw();
            assert_pixels_close(&actual, &expected, 1);
        }

        let etc_texture = codec
            .upload(PtxTextureDescriptor {
                data: &etc1.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Etc1,
                label: Some("ETC1 GPU test texture"),
            })
            .unwrap();
        let pvrtc_texture = codec
            .upload(PtxTextureDescriptor {
                data: &pvrtc.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Pvrtc4BppRgba,
                label: Some("PVRTC GPU test texture"),
            })
            .unwrap();
        let astc_texture = codec
            .upload(PtxTextureDescriptor {
                data: &astc.data,
                width: 8,
                height: 8,
                format: PtxGpuFormat::Astc {
                    block_width: 4,
                    block_height: 4,
                },
                label: Some("ASTC GPU test texture"),
            })
            .unwrap();
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let upload_error = upload_errors.pop().await;
        assert!(
            upload_error.is_none(),
            "compute decoding and texture upload must validate: {upload_error:?}"
        );
        assert_eq!(etc_texture.backend(), DecodeBackend::Compute);
        assert_eq!(pvrtc_texture.backend(), DecodeBackend::Compute);
        assert_eq!(astc_texture.backend(), DecodeBackend::CpuFallback);

        // The fast PVRTC shader intentionally follows the CPU encoder's two
        // passes, so deterministic inputs should remain byte-identical.
        assert_eq!(
            pvrtc.data,
            encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(&image), true).unwrap(),
            "GPU and CPU PVRTC packets diverged"
        );
        assert_eq!(
            pvrtc_a8.data,
            PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Pvrtc4BppRgbaA8),)
                .unwrap(),
            "GPU and CPU PVRTC+A8 payloads diverged"
        );
        let cpu_palette =
            PtxEncoder::encode_image(&image, PtxEncodeOptions::new(PtxFormat::Etc1Palette))
                .unwrap();
        assert_eq!(
            &etc1_palette.data[32..],
            &cpu_palette[32..],
            "GPU and CPU ETC1 palette alpha streams diverged"
        );

        // Ensure the high-quality CPU path remains independently usable with
        // the same format selected for an interactive GPU preview.
        assert_eq!(
            encode_astc_rgba8(Rgba8Surface::from_image(&image), 4, 4, AstcQuality::FASTEST,)
                .unwrap()
                .len(),
            astc.data.len()
        );

        if let Ok((fallback_device, fallback_queue)) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rsb-archive WebGL fallback tests"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
        {
            let fallback_codec = PtxGpuCodec::new(&fallback_device, &fallback_queue);
            assert!(!fallback_codec.supports_compute());
            let encoded = fallback_codec
                .encode_fast_rgba8(
                    image.as_raw(),
                    8,
                    8,
                    GpuEncodeOptions::new(PtxGpuFormat::Etc1),
                )
                .await
                .unwrap();
            assert_eq!(encoded.backend, EncodeBackend::CpuFallback);
            let compressed_alpha = fallback_codec
                .encode_fast_rgba8(
                    image.as_raw(),
                    8,
                    8,
                    GpuEncodeOptions::new(PtxGpuFormat::Etc1CompressedAlpha),
                )
                .await
                .unwrap();
            assert_eq!(compressed_alpha.backend, EncodeBackend::CpuFallback);
            assert_eq!(compressed_alpha.data.len(), 64);
            let texture = fallback_codec
                .upload(PtxTextureDescriptor {
                    data: &pvrtc.data,
                    width: 8,
                    height: 8,
                    format: PtxGpuFormat::Pvrtc4BppRgba,
                    label: Some("PVRTC WebGL fallback texture"),
                })
                .unwrap();
            assert_eq!(texture.backend(), DecodeBackend::CpuFallback);
            let palette_texture = fallback_codec
                .upload(PtxTextureDescriptor {
                    data: &etc1_palette.data,
                    width: 8,
                    height: 8,
                    format: PtxGpuFormat::Etc1Palette,
                    label: Some("ETC1 palette WebGL fallback texture"),
                })
                .unwrap();
            assert_eq!(palette_texture.backend(), DecodeBackend::CpuFallback);
        }

        let compressed_features = supported_optional_features(&adapter);
        if !compressed_features.is_empty()
            && let Ok((hardware_device, hardware_queue)) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("rsb-archive compressed texture tests"),
                    required_features: compressed_features,
                    required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    trace: wgpu::Trace::Off,
                })
                .await
        {
            let hardware_errors = hardware_device.push_error_scope(wgpu::ErrorFilter::Validation);
            let hardware_codec = PtxGpuCodec::new(&hardware_device, &hardware_queue);
            if compressed_features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC) {
                let texture = hardware_codec
                    .upload(PtxTextureDescriptor {
                        data: &astc.data,
                        width: 8,
                        height: 8,
                        format: PtxGpuFormat::Astc {
                            block_width: 4,
                            block_height: 4,
                        },
                        label: Some("hardware ASTC test texture"),
                    })
                    .unwrap();
                assert_eq!(texture.backend(), DecodeBackend::HardwareCompressed);
                let actual = hardware_codec
                    .decode_rgba8(PtxTextureDescriptor {
                        data: &astc.data,
                        width: 8,
                        height: 8,
                        format: PtxGpuFormat::Astc {
                            block_width: 4,
                            block_height: 4,
                        },
                        label: Some("hardware ASTC readback"),
                    })
                    .await
                    .unwrap();
                let expected = decode_astc_rgba8(&astc.data, 8, 8, 4, 4)
                    .unwrap()
                    .into_raw();
                assert_pixels_close(&actual, &expected, 1);
            }
            if compressed_features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
                let texture = hardware_codec
                    .upload(PtxTextureDescriptor {
                        data: &etc1.data,
                        width: 8,
                        height: 8,
                        format: PtxGpuFormat::Etc1,
                        label: Some("hardware ETC1 test texture"),
                    })
                    .unwrap();
                assert_eq!(texture.backend(), DecodeBackend::HardwareCompressed);
                let actual = hardware_codec
                    .decode_rgba8(PtxTextureDescriptor {
                        data: &etc1.data,
                        width: 8,
                        height: 8,
                        format: PtxGpuFormat::Etc1,
                        label: Some("hardware ETC1 readback"),
                    })
                    .await
                    .unwrap();
                assert_pixels_close(&actual, &etc_cpu_rgba, 1);
            }
            let _ = hardware_device.poll(wgpu::PollType::wait_indefinitely());
            let hardware_error = hardware_errors.pop().await;
            assert!(
                hardware_error.is_none(),
                "hardware compressed paths must validate: {hardware_error:?}"
            );
        }
    });
}

fn test_image(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, y| {
        Rgba([
            (x * 31 + y * 7) as u8,
            (x * 11 + y * 29) as u8,
            (x * 19 + y * 13) as u8,
            64 + (x * 9 + y * 5) as u8,
        ])
    })
}

fn assert_pixels_close(actual: &[u8], expected: &[u8], tolerance: u8) {
    assert_eq!(actual.len(), expected.len());
    let maximum = actual
        .iter()
        .zip(expected)
        .map(|(&actual, &expected)| actual.abs_diff(expected))
        .max()
        .unwrap_or(0);
    assert!(
        maximum <= tolerance,
        "maximum channel difference {maximum} exceeds tolerance {tolerance}"
    );
}
