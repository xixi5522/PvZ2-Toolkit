use image::RgbaImage;

use super::PackedPixelFormat;
use crate::ptx::error::{PtxError, Result};
use crate::ptx::{PtxDescriptor, PtxPayloadLayout, Rgba8Surface};

pub(crate) fn encode_linear(
    surface: Rgba8Surface<'_>,
    descriptor: PtxDescriptor,
) -> Result<Vec<u8>> {
    let format = PackedPixelFormat::from_ptx(descriptor.format)?;
    let layout = PtxPayloadLayout::for_encode(descriptor)?;
    let row_bytes = surface.width() as usize * format.byte_len();
    let pitch = descriptor.row_pitch.unwrap_or(row_bytes as u32) as usize;
    let mut output = vec![0; layout.total_len];
    for y in 0..surface.height() {
        let row_offset = y as usize * pitch;
        for x in 0..surface.width() {
            let (bytes, len) = format.encode(surface.pixel(x, y), descriptor.channel_order);
            let offset = row_offset + x as usize * len;
            output[offset..offset + len].copy_from_slice(&bytes[..len]);
        }
    }
    Ok(output)
}

pub(crate) fn decode_linear(data: &[u8], descriptor: PtxDescriptor) -> Result<RgbaImage> {
    let format = PackedPixelFormat::from_ptx(descriptor.format)?;
    let layout = PtxPayloadLayout::for_decode(descriptor, data)?;
    if data.len() != layout.total_len {
        return Err(PtxError::InvalidPayloadSize {
            format: descriptor.format,
            expected: layout.total_len,
            actual: data.len(),
        });
    }
    let row_bytes = descriptor.width as usize * format.byte_len();
    let pitch = descriptor.row_pitch.unwrap_or(row_bytes as u32) as usize;
    let pixel_count = descriptor.width as usize * descriptor.height as usize;
    let mut output = Vec::with_capacity(pixel_count * 4);
    for y in 0..descriptor.height {
        let row_offset = y as usize * pitch;
        for x in 0..descriptor.width {
            let offset = row_offset + x as usize * format.byte_len();
            output.extend_from_slice(&format.decode(
                &data[offset..offset + format.byte_len()],
                descriptor.channel_order,
            ));
        }
    }
    RgbaImage::from_raw(descriptor.width, descriptor.height, output)
        .ok_or(PtxError::DimensionsOverflow)
}
