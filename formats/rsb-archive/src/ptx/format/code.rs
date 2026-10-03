#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct PtxFormatCode(pub i32);

impl PtxFormatCode {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

impl From<i32> for PtxFormatCode {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PtxFormat {
    Rgba8888,
    Rgba4444,
    Rgb565,
    Rgba5551,
    Rgba4444Block,
    Rgb565Block,
    Rgba5551Block,
    Pvrtc4BppRgba,
    Etc1,
    Pvrtc4BppRgbaA8,
    /// ETC1 color followed by an uncompressed A8 plane.
    Etc1A8,
    /// ETC1 color followed by a second ETC1-compressed grayscale plane.
    Etc1CompressedAlpha,
    Etc1Palette,
    Astc {
        block_width: u32,
        block_height: u32,
    },
    A8,
    L8,
    La88,
    Al88,
    La44,
    Al44,
    Rgb332,
    Rgb888,
    Argb8888,
    Argb4444,
    Argb1555,
    Unknown(i32),
}

impl PtxFormat {
    pub const fn bytes_per_pixel(self) -> Option<u32> {
        match self {
            Self::Rgba8888 | Self::Argb8888 => Some(4),
            Self::Rgba4444
            | Self::Rgb565
            | Self::Rgba5551
            | Self::Argb4444
            | Self::Argb1555
            | Self::La88
            | Self::Al88 => Some(2),
            Self::Rgb888 => Some(3),
            Self::A8 | Self::L8 | Self::La44 | Self::Al44 | Self::Rgb332 => Some(1),
            _ => None,
        }
    }

    /// Compatibility helper retained for callers of the original API.
    pub const fn bpp(&self) -> u32 {
        match self.bytes_per_pixel() {
            Some(value) => value,
            None => 0,
        }
    }

    pub const fn is_tiled(self) -> bool {
        matches!(
            self,
            Self::Rgba4444Block | Self::Rgb565Block | Self::Rgba5551Block
        )
    }

    pub const fn is_gpu_supported(self) -> bool {
        matches!(
            self,
            Self::Astc { .. }
                | Self::Pvrtc4BppRgba
                | Self::Pvrtc4BppRgbaA8
                | Self::Etc1
                | Self::Etc1A8
                | Self::Etc1CompressedAlpha
                | Self::Etc1Palette
        )
    }
}

impl From<i32> for PtxFormat {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Rgba8888,
            1 => Self::Rgba4444,
            2 => Self::Rgb565,
            3 => Self::Rgba5551,
            21 => Self::Rgba4444Block,
            22 => Self::Rgb565Block,
            23 => Self::Rgba5551Block,
            30 => Self::Pvrtc4BppRgba,
            147 => Self::Etc1,
            148 => Self::Pvrtc4BppRgbaA8,
            160 => Self::Astc {
                block_width: 4,
                block_height: 4,
            },
            161 => Self::Astc {
                block_width: 5,
                block_height: 5,
            },
            162 => Self::Astc {
                block_width: 6,
                block_height: 6,
            },
            163 => Self::Astc {
                block_width: 8,
                block_height: 8,
            },
            200 => Self::A8,
            201 => Self::L8,
            202 => Self::La88,
            203 => Self::Al88,
            204 => Self::La44,
            205 => Self::Al44,
            206 => Self::Rgb332,
            207 => Self::Rgb888,
            208 => Self::Argb8888,
            209 => Self::Argb4444,
            210 => Self::Argb1555,
            code => Self::Unknown(code),
        }
    }
}
