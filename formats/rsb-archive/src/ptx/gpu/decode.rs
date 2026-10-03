use wgpu::util::DeviceExt;

use super::dispatch::{create_uniform, dispatch};
use super::params::DecodeParams;
use super::readback::map_buffer;
use super::util::{align_u32, usize_to_u32};
use super::{
    DecodeBackend, PreviewOptions, PtxGpuCodec, PtxGpuError, PtxGpuTexture, PtxPreviewRenderer,
    PtxTextureDescriptor, Result,
};
use crate::ptx::{PtxDecoder, PtxDescriptor, PtxFormat, PtxPayload};

impl PtxGpuCodec {
    pub fn upload(&self, descriptor: PtxTextureDescriptor<'_>) -> Result<PtxGpuTexture> {
        if !descriptor.format.is_gpu_supported() {
            return Err(PtxGpuError::UnsupportedFormat);
        }
        let ptx_descriptor =
            PtxDescriptor::new(descriptor.width, descriptor.height, descriptor.format)?;
        let payload = PtxPayload::parse(descriptor.data, ptx_descriptor)?;

        match descriptor.format {
            PtxFormat::Astc {
                block_width,
                block_height,
            } if self
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC) =>
            {
                Ok(self.upload_compressed(
                    &payload,
                    descriptor.label,
                    astc_texture_format(block_width, block_height)?,
                    block_width,
                    block_height,
                    16,
                ))
            }
            PtxFormat::Etc1
                if self
                    .device
                    .features()
                    .contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) =>
            {
                Ok(self.upload_compressed(
                    &payload,
                    descriptor.label,
                    wgpu::TextureFormat::Etc2Rgb8UnormSrgb,
                    4,
                    4,
                    8,
                ))
            }
            PtxFormat::Etc1
            | PtxFormat::Etc1A8
            | PtxFormat::Etc1CompressedAlpha
            | PtxFormat::Etc1Palette
                if self.pipelines.is_some() =>
            {
                let alpha_mode = match descriptor.format {
                    PtxFormat::Etc1 => 0,
                    PtxFormat::Etc1A8 => 1,
                    PtxFormat::Etc1CompressedAlpha => 2,
                    PtxFormat::Etc1Palette => 3,
                    _ => unreachable!(),
                };
                Ok(self.decode_compute(
                    &payload,
                    descriptor.label,
                    &self
                        .pipelines
                        .as_ref()
                        .expect("pipeline presence checked")
                        .decode_etc1,
                    alpha_mode,
                )?)
            }
            PtxFormat::Pvrtc4BppRgba | PtxFormat::Pvrtc4BppRgbaA8 if self.pipelines.is_some() => {
                Ok(self.decode_compute(
                    &payload,
                    descriptor.label,
                    &self
                        .pipelines
                        .as_ref()
                        .expect("pipeline presence checked")
                        .decode_pvrtc,
                    u32::from(descriptor.format == PtxFormat::Pvrtc4BppRgbaA8),
                )?)
            }
            PtxFormat::Astc { .. }
            | PtxFormat::Etc1
            | PtxFormat::Etc1A8
            | PtxFormat::Etc1CompressedAlpha
            | PtxFormat::Etc1Palette
            | PtxFormat::Pvrtc4BppRgba
            | PtxFormat::Pvrtc4BppRgbaA8 => {
                let rgba = PtxDecoder::decode_payload(&payload)?;
                Ok(self.upload_rgba(
                    rgba.as_raw(),
                    descriptor.width,
                    descriptor.height,
                    descriptor.label,
                    DecodeBackend::CpuFallback,
                ))
            }
            _ => Err(PtxGpuError::UnsupportedFormat),
        }
    }

    /// Decodes a PTX texture into tightly packed RGBA8 pixels.
    pub async fn decode_rgba8(&self, descriptor: PtxTextureDescriptor<'_>) -> Result<Vec<u8>> {
        let width = descriptor.width;
        let height = descriptor.height;
        let texture = self.upload(descriptor)?;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsb-archive PTX RGBA decode target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let renderer = PtxPreviewRenderer::new(&self.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let padded_bytes_per_row = align_u32(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsb-archive PTX RGBA readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsb-archive PTX RGBA decode commands"),
            });
        let mut preview = PreviewOptions::new(width, height);
        preview.composite_alpha = false;
        renderer.render(&mut encoder, &target_view, &texture, preview);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let padded = map_buffer(&self.device, &readback).await?;
        let row_len = width as usize * 4;
        let mut rgba = Vec::with_capacity(row_len * height as usize);
        for row in padded.chunks_exact(padded_bytes_per_row as usize) {
            rgba.extend_from_slice(&row[..row_len]);
        }
        Ok(rgba)
    }

    fn decode_compute(
        &self,
        payload: &PtxPayload<'_>,
        label: Option<&str>,
        pipeline: &wgpu::ComputePipeline,
        alpha_mode: u32,
    ) -> Result<PtxGpuTexture> {
        let descriptor = payload.descriptor();
        let padded_bytes_per_row =
            align_u32(descriptor.width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let params = DecodeParams {
            width: descriptor.width,
            height: descriptor.height,
            alpha_offset: if payload.layout().alpha.is_some() {
                usize_to_u32(payload.layout().color.end)?
            } else {
                0
            },
            reserved: 0,
            blocks_x: descriptor.width.div_ceil(4),
            blocks_y: descriptor.height.div_ceil(4),
            output_stride_words: padded_bytes_per_row / 4,
            alpha_mode,
        };
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rsb-archive compressed PTX input"),
                contents: payload.data(),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsb-archive decoded PTX pixels"),
            size: u64::from(padded_bytes_per_row) * u64::from(descriptor.height),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let uniform = create_uniform(&self.device, &params, "rsb-archive PTX decode parameters");
        let texture = self.create_rgba_texture(
            descriptor.width,
            descriptor.height,
            label,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsb-archive PTX decode commands"),
            });
        dispatch(
            &self.device,
            &mut encoder,
            pipeline,
            &input,
            &output,
            &uniform,
            descriptor.width,
            descriptor.height,
            "rsb-archive PTX compute decode pass",
        );
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(descriptor.height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: descriptor.width,
                height: descriptor.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(PtxGpuTexture::new(
            texture,
            view,
            descriptor.width,
            descriptor.height,
            DecodeBackend::Compute,
        ))
    }

    fn upload_compressed(
        &self,
        payload: &PtxPayload<'_>,
        label: Option<&str>,
        format: wgpu::TextureFormat,
        block_width: u32,
        block_height: u32,
        bytes_per_block: u32,
    ) -> PtxGpuTexture {
        let descriptor = payload.descriptor();
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width: descriptor.width,
                height: descriptor.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            payload.color(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(descriptor.width.div_ceil(block_width) * bytes_per_block),
                rows_per_image: Some(descriptor.height.div_ceil(block_height)),
            },
            wgpu::Extent3d {
                width: descriptor.width,
                height: descriptor.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        PtxGpuTexture::new(
            texture,
            view,
            descriptor.width,
            descriptor.height,
            DecodeBackend::HardwareCompressed,
        )
    }

    fn upload_rgba(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: Option<&str>,
        backend: DecodeBackend,
    ) -> PtxGpuTexture {
        let texture = self.create_rgba_texture(
            width,
            height,
            label,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        PtxGpuTexture::new(texture, view, width, height, backend)
    }

    fn create_rgba_texture(
        &self,
        width: u32,
        height: u32,
        label: Option<&str>,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage,
            view_formats: &[],
        })
    }
}

fn astc_texture_format(width: u32, height: u32) -> Result<wgpu::TextureFormat> {
    use wgpu::AstcBlock::*;
    let block = match (width, height) {
        (4, 4) => B4x4,
        (5, 4) => B5x4,
        (5, 5) => B5x5,
        (6, 5) => B6x5,
        (6, 6) => B6x6,
        (8, 5) => B8x5,
        (8, 6) => B8x6,
        (8, 8) => B8x8,
        (10, 5) => B10x5,
        (10, 6) => B10x6,
        (10, 8) => B10x8,
        (10, 10) => B10x10,
        (12, 10) => B12x10,
        (12, 12) => B12x12,
        _ => return Err(PtxGpuError::InvalidAstcFootprint { width, height }),
    };
    Ok(wgpu::TextureFormat::Astc {
        block,
        channel: wgpu::AstcChannel::UnormSrgb,
    })
}
