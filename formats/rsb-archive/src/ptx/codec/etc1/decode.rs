use image::RgbaImage;

use super::{apply_a8, apply_etc1, decode_palette, decode_plane};
use crate::error::{Result, RsbError};

/// Decodes a color-only ETC1 payload into canonical RGBA8 pixels.
pub fn decode_etc1_rgba8(data: &[u8], width: u32, height: u32) -> Result<RgbaImage> {
    decode_plane(data, width, height).map_err(to_rsb_error)
}

/// Decodes ETC1 plus an A8 or ETC1-compressed alpha plane into RGBA8 pixels.
pub fn decode_etc1_a8_rgba8(
    data_color: &[u8],
    data_alpha: &[u8],
    width: u32,
    height: u32,
    compressed_alpha: bool,
) -> Result<RgbaImage> {
    let mut image = decode_plane(data_color, width, height).map_err(to_rsb_error)?;
    if compressed_alpha {
        apply_etc1(&mut image, data_alpha).map_err(to_rsb_error)?;
    } else {
        apply_a8(&mut image, data_alpha).map_err(to_rsb_error)?;
    }
    Ok(image)
}

/// Decodes the PvZ ETC1 palette-alpha layout into canonical RGBA8 pixels.
pub fn decode_palette_alpha_rgba8(data: &[u8], width: u32, height: u32) -> Result<RgbaImage> {
    let color_len =
        usize::try_from(u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4)) * 8)
            .map_err(|_| RsbError::DeserializationError("PTX dimensions overflow".into()))?;
    if data.len() < color_len {
        return Err(RsbError::DeserializationError(
            "insufficient ETC1 palette payload".into(),
        ));
    }
    let mut image = decode_plane(&data[..color_len], width, height).map_err(to_rsb_error)?;
    let alpha = decode_palette(
        &data[color_len..],
        usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| RsbError::DeserializationError("PTX dimensions overflow".into()))?,
    )
    .map_err(to_rsb_error)?;
    apply_alpha_values(&mut image, &alpha);
    Ok(image)
}

pub fn decode_palette_alpha_values(
    data: &[u8],
    num_pixels: usize,
) -> std::result::Result<Vec<u8>, String> {
    decode_palette(data, num_pixels).map_err(|error| error.to_string())
}

fn apply_alpha_values(image: &mut RgbaImage, alpha: &[u8]) {
    for (pixel, &value) in image.pixels_mut().zip(alpha) {
        pixel[3] = value;
    }
}

fn to_rsb_error(error: crate::ptx::PtxError) -> RsbError {
    RsbError::DeserializationError(error.to_string())
}
