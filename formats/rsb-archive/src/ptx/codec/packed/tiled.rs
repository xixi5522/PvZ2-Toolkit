use image::RgbaImage;

use super::PackedPixelFormat;
use crate::ptx::error::{PtxError, Result};
use crate::ptx::{PtxDescriptor, PtxPayloadLayout, Rgba8Surface};

const TILE_WIDTH: u32 = 32;
const TILE_HEIGHT: u32 = 32;

pub(crate) fn encode_tiled(
    surface: Rgba8Surface<'_>,
    descriptor: PtxDescriptor,
) -> Result<Vec<u8>> {
    let format = PackedPixelFormat::from_ptx(descriptor.format)?;
    let layout = PtxPayloadLayout::for_encode(descriptor)?;
    let mut output = Vec::with_capacity(layout.total_len);
    for tile_y in 0..descriptor.height.div_ceil(TILE_HEIGHT) {
        for tile_x in 0..descriptor.width.div_ceil(TILE_WIDTH) {
            for local_y in 0..TILE_HEIGHT {
                for local_x in 0..TILE_WIDTH {
                    let x = tile_x * TILE_WIDTH + local_x;
                    let y = tile_y * TILE_HEIGHT + local_y;
                    let pixel = if x < descriptor.width && y < descriptor.height {
                        surface.pixel(x, y)
                    } else {
                        [0, 0, 0, 0]
                    };
                    let (bytes, len) = format.encode(pixel, descriptor.channel_order);
                    output.extend_from_slice(&bytes[..len]);
                }
            }
        }
    }
    debug_assert_eq!(output.len(), layout.total_len);
    Ok(output)
}

pub(crate) fn decode_tiled(data: &[u8], descriptor: PtxDescriptor) -> Result<RgbaImage> {
    let format = PackedPixelFormat::from_ptx(descriptor.format)?;
    PtxPayloadLayout::for_decode(descriptor, data)?;
    let output_len =
        usize::try_from(u64::from(descriptor.width) * u64::from(descriptor.height) * 4)
            .map_err(|_| PtxError::DimensionsOverflow)?;
    let mut output = vec![0; output_len];
    let mut offset = 0;
    for tile_y in 0..descriptor.height.div_ceil(TILE_HEIGHT) {
        for tile_x in 0..descriptor.width.div_ceil(TILE_WIDTH) {
            for local_y in 0..TILE_HEIGHT {
                for local_x in 0..TILE_WIDTH {
                    let pixel = format.decode(
                        &data[offset..offset + format.byte_len()],
                        descriptor.channel_order,
                    );
                    offset += format.byte_len();
                    let x = tile_x * TILE_WIDTH + local_x;
                    let y = tile_y * TILE_HEIGHT + local_y;
                    if x < descriptor.width && y < descriptor.height {
                        let target = (y as usize * descriptor.width as usize + x as usize) * 4;
                        output[target..target + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
    }
    RgbaImage::from_raw(descriptor.width, descriptor.height, output)
        .ok_or(PtxError::DimensionsOverflow)
}
