use image::RgbaImage;

use super::{decode_etc1_block, encode_etc1_alpha_block, encode_etc1_block};
use crate::ptx::Rgba8Surface;
use crate::ptx::color::Rgba8;
use crate::ptx::error::{PtxError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Etc1Plane {
    Color,
    Alpha,
}

pub(crate) fn encode_plane(surface: Rgba8Surface<'_>, plane: Etc1Plane) -> Result<Vec<u8>> {
    let blocks_x = surface.width().div_ceil(4);
    let blocks_y = surface.height().div_ceil(4);
    let capacity = usize::try_from(u64::from(blocks_x) * u64::from(blocks_y) * 8)
        .map_err(|_| PtxError::DimensionsOverflow)?;
    let mut output = Vec::with_capacity(capacity);
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            let mut pixels = [Rgba8::new(0, 0, 0, 255); 16];
            for y in 0..4 {
                for x in 0..4 {
                    let source_x = block_x * 4 + x;
                    let source_y = block_y * 4 + y;
                    if source_x < surface.width() && source_y < surface.height() {
                        let [r, g, b, a] = surface.pixel(source_x, source_y);
                        pixels[(y * 4 + x) as usize] = Rgba8::new(r, g, b, a);
                    }
                }
            }
            let block = match plane {
                Etc1Plane::Color => encode_etc1_block(&pixels),
                Etc1Plane::Alpha => encode_etc1_alpha_block(&pixels),
            };
            output.extend_from_slice(&block.to_be_bytes());
        }
    }
    Ok(output)
}

pub(crate) fn decode_plane(data: &[u8], width: u32, height: u32) -> Result<RgbaImage> {
    let expected =
        usize::try_from(u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4)) * 8)
            .map_err(|_| PtxError::DimensionsOverflow)?;
    if data.len() != expected {
        return Err(PtxError::InvalidPayloadSize {
            format: crate::ptx::PtxFormat::Etc1,
            expected,
            actual: data.len(),
        });
    }
    let output_len = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| PtxError::DimensionsOverflow)?;
    let mut output = vec![0; output_len];
    let mut offset = 0;
    for block_y in 0..height.div_ceil(4) {
        for block_x in 0..width.div_ceil(4) {
            let block = u64::from_be_bytes(
                data[offset..offset + 8]
                    .try_into()
                    .expect("ETC1 plane length was validated"),
            );
            offset += 8;
            let mut decoded = [Rgba8::default(); 16];
            decode_etc1_block(block, &mut decoded);
            for y in 0..4 {
                for x in 0..4 {
                    let target_x = block_x * 4 + x;
                    let target_y = block_y * 4 + y;
                    if target_x < width && target_y < height {
                        // The ported ETC1 block decoder exposes its 4x4 result
                        // in column-major order.
                        let pixel = decoded[(x * 4 + y) as usize];
                        let target = (target_y as usize * width as usize + target_x as usize) * 4;
                        output[target..target + 4]
                            .copy_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
                    }
                }
            }
        }
    }
    RgbaImage::from_raw(width, height, output).ok_or(PtxError::DimensionsOverflow)
}
