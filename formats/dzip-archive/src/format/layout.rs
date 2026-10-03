//! Resolution of stored chunk records into physical byte spans.

use super::Chunk;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedChunk {
    /// Exact record read from the chunk table.
    pub stored: Chunk,
    /// Number of bytes occupied by the physical stream after applying the
    /// original packer's placeholder-length convention.
    pub physical_length: u32,
}

impl ResolvedChunk {
    /// Return a compatibility view suitable for the existing decoder API.
    pub const fn decoder_chunk(self) -> Chunk {
        Chunk {
            compressed_length: self.physical_length,
            ..self.stored
        }
    }
}

/// Resolve physical lengths without mutating the stored header records.
pub fn resolve_chunk_layout(
    chunks: &[Chunk],
    file_sizes: &HashMap<u16, u64>,
) -> Vec<ResolvedChunk> {
    let mut resolved = chunks
        .iter()
        .copied()
        .map(|stored| ResolvedChunk {
            physical_length: stored.compressed_length,
            stored,
        })
        .collect::<Vec<_>>();
    let mut chunks_by_file: HashMap<u16, Vec<usize>> = HashMap::new();

    for (index, chunk) in chunks.iter().enumerate() {
        if !crate::compat::original::is_physical_boundary(*chunk) {
            resolved[index].physical_length = 0;
            continue;
        }
        chunks_by_file.entry(chunk.file).or_default().push(index);
    }

    for (file_id, mut indices) in chunks_by_file {
        indices.sort_by_key(|&index| chunks[index].offset);
        let file_size = file_sizes.get(&file_id).copied();

        for (position, &index) in indices.iter().enumerate() {
            let chunk = chunks[index];
            let start = u64::from(chunk.offset);
            let end = indices
                .get(position + 1)
                .map(|&next| u64::from(chunks[next].offset))
                .or(file_size);
            let Some(end) = end else {
                // Auxiliary volumes are deliberately opened lazily. Until EOF
                // is known, retain the stored field as the best lossless view;
                // Archive resolves this final span before reading the volume.
                continue;
            };
            let available = end.saturating_sub(start).min(u64::from(u32::MAX)) as u32;
            resolved[index].physical_length =
                if crate::compat::original::has_placeholder_length(chunk) {
                    available
                } else {
                    chunk.compressed_length.min(available)
                };
        }
    }

    resolved
}

/// Resolve only one physical volume into an existing lossless layout.
///
/// Callers that open auxiliary volumes lazily can build the volume-to-chunk
/// index once and avoid rescanning every record whenever a new volume is used.
#[cfg(any(feature = "decode", test))]
pub(crate) fn resolve_volume_chunk_layout(
    chunks: &[Chunk],
    volume_indices: &[usize],
    file_size: u64,
    resolved: &mut [ResolvedChunk],
) {
    let mut physical = Vec::with_capacity(volume_indices.len());
    for &index in volume_indices {
        let chunk = chunks[index];
        if crate::compat::original::is_physical_boundary(chunk) {
            physical.push(index);
        } else {
            resolved[index].physical_length = 0;
        }
    }
    physical.sort_by_key(|&index| chunks[index].offset);
    for (position, &index) in physical.iter().enumerate() {
        let chunk = chunks[index];
        let start = u64::from(chunk.offset);
        let end = physical
            .get(position + 1)
            .map(|&next| u64::from(chunks[next].offset))
            .unwrap_or(file_size);
        let available = end.saturating_sub(start).min(u64::from(u32::MAX)) as u32;
        resolved[index].physical_length = if crate::compat::original::has_placeholder_length(chunk)
        {
            available
        } else {
            chunk.compressed_length.min(available)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{CHUNK_BZIP, CHUNK_COPYCOMP, CHUNK_LZMA};

    #[test]
    fn resolution_preserves_stored_records_and_caps_large_files() {
        let chunks = [
            Chunk {
                offset: 10,
                compressed_length: 100,
                decompressed_length: 100,
                flags: CHUNK_LZMA,
                file: 0,
            },
            Chunk {
                offset: 30,
                compressed_length: 5,
                decompressed_length: 5,
                flags: CHUNK_COPYCOMP,
                file: 0,
            },
        ];
        let resolved = resolve_chunk_layout(&chunks, &HashMap::from([(0, u64::MAX)]));
        assert_eq!(resolved[0].stored, chunks[0]);
        assert_eq!(resolved[0].physical_length, 20);
        assert_eq!(resolved[1].physical_length, 5);
    }

    #[test]
    fn empty_bzip_stream_remains_a_physical_boundary() {
        let chunks = [
            Chunk {
                offset: 10,
                compressed_length: 0,
                decompressed_length: 0,
                flags: CHUNK_BZIP,
                file: 0,
            },
            Chunk {
                offset: 24,
                compressed_length: 5,
                decompressed_length: 5,
                flags: CHUNK_COPYCOMP,
                file: 0,
            },
        ];
        let resolved = resolve_chunk_layout(&chunks, &HashMap::from([(0, 29)]));
        assert_eq!(resolved[0].physical_length, 14);
        assert_eq!(resolved[1].physical_length, 5);
    }

    #[test]
    fn missing_volume_size_preserves_the_last_stored_length() {
        let chunks = [
            Chunk {
                offset: 10,
                compressed_length: 100,
                decompressed_length: 100,
                flags: CHUNK_LZMA,
                file: 1,
            },
            Chunk {
                offset: 30,
                compressed_length: 5,
                decompressed_length: 5,
                flags: CHUNK_COPYCOMP,
                file: 1,
            },
        ];
        let resolved = resolve_chunk_layout(&chunks, &HashMap::new());
        assert_eq!(resolved[0].physical_length, 20);
        assert_eq!(resolved[1].physical_length, 5);
    }

    #[test]
    fn per_volume_resolution_matches_whole_archive_resolution() {
        let chunks = [
            Chunk {
                offset: 4,
                compressed_length: 20,
                decompressed_length: 20,
                flags: CHUNK_LZMA,
                file: 1,
            },
            Chunk {
                offset: 12,
                compressed_length: 5,
                decompressed_length: 5,
                flags: CHUNK_COPYCOMP,
                file: 1,
            },
        ];
        let expected = resolve_chunk_layout(&chunks, &HashMap::from([(1, 17)]));
        let mut actual = resolve_chunk_layout(&chunks, &HashMap::new());
        resolve_volume_chunk_layout(&chunks, &[0, 1], 17, &mut actual);
        assert_eq!(actual, expected);
    }
}
