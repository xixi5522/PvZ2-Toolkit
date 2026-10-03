//! Pure compatibility rules shared by the parser, reader, and writer.
//!
//! Keeping these decisions in one place prevents a modern refactor from
//! accidentally making the original reader and packer more symmetrical than
//! dzip.exe actually was.

use crate::codec::Compression;
use crate::format::{
    CHUNK_BZIP, CHUNK_COMBUF, CHUNK_COPYCOMP, CHUNK_DZ, CHUNK_JPEG, CHUNK_LZMA, CHUNK_MP3,
    CHUNK_RANDOMACCESS, CHUNK_ZERO, CHUNK_ZLIB, Chunk,
};

pub(crate) const STORAGE_FLAGS: u16 =
    CHUNK_ZERO | CHUNK_BZIP | CHUNK_COPYCOMP | CHUNK_ZLIB | CHUNK_LZMA | CHUNK_DZ;

/// Flags understood by the semantic Rust reader. BZip and the media hints are
/// deliberately kept as safe extensions because dzip.exe's packer can emit
/// them even though the 8.6 command-line reader does not register those
/// decoders. Truly unknown bits must not be silently ignored.
pub(crate) const SEMANTIC_READER_FLAGS: u16 =
    STORAGE_FLAGS | CHUNK_COMBUF | CHUNK_MP3 | CHUNK_JPEG | CHUNK_RANDOMACCESS;

pub(crate) const fn semantic_reader_supports_flags(flags: u16) -> bool {
    flags & !SEMANTIC_READER_FLAGS == 0
}

/// The archive-wide DZ dictionary is stored in a standalone COMBUF record.
/// A user payload may also carry COMBUF as an orthogonal flag; dzip.exe accepts
/// that for non-DZ coders and it must not be mistaken for dictionary data.
pub(crate) const fn is_dedicated_combuf(flags: u16) -> bool {
    flags & CHUNK_COMBUF != 0 && flags & STORAGE_FLAGS == 0
}

pub(crate) const fn registered_compression(flags: u16) -> Option<Compression> {
    if flags & CHUNK_ZERO != 0 {
        Some(Compression::Zero)
    } else if flags & CHUNK_BZIP != 0 {
        Some(Compression::Bzip)
    } else if flags & CHUNK_COPYCOMP != 0 {
        Some(Compression::Copy)
    } else if flags & CHUNK_ZLIB != 0 {
        Some(Compression::Zlib)
    } else if flags & CHUNK_LZMA != 0 {
        Some(Compression::Lzma)
    } else if flags & CHUNK_DZ != 0 {
        Some(Compression::Dz)
    } else {
        None
    }
}

/// The original reader looks only at the raw DZ bit when deciding whether the
/// ten-byte archive-wide settings field exists.
#[cfg(any(feature = "decode", test))]
pub(crate) const fn flags_require_range_settings(flags: u16) -> bool {
    flags & CHUNK_DZ != 0
}

/// The original writer emits range settings only if DZ wins coder selection.
#[cfg(any(feature = "encode", test))]
pub(crate) const fn compression_writes_range_settings(compression: Compression) -> bool {
    matches!(compression, Compression::Dz)
}

#[cfg(any(feature = "encode", test))]
pub(crate) const fn compression_write_rank(compression: Compression) -> u8 {
    match compression {
        Compression::Zero => 0,
        Compression::Bzip => 1,
        Compression::Copy => 2,
        Compression::Zlib => 3,
        Compression::Lzma => 4,
        Compression::Dz => 5,
    }
}

#[cfg(any(feature = "encode", test))]
pub(crate) const fn stored_length(
    compression: Compression,
    input_length: usize,
    encoded_length: usize,
) -> usize {
    match compression {
        Compression::Zlib | Compression::Bzip | Compression::Lzma => input_length,
        Compression::Copy | Compression::Zero | Compression::Dz => encoded_length,
    }
}

/// Copy ignores the stored compressed-length field and tracks decoded bytes.
pub(crate) const fn payload_read_length(chunk: Chunk, compression: Compression) -> u32 {
    match compression {
        Compression::Copy => chunk.decompressed_length,
        _ => chunk.compressed_length,
    }
}

pub(crate) const fn has_placeholder_length(chunk: Chunk) -> bool {
    let compressed = chunk.flags & (CHUNK_LZMA | CHUNK_ZLIB | CHUNK_BZIP | CHUNK_DZ) != 0;
    compressed && chunk.compressed_length == chunk.decompressed_length
}

/// Whether a chunk occupies bytes and can delimit a neighboring stream.
pub(crate) const fn is_physical_boundary(chunk: Chunk) -> bool {
    if chunk.flags & CHUNK_ZERO != 0 {
        return false;
    }
    // BZip2's valid empty stream is 14 bytes even though both stored lengths
    // are zero. Other zero-length records are virtual placeholders.
    chunk.compressed_length != 0 || chunk.flags & CHUNK_BZIP != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_flags_follow_the_registered_original_order() {
        let all = CHUNK_ZERO | CHUNK_BZIP | CHUNK_COPYCOMP | CHUNK_ZLIB | CHUNK_LZMA | CHUNK_DZ;
        assert_eq!(registered_compression(all), Some(Compression::Zero));
        assert_eq!(
            registered_compression(all & !CHUNK_ZERO),
            Some(Compression::Bzip)
        );
        assert_eq!(
            registered_compression(all & !(CHUNK_ZERO | CHUNK_BZIP)),
            Some(Compression::Copy)
        );
    }

    #[test]
    fn dz_settings_keep_the_original_writer_reader_asymmetry() {
        assert!(flags_require_range_settings(CHUNK_ZLIB | CHUNK_DZ));
        assert!(!compression_writes_range_settings(Compression::Zlib));
        assert!(compression_writes_range_settings(Compression::Dz));
    }
}
