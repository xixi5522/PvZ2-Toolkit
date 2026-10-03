use std::ops::Range;

use thiserror::Error;

use super::PtxFormat;

#[derive(Debug, Error)]
pub enum PtxError {
    #[error("unknown PTX format code {0}")]
    UnknownFormatCode(i32),
    #[error("unsupported PTX format {0:?}")]
    UnsupportedFormat(PtxFormat),
    #[error("texture dimensions must be non-zero")]
    EmptyTexture,
    #[error("texture dimensions overflow addressable memory")]
    DimensionsOverflow,
    #[error("invalid row pitch {pitch}: at least {minimum} bytes are required")]
    InvalidRowPitch { pitch: u32, minimum: u32 },
    #[error("invalid {format:?} payload size: expected {expected} bytes, found {actual}")]
    InvalidPayloadSize {
        format: PtxFormat,
        expected: usize,
        actual: usize,
    },
    #[error("PTX plane range {range:?} lies outside a {payload_len}-byte payload")]
    PlaneOutOfBounds {
        range: Range<usize>,
        payload_len: usize,
    },
    #[error("invalid ETC1 palette alpha payload: {0}")]
    InvalidPalette(String),
    #[error("invalid ASTC block footprint {width}x{height}")]
    InvalidAstcFootprint { width: u32, height: u32 },
    #[error("PVRTC1 4bpp requires power-of-two dimensions of at least 4x4, found {width}x{height}")]
    InvalidPvrtcDimensions { width: u32, height: u32 },
    #[error("invalid RGBA surface length: expected at least {expected} bytes, found {actual}")]
    InvalidRgbaSize { expected: usize, actual: usize },
    #[error("PTX codec failed: {0}")]
    Codec(String),
}

pub type Result<T> = std::result::Result<T, PtxError>;

impl From<crate::RsbError> for PtxError {
    fn from(error: crate::RsbError) -> Self {
        Self::Codec(error.to_string())
    }
}
