use wgpu::util::DeviceExt;

use super::dispatch::{create_uniform, dispatch};
use super::params::{AlphaEncodeParams, BlockEncodeParams};
use super::readback::read_buffer;
use super::util::{align_usize, usize_to_u32};
use super::{EncodeBackend, GpuEncodeOptions, GpuEncodedPtx, PtxGpuCodec, PtxGpuError, Result};
use crate::ptx::{
    AstcQuality, PtxDescriptor, PtxEncodeOptions, PtxEncoder, PtxFormat, PtxPayloadLayout,
    Rgba8Surface, encode_pvrtc_4bpp_rgba8,
};

impl PtxGpuCodec {
    /// Encodes tightly packed RGBA8 pixels using the GPU fast path when available.
    pub async fn encode_fast_rgba8(
        &self,
        rgba8: &[u8],
        width: u32,
        height: u32,
        options: GpuEncodeOptions,
    ) -> Result<GpuEncodedPtx> {
        if !options.format.is_gpu_supported() {
            return Err(PtxGpuError::UnsupportedFormat);
        }
        let stride = width
            .checked_mul(4)
            .ok_or(PtxGpuError::DimensionsOverflow)?;
        let surface = Rgba8Surface::new(rgba8, width, height, stride)?;
        let descriptor = PtxDescriptor::new(width, height, options.format)?;
        let layout = PtxPayloadLayout::for_encode(descriptor)?;
        let Some(pipelines) = self.pipelines.as_ref() else {
            return self.encode_cpu_fallback(surface, options);
        };

        let pixels = width as usize * height as usize;
        let color_size = layout.color.end;
        let palette_stream_size = pixels.div_ceil(2);
        let palette_output = options.format == PtxFormat::Etc1Palette;
        let gpu_output_size = if palette_output {
            color_size
                .checked_add(palette_stream_size)
                .ok_or(PtxGpuError::DimensionsOverflow)?
        } else {
            layout.total_len
        };
        let padded_output_size = align_usize(gpu_output_size, 4)?;
        let input = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rsb-archive PTX GPU encoder input"),
                contents: rgba8,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: options.label.or(Some("rsb-archive PTX GPU encoder output")),
            size: padded_output_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsb-archive PTX GPU encode commands"),
            });

        match options.format {
            PtxFormat::Astc {
                block_width,
                block_height,
            } => {
                let params = block_params(
                    width,
                    height,
                    block_width,
                    block_height,
                    0,
                    u32::from(options.include_alpha),
                );
                dispatch_block(
                    &self.device,
                    &mut encoder,
                    &pipelines.encode_astc,
                    &input,
                    &output,
                    params,
                    "rsb-archive ASTC fast encode pass",
                );
            }
            PtxFormat::Etc1
            | PtxFormat::Etc1A8
            | PtxFormat::Etc1CompressedAlpha
            | PtxFormat::Etc1Palette => {
                let color_params = block_params(width, height, 4, 4, 0, 0);
                dispatch_block(
                    &self.device,
                    &mut encoder,
                    &pipelines.encode_etc1,
                    &input,
                    &output,
                    color_params,
                    "rsb-archive ETC1 fast encode pass",
                );
                match options.format {
                    PtxFormat::Etc1A8 => self.dispatch_alpha(
                        &mut encoder,
                        &pipelines.encode_alpha,
                        &input,
                        &output,
                        surface,
                        color_size,
                        1,
                    )?,
                    PtxFormat::Etc1CompressedAlpha => {
                        let alpha_params =
                            block_params(width, height, 4, 4, usize_to_u32(color_size)? / 4, 2);
                        dispatch_block(
                            &self.device,
                            &mut encoder,
                            &pipelines.encode_etc1,
                            &input,
                            &output,
                            alpha_params,
                            "rsb-archive ETC1 compressed alpha encode pass",
                        );
                    }
                    PtxFormat::Etc1Palette => self.dispatch_alpha(
                        &mut encoder,
                        &pipelines.encode_alpha,
                        &input,
                        &output,
                        surface,
                        color_size,
                        3,
                    )?,
                    _ => {}
                }
            }
            PtxFormat::Pvrtc4BppRgba | PtxFormat::Pvrtc4BppRgbaA8 => {
                let params = block_params(
                    width,
                    height,
                    4,
                    4,
                    0,
                    u32::from(options.include_alpha && options.format == PtxFormat::Pvrtc4BppRgba),
                );
                dispatch_block(
                    &self.device,
                    &mut encoder,
                    &pipelines.encode_pvrtc_endpoints,
                    &input,
                    &output,
                    params,
                    "rsb-archive PVRTC endpoint pass",
                );
                dispatch_block(
                    &self.device,
                    &mut encoder,
                    &pipelines.encode_pvrtc_modulation,
                    &input,
                    &output,
                    params,
                    "rsb-archive PVRTC modulation pass",
                );
                if options.format == PtxFormat::Pvrtc4BppRgbaA8 {
                    self.dispatch_alpha(
                        &mut encoder,
                        &pipelines.encode_alpha,
                        &input,
                        &output,
                        surface,
                        color_size,
                        1,
                    )?;
                }
            }
            _ => return Err(PtxGpuError::UnsupportedFormat),
        }

        self.queue.submit(Some(encoder.finish()));
        let mut data = read_buffer(
            &self.device,
            &self.queue,
            &output,
            padded_output_size as u64,
            "rsb-archive PTX GPU encode readback",
        )
        .await?;
        data.truncate(gpu_output_size);
        if palette_output {
            let stream = data.split_off(color_size);
            data.push(16);
            data.extend(0u8..16);
            data.extend_from_slice(&stream[..palette_stream_size]);
        }
        Ok(GpuEncodedPtx {
            data,
            backend: EncodeBackend::ComputeFast,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_alpha(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        input: &wgpu::Buffer,
        output: &wgpu::Buffer,
        surface: Rgba8Surface<'_>,
        output_offset: usize,
        mode: u32,
    ) -> Result<()> {
        let pixels_per_word = if mode == 3 { 8 } else { 4 };
        let pixels = surface.width() as usize * surface.height() as usize;
        let word_count = usize_to_u32(pixels.div_ceil(pixels_per_word))?;
        let params = AlphaEncodeParams {
            width: surface.width(),
            height: surface.height(),
            reserved_0: 0,
            reserved_1: 0,
            output_word_count: word_count,
            reserved_2: 0,
            output_offset_words: usize_to_u32(output_offset)? / 4,
            mode,
        };
        let uniform = create_uniform(
            &self.device,
            &params,
            "rsb-archive alpha plane encode parameters",
        );
        dispatch(
            &self.device,
            encoder,
            pipeline,
            input,
            output,
            &uniform,
            word_count,
            1,
            "rsb-archive alpha plane encode pass",
        );
        Ok(())
    }

    fn encode_cpu_fallback(
        &self,
        surface: Rgba8Surface<'_>,
        options: GpuEncodeOptions,
    ) -> Result<GpuEncodedPtx> {
        let data = if options.format == PtxFormat::Pvrtc4BppRgba && !options.include_alpha {
            encode_pvrtc_4bpp_rgba8(surface, false)?
        } else {
            let mut cpu_options = PtxEncodeOptions::new(options.format);
            cpu_options.astc_quality = AstcQuality::FASTEST;
            PtxEncoder::encode_surface(surface, cpu_options)?
        };
        Ok(GpuEncodedPtx {
            data,
            backend: EncodeBackend::CpuFallback,
        })
    }
}

fn block_params(
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    output_offset_words: u32,
    flags: u32,
) -> BlockEncodeParams {
    BlockEncodeParams {
        width,
        height,
        block_width,
        block_height,
        blocks_x: width.div_ceil(block_width),
        blocks_y: height.div_ceil(block_height),
        output_offset_words,
        flags,
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_block(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
    params: BlockEncodeParams,
    label: &'static str,
) {
    let uniform = create_uniform(device, &params, "rsb-archive block encode parameters");
    dispatch(
        device,
        encoder,
        pipeline,
        input,
        output,
        &uniform,
        params.blocks_x,
        params.blocks_y,
        label,
    );
}
