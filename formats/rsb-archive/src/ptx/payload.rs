use std::ops::Range;

use super::error::{PtxError, Result};
use super::{PtxDescriptor, PtxFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PtxAlphaEncoding {
    A8,
    Etc1,
    Palette,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtxPlaneLayout {
    pub range: Range<usize>,
    pub encoding: PtxAlphaEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtxPayloadLayout {
    pub color: Range<usize>,
    pub alpha: Option<PtxPlaneLayout>,
    pub total_len: usize,
}

impl PtxPayloadLayout {
    pub fn for_decode(descriptor: PtxDescriptor, data: &[u8]) -> Result<Self> {
        let layout = Self::build(descriptor, Some(data))?;
        if data.len() != layout.total_len {
            return Err(PtxError::InvalidPayloadSize {
                format: descriptor.format,
                expected: layout.total_len,
                actual: data.len(),
            });
        }
        Ok(layout)
    }

    pub fn for_encode(descriptor: PtxDescriptor) -> Result<Self> {
        Self::build(descriptor, None)
    }

    fn build(descriptor: PtxDescriptor, data: Option<&[u8]>) -> Result<Self> {
        let width = descriptor.width;
        let height = descriptor.height;
        if width == 0 || height == 0 {
            return Err(PtxError::EmptyTexture);
        }
        let pixels = pixel_count(width, height)?;
        let single_plane = |len| Self {
            color: 0..len,
            alpha: None,
            total_len: len,
        };

        match descriptor.format {
            PtxFormat::Rgba8888
            | PtxFormat::Rgba4444
            | PtxFormat::Rgb565
            | PtxFormat::Rgba5551
            | PtxFormat::A8
            | PtxFormat::L8
            | PtxFormat::La88
            | PtxFormat::Al88
            | PtxFormat::La44
            | PtxFormat::Al44
            | PtxFormat::Rgb332
            | PtxFormat::Rgb888
            | PtxFormat::Argb8888
            | PtxFormat::Argb4444
            | PtxFormat::Argb1555 => {
                let bytes_per_pixel = descriptor
                    .format
                    .bytes_per_pixel()
                    .expect("matched a packed pixel format");
                let minimum_pitch = width
                    .checked_mul(bytes_per_pixel)
                    .ok_or(PtxError::DimensionsOverflow)?;
                let pitch = descriptor.row_pitch.unwrap_or(minimum_pitch);
                if pitch < minimum_pitch {
                    return Err(PtxError::InvalidRowPitch {
                        pitch,
                        minimum: minimum_pitch,
                    });
                }
                Ok(single_plane(checked_usize(
                    u64::from(pitch) * u64::from(height),
                )?))
            }
            PtxFormat::Rgba4444Block | PtxFormat::Rgb565Block | PtxFormat::Rgba5551Block => {
                let blocks = u64::from(width.div_ceil(32)) * u64::from(height.div_ceil(32));
                Ok(single_plane(checked_usize(blocks * 32 * 32 * 2)?))
            }
            PtxFormat::Etc1 => Ok(single_plane(etc1_plane_size(width, height)?)),
            PtxFormat::Etc1A8 => {
                let color_len = etc1_plane_size(width, height)?;
                let total_len = color_len
                    .checked_add(pixels)
                    .ok_or(PtxError::DimensionsOverflow)?;
                Ok(Self {
                    color: 0..color_len,
                    alpha: Some(PtxPlaneLayout {
                        range: color_len..total_len,
                        encoding: PtxAlphaEncoding::A8,
                    }),
                    total_len,
                })
            }
            PtxFormat::Etc1CompressedAlpha => {
                let color_len = etc1_plane_size(width, height)?;
                let total_len = color_len
                    .checked_mul(2)
                    .ok_or(PtxError::DimensionsOverflow)?;
                Ok(Self {
                    color: 0..color_len,
                    alpha: Some(PtxPlaneLayout {
                        range: color_len..total_len,
                        encoding: PtxAlphaEncoding::Etc1,
                    }),
                    total_len,
                })
            }
            PtxFormat::Etc1Palette => {
                let color_len = etc1_plane_size(width, height)?;
                let alpha_len = match data {
                    Some(data) => palette_alpha_len(data, color_len, pixels)?,
                    None => 17usize
                        .checked_add(pixels.div_ceil(2))
                        .ok_or(PtxError::DimensionsOverflow)?,
                };
                let total_len = color_len
                    .checked_add(alpha_len)
                    .ok_or(PtxError::DimensionsOverflow)?;
                Ok(Self {
                    color: 0..color_len,
                    alpha: Some(PtxPlaneLayout {
                        range: color_len..total_len,
                        encoding: PtxAlphaEncoding::Palette,
                    }),
                    total_len,
                })
            }
            PtxFormat::Pvrtc4BppRgba => Ok(single_plane(pvrtc_plane_size(width, height)?)),
            PtxFormat::Pvrtc4BppRgbaA8 => {
                let color_len = pvrtc_plane_size(width, height)?;
                let total_len = color_len
                    .checked_add(pixels)
                    .ok_or(PtxError::DimensionsOverflow)?;
                Ok(Self {
                    color: 0..color_len,
                    alpha: Some(PtxPlaneLayout {
                        range: color_len..total_len,
                        encoding: PtxAlphaEncoding::A8,
                    }),
                    total_len,
                })
            }
            PtxFormat::Astc {
                block_width,
                block_height,
            } => Ok(single_plane(
                crate::ptx::codec::astc::astc_data_size(width, height, block_width, block_height)
                    .map_err(PtxError::from)?,
            )),
            PtxFormat::Unknown(code) => Err(PtxError::UnknownFormatCode(code)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PtxPayload<'a> {
    descriptor: PtxDescriptor,
    data: &'a [u8],
    layout: PtxPayloadLayout,
}

impl<'a> PtxPayload<'a> {
    pub fn parse(data: &'a [u8], descriptor: PtxDescriptor) -> Result<Self> {
        let layout = PtxPayloadLayout::for_decode(descriptor, data)?;
        Ok(Self {
            descriptor,
            data,
            layout,
        })
    }

    pub const fn descriptor(&self) -> PtxDescriptor {
        self.descriptor
    }

    pub fn layout(&self) -> &PtxPayloadLayout {
        &self.layout
    }

    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn color(&self) -> &'a [u8] {
        &self.data[self.layout.color.clone()]
    }

    pub fn alpha(&self) -> Option<&'a [u8]> {
        self.layout
            .alpha
            .as_ref()
            .map(|plane| &self.data[plane.range.clone()])
    }
}

pub(crate) fn pixel_count(width: u32, height: u32) -> Result<usize> {
    checked_usize(u64::from(width) * u64::from(height))
}

pub(crate) fn etc1_plane_size(width: u32, height: u32) -> Result<usize> {
    checked_usize(u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4)) * 8)
}

pub(crate) fn pvrtc_plane_size(width: u32, height: u32) -> Result<usize> {
    if width < 4 || height < 4 || !width.is_power_of_two() || !height.is_power_of_two() {
        return Err(PtxError::InvalidPvrtcDimensions { width, height });
    }
    pixel_count(width, height)?
        .checked_div(2)
        .ok_or(PtxError::DimensionsOverflow)
}

fn palette_alpha_len(data: &[u8], offset: usize, pixels: usize) -> Result<usize> {
    let count = *data
        .get(offset)
        .ok_or_else(|| PtxError::InvalidPalette("missing palette count".into()))?
        as usize;
    let (palette_len, depth) = if count == 0 {
        (0, 1)
    } else {
        let depth = (usize::BITS - count.saturating_sub(1).leading_zeros()) as usize;
        (count, depth.max(1))
    };
    let stream_len = pixels
        .checked_mul(depth)
        .ok_or(PtxError::DimensionsOverflow)?
        .div_ceil(8);
    1usize
        .checked_add(palette_len)
        .and_then(|len| len.checked_add(stream_len))
        .ok_or(PtxError::DimensionsOverflow)
}

fn checked_usize(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| PtxError::DimensionsOverflow)
}
