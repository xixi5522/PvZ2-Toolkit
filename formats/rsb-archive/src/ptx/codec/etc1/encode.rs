use super::{Etc1Plane, encode_palette, encode_plane};
use crate::error::{Result, RsbError};
use crate::ptx::Rgba8Surface;

pub fn encode_alpha(data: &[u8], _width: u32, _height: u32) -> Vec<u8> {
    data.to_vec()
}

/// Encodes canonical RGBA8 pixels using the PvZ ETC1 palette-alpha layout.
pub fn encode_palette_alpha_rgba8(surface: Rgba8Surface<'_>) -> Result<Vec<u8>> {
    let mut output = encode_plane(surface, Etc1Plane::Color).map_err(to_rsb_error)?;
    output.extend(encode_palette(surface).map_err(to_rsb_error)?);
    Ok(output)
}

fn to_rsb_error(error: crate::ptx::PtxError) -> RsbError {
    RsbError::DeserializationError(error.to_string())
}
