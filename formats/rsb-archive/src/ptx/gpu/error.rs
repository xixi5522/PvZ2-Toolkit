use thiserror::Error;

#[derive(Debug, Error)]
pub enum PtxGpuError {
    #[error("unsupported ASTC block footprint {width}x{height}")]
    InvalidAstcFootprint { width: u32, height: u32 },
    #[error("GPU backend does not support this PTX format")]
    UnsupportedFormat,
    #[error("texture dimensions overflow addressable memory")]
    DimensionsOverflow,
    #[error("GPU buffer mapping failed: {0}")]
    BufferMap(#[from] wgpu::BufferAsyncError),
    #[error("GPU buffer mapping callback was dropped")]
    CallbackDropped,
    #[error("GPU buffer access failed: {0}")]
    BufferAccess(String),
    #[error("CPU codec fallback failed: {0}")]
    CpuCodec(#[from] crate::RsbError),
    #[error(transparent)]
    Ptx(#[from] crate::ptx::PtxError),
}

pub type Result<T> = std::result::Result<T, PtxGpuError>;
