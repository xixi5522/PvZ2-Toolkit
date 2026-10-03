//! On-disk Dzip format structures and constants.

mod chunk;
mod layout;
mod raw;
mod settings;

pub use chunk::{
    CHUNK_BZIP, CHUNK_COMBUF, CHUNK_COPYCOMP, CHUNK_DZ, CHUNK_JPEG, CHUNK_LZMA, CHUNK_MP3,
    CHUNK_RANDOMACCESS, CHUNK_ZERO, CHUNK_ZLIB, Chunk,
};
#[cfg(feature = "decode")]
pub(crate) use layout::resolve_volume_chunk_layout;
pub use layout::{ResolvedChunk, resolve_chunk_layout};
pub use raw::{ArchivePath, ArchiveString, RawArchive, RawFileRecord};
pub use settings::{ArchiveSettings, ChunkSettings, RangeSettings};
