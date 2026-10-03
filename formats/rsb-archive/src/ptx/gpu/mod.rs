//! WGPU acceleration and preview rendering for compressed PopCap PTX textures.
//!
//! The module is enabled by the optional `gpu` Cargo feature. Applications
//! that only need format parsing keep the default GPU-independent dependency
//! graph, while UI applications can opt into hardware ASTC/ETC sampling,
//! portable compute decoding, fast compute encoding, and preview rendering.

mod context;
mod decode;
mod dispatch;
mod encode;
mod error;
mod params;
mod pipelines;
mod preview;
mod readback;
mod texture;
mod types;
mod util;

pub use context::PtxGpuCodec;
pub use error::{PtxGpuError, Result};
pub use preview::{PreviewOptions, PtxPreviewRenderer};
pub use texture::PtxGpuTexture;
pub use types::{
    DecodeBackend, EncodeBackend, GpuEncodeOptions, GpuEncodedPtx, PtxGpuFormat,
    PtxTextureDescriptor,
};

/// Optional compressed-texture features worth enabling when requesting a
/// device for the PTX GPU backend.
///
/// Callers must intersect this value with `adapter.features()` before placing
/// it in `wgpu::DeviceDescriptor::required_features`.
pub fn optional_texture_features() -> wgpu::Features {
    wgpu::Features::TEXTURE_COMPRESSION_ASTC | wgpu::Features::TEXTURE_COMPRESSION_ETC2
}

/// Returns the compressed-texture features supported by an adapter and used
/// by this crate. Compute fallbacks remain available when this returns empty.
pub fn supported_optional_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & optional_texture_features()
}
