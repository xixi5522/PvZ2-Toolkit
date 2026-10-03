use image::RgbaImage;

use super::error::{PtxError, Result};

/// Borrowed, row-strided RGBA8 pixels.
#[derive(Debug, Clone, Copy)]
pub struct Rgba8Surface<'a> {
    data: &'a [u8],
    width: u32,
    height: u32,
    stride: u32,
}

impl<'a> Rgba8Surface<'a> {
    pub fn new(data: &'a [u8], width: u32, height: u32, stride: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(PtxError::EmptyTexture);
        }
        let minimum_stride = width.checked_mul(4).ok_or(PtxError::DimensionsOverflow)?;
        if stride < minimum_stride {
            return Err(PtxError::InvalidRowPitch {
                pitch: stride,
                minimum: minimum_stride,
            });
        }
        let expected = usize::try_from(u64::from(stride) * u64::from(height))
            .map_err(|_| PtxError::DimensionsOverflow)?;
        if data.len() < expected {
            return Err(PtxError::InvalidRgbaSize {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            width,
            height,
            stride,
        })
    }

    pub fn from_image(image: &'a RgbaImage) -> Self {
        Self {
            data: image.as_raw(),
            width: image.width(),
            height: image.height(),
            stride: image.width() * 4,
        }
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn stride(self) -> u32 {
        self.stride
    }

    pub const fn data(self) -> &'a [u8] {
        self.data
    }

    pub fn pixel(self, x: u32, y: u32) -> [u8; 4] {
        let offset = y as usize * self.stride as usize + x as usize * 4;
        self.data[offset..offset + 4]
            .try_into()
            .expect("surface bounds were validated")
    }

    pub fn to_image(self) -> RgbaImage {
        let mut data = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for row in self
            .data
            .chunks_exact(self.stride as usize)
            .take(self.height as usize)
        {
            data.extend_from_slice(&row[..self.width as usize * 4]);
        }
        RgbaImage::from_raw(self.width, self.height, data)
            .expect("surface dimensions match the copied RGBA data")
    }
}
