pub(super) const WORKGROUP_SIZE: u32 = 8;

pub(super) struct Pipelines {
    pub decode_etc1: wgpu::ComputePipeline,
    pub decode_pvrtc: wgpu::ComputePipeline,
    pub encode_astc: wgpu::ComputePipeline,
    pub encode_alpha: wgpu::ComputePipeline,
    pub encode_etc1: wgpu::ComputePipeline,
    pub encode_pvrtc_endpoints: wgpu::ComputePipeline,
    pub encode_pvrtc_modulation: wgpu::ComputePipeline,
}

impl Pipelines {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            decode_etc1: create_pipeline(
                device,
                "rsb-archive ETC1 compute decoder",
                include_str!("shaders/decode_etc1.wgsl"),
            ),
            decode_pvrtc: create_pipeline(
                device,
                "rsb-archive PVRTC compute decoder",
                include_str!("shaders/decode_pvrtc.wgsl"),
            ),
            encode_astc: create_pipeline(
                device,
                "rsb-archive ASTC fast compute encoder",
                include_str!("shaders/encode_astc.wgsl"),
            ),
            encode_alpha: create_pipeline(
                device,
                "rsb-archive alpha plane compute encoder",
                include_str!("shaders/encode_alpha.wgsl"),
            ),
            encode_etc1: create_pipeline(
                device,
                "rsb-archive ETC1 fast compute encoder",
                include_str!("shaders/encode_etc1.wgsl"),
            ),
            encode_pvrtc_endpoints: create_pipeline(
                device,
                "rsb-archive PVRTC endpoint compute encoder",
                include_str!("shaders/encode_pvrtc_endpoints.wgsl"),
            ),
            encode_pvrtc_modulation: create_pipeline(
                device,
                "rsb-archive PVRTC modulation compute encoder",
                include_str!("shaders/encode_pvrtc_modulation.wgsl"),
            ),
        }
    }
}

pub(super) const fn supports_compute_limits(limits: &wgpu::Limits) -> bool {
    limits.max_compute_workgroup_size_x >= WORKGROUP_SIZE
        && limits.max_compute_workgroup_size_y >= WORKGROUP_SIZE
        && limits.max_compute_workgroup_size_z >= 1
        && limits.max_compute_invocations_per_workgroup >= WORKGROUP_SIZE * WORKGROUP_SIZE
        && limits.max_compute_workgroups_per_dimension > 0
        && limits.max_storage_buffers_per_shader_stage >= 2
}

fn create_pipeline(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}
