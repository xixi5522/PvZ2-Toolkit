use super::layout::{BILINEAR_FACTORS, WEIGHTS, get_morton_number_rect};
use super::packet::PvrTcPacket;
use crate::error::{Result, RsbError};
use crate::ptx::color::Rgba8;
use image::RgbaImage;

pub fn decode_4bpp(packets: &[PvrTcPacket], width: i32) -> Vec<Rgba8> {
    decode_4bpp_rect(packets, width, width)
}

fn decode_4bpp_rect(packets: &[PvrTcPacket], width: i32, height: i32) -> Vec<Rgba8> {
    let blocks_x = width >> 2;
    let blocks_y = height >> 2;
    let block_mask_x = blocks_x - 1;
    let block_mask_y = blocks_y - 1;
    let mut result = vec![Rgba8::default(); (width * height) as usize];

    for y in 0..blocks_y {
        for x in 0..blocks_x {
            let packet = packets[get_morton_number_rect(x, y, blocks_x, blocks_y)];
            let mut mod_data = packet.modulation_data();

            let weight_index = if packet.use_punchthrough_alpha() {
                16
            } else {
                0
            };
            let mut factor_index = 0;

            for py in 0..4 {
                let y_offset = if py < 2 { -1 } else { 0 };
                let y0 = (y + y_offset) & block_mask_y;
                let y1 = (y0 + 1) & block_mask_y;

                for px in 0..4 {
                    let factor = BILINEAR_FACTORS[factor_index];
                    let x_offset = if px < 2 { -1 } else { 0 };
                    let x0 = (x + x_offset) & block_mask_x;
                    let x1 = (x0 + 1) & block_mask_x;

                    let p0 = packets[get_morton_number_rect(x0, y0, blocks_x, blocks_y)];
                    let p1 = packets[get_morton_number_rect(x1, y0, blocks_x, blocks_y)];
                    let p2 = packets[get_morton_number_rect(x0, y1, blocks_x, blocks_y)];
                    let p3 = packets[get_morton_number_rect(x1, y1, blocks_x, blocks_y)];

                    let ca = p0.get_color_a_rgba() * factor[0] as i32
                        + p1.get_color_a_rgba() * factor[1] as i32
                        + p2.get_color_a_rgba() * factor[2] as i32
                        + p3.get_color_a_rgba() * factor[3] as i32;

                    let cb = p0.get_color_b_rgba() * factor[0] as i32
                        + p1.get_color_b_rgba() * factor[1] as i32
                        + p2.get_color_b_rgba() * factor[2] as i32
                        + p3.get_color_b_rgba() * factor[3] as i32;

                    let index = weight_index + (((mod_data as i32) & 0b11) << 2) as usize;

                    let r = (ca.r * WEIGHTS[index] as i32 + cb.r * WEIGHTS[index + 1] as i32) >> 7;
                    let g = (ca.g * WEIGHTS[index] as i32 + cb.g * WEIGHTS[index + 1] as i32) >> 7;
                    let b = (ca.b * WEIGHTS[index] as i32 + cb.b * WEIGHTS[index + 1] as i32) >> 7;
                    let a =
                        (ca.a * WEIGHTS[index + 2] as i32 + cb.a * WEIGHTS[index + 3] as i32) >> 7;

                    let result_idx = ((py + (y << 2)) * width + px + (x << 2)) as usize;
                    result[result_idx] = Rgba8::new(r as u8, g as u8, b as u8, a as u8);

                    mod_data >>= 2;
                    factor_index += 1;
                }
            }
        }
    }
    result
}

/// Decodes PVRTC1 4bpp into canonical RGBA8 pixels.
pub fn decode_pvrtc_4bpp_rgba8(data: &[u8], width: u32, height: u32) -> Result<RgbaImage> {
    if width < 4 || height < 4 || !width.is_power_of_two() || !height.is_power_of_two() {
        return Err(RsbError::DeserializationError(format!(
            "PVRTC1 4bpp decoding requires power-of-two dimensions of at least 4x4 pixels, found {width}x{height}"
        )));
    }
    // PVRTC input data is packets.
    // 4bpp = 8 bytes per 4x4 block, aka 1 64-bit word per block.
    // Length check
    let expected_packets = (width * height / 16) as usize;
    if data.len() < expected_packets * 8 {
        return Err(RsbError::DeserializationError(
            "Insufficient data for PVRTC".into(),
        ));
    }

    let mut packets = Vec::with_capacity(expected_packets);
    for i in 0..expected_packets {
        let offset = i * 8;
        let word = u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        packets.push(PvrTcPacket::new(word));
    }

    let pixels = decode_4bpp_rect(&packets, width as i32, height as i32);

    let mut rgba8 = Vec::with_capacity(pixels.len() * 4);
    for pixel in pixels {
        rgba8.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
    }
    RgbaImage::from_raw(width, height, rgba8)
        .ok_or_else(|| RsbError::DeserializationError("PVRTC dimensions overflow".into()))
}

/// Decodes PVRTC1 4bpp plus an A8 plane into canonical RGBA8 pixels.
pub fn decode_pvrtc_4bpp_a8_rgba8(
    data_pvrtc: &[u8],
    data_alpha: &[u8],
    width: u32,
    height: u32,
) -> Result<RgbaImage> {
    let mut image = decode_pvrtc_4bpp_rgba8(data_pvrtc, width, height)?;

    if data_alpha.len() < (width * height) as usize {
        return Err(RsbError::DeserializationError(
            "Insufficient alpha data for PVRTC+A8".into(),
        ));
    }

    for (pixel, &alpha) in image.pixels_mut().zip(data_alpha) {
        pixel[3] = alpha;
    }
    Ok(image)
}
