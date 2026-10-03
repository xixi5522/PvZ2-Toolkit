use crate::PtxFormat;

/// Compatibility alias. CPU and GPU consume the same resolved PTX format.
pub type PtxGpuFormat = PtxFormat;

#[derive(Debug, Clone, Copy)]
pub struct PtxTextureDescriptor<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub format: PtxFormat,
    pub label: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeBackend {
    HardwareCompressed,
    Compute,
    CpuFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeBackend {
    ComputeFast,
    CpuFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuEncodedPtx {
    pub data: Vec<u8>,
    pub backend: EncodeBackend,
}

#[derive(Debug, Clone, Copy)]
pub struct GpuEncodeOptions {
    pub format: PtxFormat,
    pub include_alpha: bool,
    pub label: Option<&'static str>,
}

impl GpuEncodeOptions {
    pub const fn new(format: PtxFormat) -> Self {
        Self {
            format,
            include_alpha: true,
            label: None,
        }
    }
}
