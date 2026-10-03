use super::decoder_core::{Footprint, astc_decode_block};
use super::format::astc_data_size;
use crate::error::{Result, RsbError};
use image::{Rgba, RgbaImage};

/// Decodes a raw ASTC LDR payload into canonical RGBA8 pixels.
pub fn decode_astc_rgba8(
    data: &[u8],
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
) -> Result<RgbaImage> {
    let expected = astc_data_size(width, height, block_width, block_height)?;
    if data.len() != expected {
        return Err(RsbError::InvalidAstcDataSize {
            expected,
            actual: data.len(),
        });
    }

    let footprint = Footprint::new(block_width, block_height);
    let mut image = RgbaImage::new(width, height);
    let blocks_x = width.div_ceil(block_width);
    for (block_index, block) in data.as_chunks::<16>().0.iter().enumerate() {
        let block_x = block_index as u32 % blocks_x;
        let block_y = block_index as u32 / blocks_x;
        let valid = astc_decode_block(block, footprint, |x, y, color| {
            let output_x = block_x * block_width + x;
            let output_y = block_y * block_height + y;
            if output_x < width && output_y < height {
                image.put_pixel(output_x, output_y, Rgba(color));
            }
        });
        if !valid {
            return Err(RsbError::Astc(format!(
                "illegal ASTC block at ({block_x}, {block_y})"
            )));
        }
    }
    Ok(image)
}
