use image::RgbaImage;

use crate::error::{Result as RsbResult, RsbError};
use crate::ptx::codec::astc::decode_astc_rgba8;
use crate::ptx::codec::etc1::{apply_a8, apply_etc1, decode_palette, decode_plane};
use crate::ptx::codec::packed::{decode_linear, decode_tiled};
use crate::ptx::codec::pvrtc::{decode_pvrtc_4bpp_a8_rgba8, decode_pvrtc_4bpp_rgba8};
use crate::ptx::error::{PtxError, Result};
use crate::ptx::{
    ChannelOrder, PtxDescriptor, PtxFormat, PtxFormatCode, PtxPayload, PtxRsbMetadata,
};

pub struct PtxDecoder;

impl PtxDecoder {
    pub fn decode_payload(payload: &PtxPayload<'_>) -> Result<RgbaImage> {
        let descriptor = payload.descriptor();
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
            | PtxFormat::Argb1555 => decode_linear(payload.data(), descriptor),
            PtxFormat::Rgba4444Block | PtxFormat::Rgb565Block | PtxFormat::Rgba5551Block => {
                decode_tiled(payload.data(), descriptor)
            }
            PtxFormat::Etc1 => decode_plane(payload.color(), descriptor.width, descriptor.height),
            PtxFormat::Etc1A8 => {
                let mut image = decode_plane(payload.color(), descriptor.width, descriptor.height)?;
                apply_a8(
                    &mut image,
                    payload
                        .alpha()
                        .expect("ETC1+A8 layout always contains alpha"),
                )?;
                Ok(image)
            }
            PtxFormat::Etc1CompressedAlpha => {
                let mut image = decode_plane(payload.color(), descriptor.width, descriptor.height)?;
                apply_etc1(
                    &mut image,
                    payload
                        .alpha()
                        .expect("ETC1 compressed-alpha layout always contains alpha"),
                )?;
                Ok(image)
            }
            PtxFormat::Etc1Palette => {
                let mut image = decode_plane(payload.color(), descriptor.width, descriptor.height)?;
                let alpha = decode_palette(
                    payload
                        .alpha()
                        .expect("ETC1 palette layout always contains alpha"),
                    descriptor.width as usize * descriptor.height as usize,
                )?;
                for (pixel, value) in image.pixels_mut().zip(alpha) {
                    pixel[3] = value;
                }
                Ok(image)
            }
            PtxFormat::Pvrtc4BppRgba => {
                decode_pvrtc_4bpp_rgba8(payload.color(), descriptor.width, descriptor.height)
                    .map_err(PtxError::from)
            }
            PtxFormat::Pvrtc4BppRgbaA8 => decode_pvrtc_4bpp_a8_rgba8(
                payload.color(),
                payload
                    .alpha()
                    .expect("PVRTC+A8 layout always contains alpha"),
                descriptor.width,
                descriptor.height,
            )
            .map_err(PtxError::from),
            PtxFormat::Astc {
                block_width,
                block_height,
            } => decode_astc_rgba8(
                payload.color(),
                descriptor.width,
                descriptor.height,
                block_width,
                block_height,
            )
            .map_err(PtxError::from),
            PtxFormat::Unknown(code) => Err(PtxError::UnknownFormatCode(code)),
        }
    }

    pub fn decode_with_descriptor(data: &[u8], descriptor: PtxDescriptor) -> Result<RgbaImage> {
        let payload = PtxPayload::parse(data, descriptor)?;
        Self::decode_payload(&payload)
    }

    /// Decodes manifest fields directly into canonical RGBA8 pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn decode_rgba8(
        data: &[u8],
        width: u32,
        height: u32,
        format_code: i32,
        alpha_size: Option<i32>,
        alpha_format: Option<i32>,
        pitch: Option<i32>,
        apple_channel_order: bool,
    ) -> RsbResult<RgbaImage> {
        let metadata = PtxRsbMetadata {
            format_code: PtxFormatCode::new(format_code),
            alpha_size: alpha_size.and_then(|value| u32::try_from(value).ok()),
            alpha_format,
            row_pitch: pitch.and_then(|value| u32::try_from(value).ok()),
            channel_order: if apple_channel_order {
                ChannelOrder::Bgra
            } else {
                ChannelOrder::Rgba
            },
        };
        let descriptor =
            PtxDescriptor::from_rsb_payload(width, height, metadata, data).map_err(to_rsb_error)?;
        Self::decode_with_descriptor(data, descriptor).map_err(to_rsb_error)
    }
}

fn to_rsb_error(error: PtxError) -> RsbError {
    RsbError::DeserializationError(error.to_string())
}
