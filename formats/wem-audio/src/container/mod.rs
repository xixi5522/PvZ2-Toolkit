mod reader;
mod types;

#[cfg(feature = "vorbis")]
pub(crate) use reader::read_chunk_exact_from;
pub use reader::{WemReader, inspect_wem};
pub use types::{
    RiffEndian, WAVE_FORMAT_EXTENSIBLE, WAVE_FORMAT_PCM, WWISE_FORMAT_AAC, WWISE_FORMAT_IMA_ADPCM,
    WWISE_FORMAT_VORBIS, WemChunk, WemChunks, WemCodec, WemMetadata,
};
