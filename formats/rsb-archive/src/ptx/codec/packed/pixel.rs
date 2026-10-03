use crate::ptx::error::{PtxError, Result};
use crate::ptx::{ChannelOrder, PtxFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackedPixelFormat {
    Rgba8888,
    Rgba4444,
    Rgb565,
    Rgba5551,
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
}

impl PackedPixelFormat {
    pub(crate) fn from_ptx(format: PtxFormat) -> Result<Self> {
        match format {
            PtxFormat::Rgba8888 => Ok(Self::Rgba8888),
            PtxFormat::Rgba4444 | PtxFormat::Rgba4444Block => Ok(Self::Rgba4444),
            PtxFormat::Rgb565 | PtxFormat::Rgb565Block => Ok(Self::Rgb565),
            PtxFormat::Rgba5551 | PtxFormat::Rgba5551Block => Ok(Self::Rgba5551),
            PtxFormat::A8 => Ok(Self::A8),
            PtxFormat::L8 => Ok(Self::L8),
            PtxFormat::La88 => Ok(Self::La88),
            PtxFormat::Al88 => Ok(Self::Al88),
            PtxFormat::La44 => Ok(Self::La44),
            PtxFormat::Al44 => Ok(Self::Al44),
            PtxFormat::Rgb332 => Ok(Self::Rgb332),
            PtxFormat::Rgb888 => Ok(Self::Rgb888),
            PtxFormat::Argb8888 => Ok(Self::Argb8888),
            PtxFormat::Argb4444 => Ok(Self::Argb4444),
            PtxFormat::Argb1555 => Ok(Self::Argb1555),
            format => Err(PtxError::UnsupportedFormat(format)),
        }
    }

    pub(crate) const fn byte_len(self) -> usize {
        match self {
            Self::Rgba8888 | Self::Argb8888 => 4,
            Self::Rgba4444
            | Self::Rgb565
            | Self::Rgba5551
            | Self::La88
            | Self::Al88
            | Self::Argb4444
            | Self::Argb1555 => 2,
            Self::Rgb888 => 3,
            Self::A8 | Self::L8 | Self::La44 | Self::Al44 | Self::Rgb332 => 1,
        }
    }

    pub(crate) fn encode(self, pixel: [u8; 4], order: ChannelOrder) -> ([u8; 4], usize) {
        let [r, g, b, a] = pixel;
        let mut bytes = [0; 4];
        let len = self.byte_len();
        match self {
            Self::Rgba8888 => {
                bytes = match order {
                    ChannelOrder::Rgba => [r, g, b, a],
                    ChannelOrder::Bgra => [b, g, r, a],
                };
            }
            Self::Rgba4444 => {
                bytes[..2].copy_from_slice(
                    &((((r >> 4) as u16) << 12)
                        | (((g >> 4) as u16) << 8)
                        | (((b >> 4) as u16) << 4)
                        | (a >> 4) as u16)
                        .to_le_bytes(),
                );
            }
            Self::Rgb565 => {
                bytes[..2].copy_from_slice(
                    &((((r >> 3) as u16) << 11) | (((g >> 2) as u16) << 5) | (b >> 3) as u16)
                        .to_le_bytes(),
                );
            }
            Self::Rgba5551 => {
                bytes[..2].copy_from_slice(
                    &((((r >> 3) as u16) << 11)
                        | (((g >> 3) as u16) << 6)
                        | (((b >> 3) as u16) << 1)
                        | u16::from(a > 127))
                    .to_le_bytes(),
                );
            }
            Self::A8 => bytes[0] = a,
            Self::L8 => bytes[0] = luminance(r, g, b),
            Self::La88 => {
                bytes[0] = luminance(r, g, b);
                bytes[1] = a;
            }
            Self::Al88 => {
                bytes[0] = a;
                bytes[1] = luminance(r, g, b);
            }
            Self::La44 => bytes[0] = ((a >> 4) << 4) | (luminance(r, g, b) >> 4),
            Self::Al44 => bytes[0] = ((luminance(r, g, b) >> 4) << 4) | (a >> 4),
            Self::Rgb332 => bytes[0] = (r & 0xe0) | ((g >> 3) & 0x1c) | (b >> 6),
            Self::Rgb888 => bytes[..3].copy_from_slice(&[r, g, b]),
            // ARGB8888 is an AARRGGBB integer written little-endian.
            Self::Argb8888 => bytes = [b, g, r, a],
            Self::Argb4444 => {
                bytes[..2].copy_from_slice(
                    &((((a >> 4) as u16) << 12)
                        | (((r >> 4) as u16) << 8)
                        | (((g >> 4) as u16) << 4)
                        | (b >> 4) as u16)
                        .to_le_bytes(),
                );
            }
            Self::Argb1555 => {
                bytes[..2].copy_from_slice(
                    &((u16::from(a > 127) << 15)
                        | (((r >> 3) as u16) << 10)
                        | (((g >> 3) as u16) << 5)
                        | (b >> 3) as u16)
                        .to_le_bytes(),
                );
            }
        }
        (bytes, len)
    }

    pub(crate) fn decode(self, bytes: &[u8], order: ChannelOrder) -> [u8; 4] {
        match self {
            Self::Rgba8888 => match order {
                ChannelOrder::Rgba => [bytes[0], bytes[1], bytes[2], bytes[3]],
                ChannelOrder::Bgra => [bytes[2], bytes[1], bytes[0], bytes[3]],
            },
            Self::Rgba4444 => {
                let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                [
                    expand4((value >> 12) as u8),
                    expand4((value >> 8) as u8),
                    expand4((value >> 4) as u8),
                    expand4(value as u8),
                ]
            }
            Self::Rgb565 => {
                let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                [
                    expand5((value >> 11) as u8),
                    expand6((value >> 5) as u8),
                    expand5(value as u8),
                    255,
                ]
            }
            Self::Rgba5551 => {
                let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                [
                    expand5((value >> 11) as u8),
                    expand5((value >> 6) as u8),
                    expand5((value >> 1) as u8),
                    if value & 1 != 0 { 255 } else { 0 },
                ]
            }
            Self::A8 => [255, 255, 255, bytes[0]],
            Self::L8 => [bytes[0], bytes[0], bytes[0], 255],
            Self::La88 => [bytes[0], bytes[0], bytes[0], bytes[1]],
            Self::Al88 => [bytes[1], bytes[1], bytes[1], bytes[0]],
            Self::La44 => {
                let l = expand4(bytes[0]);
                let a = expand4(bytes[0] >> 4);
                [l, l, l, a]
            }
            Self::Al44 => {
                let a = expand4(bytes[0]);
                let l = expand4(bytes[0] >> 4);
                [l, l, l, a]
            }
            Self::Rgb332 => [
                expand3(bytes[0] >> 5),
                expand3(bytes[0] >> 2),
                expand2(bytes[0]),
                255,
            ],
            Self::Rgb888 => [bytes[0], bytes[1], bytes[2], 255],
            Self::Argb8888 => [bytes[2], bytes[1], bytes[0], bytes[3]],
            Self::Argb4444 => {
                let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                [
                    expand4((value >> 8) as u8),
                    expand4((value >> 4) as u8),
                    expand4(value as u8),
                    expand4((value >> 12) as u8),
                ]
            }
            Self::Argb1555 => {
                let value = u16::from_le_bytes([bytes[0], bytes[1]]);
                [
                    expand5((value >> 10) as u8),
                    expand5((value >> 5) as u8),
                    expand5(value as u8),
                    if value & 0x8000 != 0 { 255 } else { 0 },
                ]
            }
        }
    }
}

const fn luminance(r: u8, g: u8, b: u8) -> u8 {
    ((r as u16 + g as u16 + b as u16) / 3) as u8
}

const fn expand2(value: u8) -> u8 {
    (value & 3) * 85
}

const fn expand3(value: u8) -> u8 {
    let value = value & 7;
    (value << 5) | (value << 2) | (value >> 1)
}

const fn expand4(value: u8) -> u8 {
    let value = value & 15;
    (value << 4) | value
}

const fn expand5(value: u8) -> u8 {
    let value = value & 31;
    (value << 3) | (value >> 2)
}

const fn expand6(value: u8) -> u8 {
    let value = value & 63;
    (value << 2) | (value >> 4)
}
