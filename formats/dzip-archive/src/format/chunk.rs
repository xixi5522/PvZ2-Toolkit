//! Chunk records and their bit flags.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    /// Location of the chunk in its volume.
    pub offset: u32,
    /// Length of the encoded physical stream.
    pub compressed_length: u32,
    /// Length of the decoded data.
    ///
    /// Some original archives store the uncompressed size in both length
    /// fields; the high-level reader repairs that known writer quirk.
    pub decompressed_length: u32,
    /// Chunk encoding and modifier flags.
    pub flags: u16,
    /// Volume containing the physical chunk stream.
    pub file: u16,
}

pub const CHUNK_COMBUF: u16 = 0x1;
pub const CHUNK_DZ: u16 = 0x4;
pub const CHUNK_ZLIB: u16 = 0x8;
pub const CHUNK_BZIP: u16 = 0x10;
pub const CHUNK_MP3: u16 = 0x20;
pub const CHUNK_JPEG: u16 = 0x40;
pub const CHUNK_ZERO: u16 = 0x80;
pub const CHUNK_COPYCOMP: u16 = 0x100;
pub const CHUNK_LZMA: u16 = 0x200;
pub const CHUNK_RANDOMACCESS: u16 = 0x400;
