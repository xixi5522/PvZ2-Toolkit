use super::{PtxGpuError, Result};
use futures_channel::oneshot;

pub(crate) async fn read_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
    label: &str,
) -> Result<Vec<u8>> {
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
    encoder.copy_buffer_to_buffer(source, 0, &readback, 0, size);
    queue.submit(Some(encoder.finish()));

    map_buffer(device, &readback).await
}

pub(crate) async fn map_buffer(_device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<Vec<u8>> {
    let slice = buffer.slice(..);
    let (sender, receiver) = oneshot::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    #[cfg(not(target_arch = "wasm32"))]
    let _ = _device.poll(wgpu::PollType::wait_indefinitely());
    receiver.await.map_err(|_| PtxGpuError::CallbackDropped)??;
    let mapped = slice
        .get_mapped_range()
        .map_err(|error| PtxGpuError::BufferAccess(error.to_string()))?;
    let result = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    Ok(result)
}
