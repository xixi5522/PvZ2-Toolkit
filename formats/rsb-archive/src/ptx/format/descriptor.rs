use super::{PtxFormat, PtxFormatCode};
use crate::ptx::error::{PtxError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChannelOrder {
    #[default]
    Rgba,
    Bgra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PtxRsbMetadata {
    pub format_code: PtxFormatCode,
    /// PvZ2 China `additional_byte_count` compatibility field.
    pub alpha_size: Option<u32>,
    /// PvZ2 China texture `scale` compatibility field.
    pub alpha_format: Option<i32>,
    pub row_pitch: Option<u32>,
    pub channel_order: ChannelOrder,
}

impl PtxRsbMetadata {
    pub fn new(format_code: impl Into<PtxFormatCode>) -> Self {
        Self {
            format_code: format_code.into(),
            alpha_size: None,
            alpha_format: None,
            row_pitch: None,
            channel_order: ChannelOrder::Rgba,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PtxDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: PtxFormat,
    pub row_pitch: Option<u32>,
    pub channel_order: ChannelOrder,
}

impl PtxDescriptor {
    pub fn new(width: u32, height: u32, format: PtxFormat) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(PtxError::EmptyTexture);
        }
        Ok(Self {
            width,
            height,
            format,
            row_pitch: None,
            channel_order: ChannelOrder::Rgba,
        })
    }

    pub fn from_rsb(width: u32, height: u32, metadata: PtxRsbMetadata) -> Result<Self> {
        let mut descriptor = Self::new(
            width,
            height,
            resolve_rsb_format(
                metadata.format_code,
                metadata.alpha_size,
                metadata.alpha_format,
            )?,
        )?;
        descriptor.row_pitch = metadata.row_pitch.filter(|pitch| *pitch > 0);
        descriptor.channel_order = metadata.channel_order;
        Ok(descriptor)
    }

    /// Resolves an RSB texture format using both metadata and the encoded
    /// payload.
    ///
    /// PopCap format codes 30 and 147 are platform-dependent. In particular,
    /// Android uses 147 for ETC1+A8 while PvZ2 China uses the same code for
    /// ETC1 with palette alpha. The extended RSB field called `alpha_size` by
    /// the compatibility API is really the palette's additional byte count;
    /// older archives do not contain it. Validating candidate plane layouts
    /// against the payload keeps both archive families unambiguous.
    pub fn from_rsb_payload(
        width: u32,
        height: u32,
        metadata: PtxRsbMetadata,
        data: &[u8],
    ) -> Result<Self> {
        let metadata_format = resolve_rsb_format(
            metadata.format_code,
            metadata.alpha_size,
            metadata.alpha_format,
        )?;
        let candidates: &[PtxFormat] = match metadata.format_code.get() {
            30 if metadata_format == PtxFormat::Etc1Palette => {
                &[PtxFormat::Etc1Palette, PtxFormat::Pvrtc4BppRgba]
            }
            30 => &[PtxFormat::Pvrtc4BppRgba, PtxFormat::Etc1Palette],
            147 if metadata_format == PtxFormat::Etc1Palette => &[
                PtxFormat::Etc1Palette,
                PtxFormat::Etc1A8,
                PtxFormat::Etc1CompressedAlpha,
                PtxFormat::Etc1,
            ],
            147 => &[
                PtxFormat::Etc1,
                PtxFormat::Etc1A8,
                PtxFormat::Etc1CompressedAlpha,
                PtxFormat::Etc1Palette,
            ],
            _ => &[metadata_format],
        };
        let format = candidates
            .iter()
            .copied()
            .find(|format| {
                Self::new(width, height, *format)
                    .ok()
                    .and_then(|descriptor| {
                        crate::ptx::PtxPayloadLayout::for_decode(descriptor, data).ok()
                    })
                    .is_some()
            })
            .unwrap_or(metadata_format);
        let mut descriptor = Self::new(width, height, format)?;
        descriptor.row_pitch = metadata.row_pitch.filter(|pitch| *pitch > 0);
        descriptor.channel_order = metadata.channel_order;
        Ok(descriptor)
    }

    pub const fn with_row_pitch(mut self, row_pitch: Option<u32>) -> Self {
        self.row_pitch = row_pitch;
        self
    }

    pub const fn with_channel_order(mut self, channel_order: ChannelOrder) -> Self {
        self.channel_order = channel_order;
        self
    }
}

pub fn resolve_rsb_format(
    code: PtxFormatCode,
    alpha_size: Option<u32>,
    _alpha_format: Option<i32>,
) -> Result<PtxFormat> {
    let format = match code.get() {
        30 if alpha_size.unwrap_or(0) > 0 => PtxFormat::Etc1Palette,
        30 => PtxFormat::Pvrtc4BppRgba,
        147 if alpha_size.unwrap_or(0) > 0 => PtxFormat::Etc1Palette,
        147 => PtxFormat::Etc1,
        value => PtxFormat::from(value),
    };
    match format {
        PtxFormat::Unknown(value) => Err(PtxError::UnknownFormatCode(value)),
        format => Ok(format),
    }
}
