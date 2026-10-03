//! Pure Rust readers, writers, and converters for Audiokinetic Wwise WEM audio.
//!
//! The crate can inspect WEM metadata, convert Wwise Vorbis to and from Ogg
//! Vorbis, wrap PCM/ADPCM/AAC inputs, and decode supported WEM streams to WAV.

pub mod container;
pub mod error;

#[cfg(feature = "aac")]
mod aac;
#[cfg(feature = "wav")]
mod adpcm;
#[cfg(feature = "vorbis")]
mod bit_stream;
#[cfg(feature = "wav")]
mod pcm;
#[cfg(feature = "transcode")]
mod transcode;
#[cfg(feature = "vorbis")]
mod vorbis;

pub use container::{
    RiffEndian, WAVE_FORMAT_EXTENSIBLE, WAVE_FORMAT_PCM, WWISE_FORMAT_AAC, WWISE_FORMAT_IMA_ADPCM,
    WWISE_FORMAT_VORBIS, WemChunk, WemChunks, WemCodec, WemMetadata, WemReader, inspect_wem,
};
pub use error::{WemError, WemResult};
#[cfg(feature = "vorbis")]
pub use vorbis::{
    CodebookLibrary, PacketFormat, SetupFormat, VorbisOptions, VorbisWemDecoder, VorbisWemEncoder,
};

#[cfg(all(feature = "aac", not(target_arch = "wasm32")))]
pub use aac::probe_aac_file;
#[cfg(feature = "aac")]
pub use aac::{AacMetadata, AacWemEncoder, extract_wem_aac, probe_aac_bytes, probe_aac_metadata};
#[cfg(feature = "wav")]
pub use adpcm::AdpcmWemEncoder;
#[cfg(feature = "wav")]
pub use pcm::PcmWemEncoder;
#[cfg(feature = "transcode")]
pub use transcode::{WemDecodeOptions, decode_wem_to_wav};
