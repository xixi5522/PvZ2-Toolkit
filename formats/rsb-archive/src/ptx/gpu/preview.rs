use super::PtxGpuTexture;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PreviewUniform {
    scale: [f32; 2],
    padding: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct PreviewOptions {
    pub target_width: u32,
    pub target_height: u32,
    pub background: wgpu::Color,
    /// Composite straight-alpha PTX pixels over `background`.
    pub composite_alpha: bool,
}

impl PreviewOptions {
    pub const fn new(target_width: u32, target_height: u32) -> Self {
        Self {
            target_width,
            target_height,
            background: wgpu::Color::TRANSPARENT,
            composite_alpha: true,
        }
    }
}

/// Minimal aspect-fit renderer for a [`PtxGpuTexture`].
///
/// UI crates can render into a surface texture, an offscreen texture, or an
/// existing canvas attachment without knowing whether the source was sampled
/// as ASTC/ETC hardware compression or decoded by a compute/CPU fallback.
pub struct PtxPreviewRenderer {
    device: wgpu::Device,
    alpha_pipeline: wgpu::RenderPipeline,
    replace_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl PtxPreviewRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsb-archive PTX preview shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/preview.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rsb-archive PTX preview bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsb-archive PTX preview pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let alpha_pipeline = create_preview_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            "rsb-archive PTX alpha preview pipeline",
        );
        let replace_pipeline = create_preview_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            None,
            "rsb-archive PTX replace preview pipeline",
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rsb-archive PTX preview sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            device: device.clone(),
            alpha_pipeline,
            replace_pipeline,
            layout,
            sampler,
        }
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        texture: &PtxGpuTexture,
        options: PreviewOptions,
    ) {
        let scale = aspect_fit_scale(
            texture.width(),
            texture.height(),
            options.target_width,
            options.target_height,
        );
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rsb-archive PTX preview parameters"),
                contents: bytemuck::bytes_of(&PreviewUniform {
                    scale,
                    padding: [0.0; 2],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsb-archive PTX preview bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rsb-archive PTX preview pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(options.background),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(if options.composite_alpha {
            &self.alpha_pipeline
        } else {
            &self.replace_pipeline
        });
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

fn create_preview_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fragment_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn aspect_fit_scale(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> [f32; 2] {
    if source_width == 0 || source_height == 0 || target_width == 0 || target_height == 0 {
        return [0.0, 0.0];
    }
    let source_aspect = source_width as f32 / source_height as f32;
    let target_aspect = target_width as f32 / target_height as f32;
    if source_aspect > target_aspect {
        [1.0, target_aspect / source_aspect]
    } else {
        [source_aspect / target_aspect, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_fit_preserves_source_ratio() {
        assert_eq!(aspect_fit_scale(200, 100, 100, 100), [1.0, 0.5]);
        assert_eq!(aspect_fit_scale(100, 200, 100, 100), [0.5, 1.0]);
        assert_eq!(aspect_fit_scale(100, 100, 200, 100), [0.5, 1.0]);
    }
}
