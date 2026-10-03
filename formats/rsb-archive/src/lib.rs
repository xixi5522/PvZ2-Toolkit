//! Reader, writer, packet, and texture codecs for PopCap/PvZ2 RSB bundles.
//!
//! The crate works with standard Rust streams and does not depend on the
//! Toolkit application. Start with [`Rsb`] to inspect an existing bundle,
//! [`RsbWriter`] to write metadata, [`pack_rsg`] and [`unpack_rsg`] for packet
//! payloads, or [`PtxDecoder`] and [`PtxEncoder`] for embedded textures.

mod edit;
pub mod error;
pub mod io;
pub mod ptx;
pub mod rsg;
pub mod schema;
mod utils;

pub use edit::{RsbArchiveEdit, RsgPacketAddition, RsgPacketEdit, RsgPacketGroup, rebuild_rsb};
pub use error::{Result, RsbError};
pub use io::reader::Rsb;
pub use io::writer::RsbWriter;
#[cfg(feature = "gpu")]
pub use ptx::gpu::{
    DecodeBackend, EncodeBackend, GpuEncodeOptions, GpuEncodedPtx, PreviewOptions, PtxGpuCodec,
    PtxGpuError, PtxGpuFormat, PtxGpuTexture, PtxPreviewRenderer, PtxTextureDescriptor,
    optional_texture_features, supported_optional_features,
};
pub use ptx::{
    ASTC_BLOCK_SIZES, AstcQuality, ChannelOrder, PtxAlphaEncoding, PtxDecoder, PtxDescriptor,
    PtxEncodeOptions, PtxEncoder, PtxError, PtxFormat, PtxFormatCode, PtxPayload, PtxPayloadLayout,
    PtxPlaneLayout, PtxResult, PtxRsbMetadata, RgbI32, Rgba8, Rgba8Surface, RgbaI32,
    astc_data_size, decode_astc_rgba8, decode_pvrtc_4bpp_a8_rgba8, decode_pvrtc_4bpp_rgba8,
    encode_astc_rgba8, encode_pvrtc_4bpp_rgba8, is_valid_astc_block_size, resolve_rsb_format,
};
pub use rsg::{
    DEFAULT_ZLIB_LEVEL, Part0Info, Part1Extra, Part1Info, RSG_DATA_ALIGNMENT, RSG_FOURCC,
    RSG_MAGIC, RsgHeader, RsgPayload, UnpackedFile, compress_rsg_zlib,
    compress_rsg_zlib_with_level, compress_zlib, compress_zlib_with_level, decompress_zlib,
    decompress_zlib_exact, is_zlib_stream, pack_rsg, unpack_rsg,
};
pub use schema::types::*;
