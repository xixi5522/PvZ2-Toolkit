use super::pipelines::{Pipelines, supports_compute_limits};

/// Reusable PTX codec and renderer resources bound to one WGPU device.
pub struct PtxGpuCodec {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) pipelines: Option<Pipelines>,
}

impl PtxGpuCodec {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            pipelines: supports_compute_limits(&device.limits()).then(|| Pipelines::new(device)),
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub const fn supports_compute(&self) -> bool {
        self.pipelines.is_some()
    }
}
