use image::RgbaImage;

use super::decode_plane;
use crate::ptx::error::{PtxError, Result};

pub(crate) fn apply_a8(image: &mut RgbaImage, alpha: &[u8]) -> Result<()> {
    let expected = image.width() as usize * image.height() as usize;
    if alpha.len() != expected {
        return Err(PtxError::InvalidPayloadSize {
            format: crate::ptx::PtxFormat::Etc1A8,
            expected,
            actual: alpha.len(),
        });
    }
    for (pixel, &value) in image.pixels_mut().zip(alpha) {
        pixel[3] = value;
    }
    Ok(())
}

pub(crate) fn apply_etc1(image: &mut RgbaImage, alpha: &[u8]) -> Result<()> {
    let alpha_image = decode_plane(alpha, image.width(), image.height())?;
    for (pixel, alpha_pixel) in image.pixels_mut().zip(alpha_image.pixels()) {
        pixel[3] = alpha_pixel[1];
    }
    Ok(())
}
