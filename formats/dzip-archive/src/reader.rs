use crate::error::{DzipError, Result};
use crate::format::*;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{BufRead, BufReader, Read, Seek};

pub use crate::codec::DzDecodeContext;

pub struct DzipReader<R: Read + Seek> {
    reader: BufReader<R>,
}

impl<R: Read + Seek> DzipReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    pub fn read_archive_settings(&mut self) -> Result<ArchiveSettings> {
        let header = self.reader.read_u32::<LittleEndian>()?;
        if header != 0x5A525444 {
            // 'DTRZ' in little endian (ZRTD)
            return Err(DzipError::InvalidHeader);
        }

        let num_user_files = self.reader.read_u16::<LittleEndian>()?;
        let num_directories = self.reader.read_u16::<LittleEndian>()?;
        let version = self.reader.read_u8()?;
        if version != 0 {
            return Err(DzipError::UnsupportedVersion(version));
        }
        if num_user_files == 0 {
            return Err(DzipError::InvalidArchive(
                "archive must contain at least one user file".to_string(),
            ));
        }
        if num_directories == 0 {
            return Err(DzipError::InvalidArchive(
                "archive must contain the implicit root directory".to_string(),
            ));
        }

        Ok(ArchiveSettings {
            header,
            num_user_files,
            num_directories,
            version,
        })
    }

    pub fn read_strings(&mut self, count: usize) -> Result<Vec<String>> {
        self.read_strings_with_limit(count, usize::MAX - 1)
    }

    pub fn read_strings_with_limit(
        &mut self,
        count: usize,
        max_string_length: usize,
    ) -> Result<Vec<String>> {
        self.read_strings_with_limits(count, max_string_length, usize::MAX)
    }

    pub fn read_strings_with_limits(
        &mut self,
        count: usize,
        max_string_length: usize,
        max_total_length: usize,
    ) -> Result<Vec<String>> {
        self.read_raw_strings_with_limits(count, max_string_length, max_total_length)?
            .into_iter()
            .map(|value| Ok(String::from_utf8(value.into_bytes())?))
            .collect()
    }

    pub fn read_raw_strings(&mut self, count: usize) -> Result<Vec<ArchiveString>> {
        self.read_raw_strings_with_limits(count, usize::MAX - 1, usize::MAX)
    }

    pub fn read_raw_strings_with_limits(
        &mut self,
        count: usize,
        max_string_length: usize,
        max_total_length: usize,
    ) -> Result<Vec<ArchiveString>> {
        let mut strings = Vec::with_capacity(count);
        let mut total_length = 0usize;
        for _ in 0..count {
            let s = self.read_null_terminated_bytes(max_string_length)?;
            total_length = total_length
                .checked_add(s.encoded_len())
                .ok_or_else(|| DzipError::InvalidArchive("metadata size overflow".to_string()))?;
            if total_length > max_total_length {
                return Err(DzipError::LimitExceeded {
                    resource: "metadata strings",
                    limit: max_total_length as u64,
                    actual: total_length as u64,
                });
            }
            strings.push(s);
        }
        Ok(strings)
    }

    fn read_null_terminated_bytes(&mut self, max_length: usize) -> Result<ArchiveString> {
        let mut bytes = Vec::new();
        let max_with_terminator = max_length.saturating_add(1);
        let read = self
            .reader
            .by_ref()
            .take(max_with_terminator as u64)
            .read_until(0, &mut bytes)?;
        if read == 0 || bytes.last() != Some(&0) {
            return Err(DzipError::InvalidArchive(
                "unterminated or oversized metadata string".to_string(),
            ));
        }
        bytes.pop();
        Ok(ArchiveString::from_terminated_field(bytes))
    }

    /// Reads the User-File to Chunk-And-Directory list.
    /// Returns a vector of tuples: (Directory ID, List of Chunk IDs).
    pub fn read_file_chunk_map(&mut self, num_files: usize) -> Result<Vec<(u16, Vec<u16>)>> {
        self.read_file_chunk_map_with_limit(num_files, usize::MAX)
    }

    pub fn read_file_chunk_map_with_limit(
        &mut self,
        num_files: usize,
        max_chunks_per_file: usize,
    ) -> Result<Vec<(u16, Vec<u16>)>> {
        self.read_file_chunk_map_with_limits(num_files, max_chunks_per_file, usize::MAX)
    }

    pub fn read_file_chunk_map_with_limits(
        &mut self,
        num_files: usize,
        max_chunks_per_file: usize,
        max_total_chunks: usize,
    ) -> Result<Vec<(u16, Vec<u16>)>> {
        let mut map = Vec::with_capacity(num_files);
        let mut total_chunks = 0usize;
        for _ in 0..num_files {
            let dir_id = self.reader.read_u16::<LittleEndian>()?;
            let mut chunks = Vec::new();
            loop {
                let chunk_id = self.reader.read_u16::<LittleEndian>()?;
                if chunk_id == 0xFFFF {
                    break;
                }
                if chunks.len() >= max_chunks_per_file {
                    return Err(DzipError::LimitExceeded {
                        resource: "chunks per entry",
                        limit: max_chunks_per_file as u64,
                        actual: chunks.len() as u64 + 1,
                    });
                }
                total_chunks = total_chunks.checked_add(1).ok_or_else(|| {
                    DzipError::InvalidArchive("chunk-reference count overflow".to_string())
                })?;
                if total_chunks > max_total_chunks {
                    return Err(DzipError::LimitExceeded {
                        resource: "chunk references",
                        limit: max_total_chunks as u64,
                        actual: total_chunks as u64,
                    });
                }
                chunks.push(chunk_id);
            }
            map.push((dir_id, chunks));
        }
        Ok(map)
    }

    pub fn read_chunk_settings(&mut self) -> Result<ChunkSettings> {
        let num_archive_files = self.reader.read_u16::<LittleEndian>()?;
        let num_chunks = self.reader.read_u16::<LittleEndian>()?;
        Ok(ChunkSettings {
            num_archive_files,
            num_chunks,
        })
    }

    pub fn read_chunks(&mut self, count: usize) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = self.reader.read_u32::<LittleEndian>()?;
            let compressed_length = self.reader.read_u32::<LittleEndian>()?;
            let decompressed_length = self.reader.read_u32::<LittleEndian>()?;
            let flags = self.reader.read_u16::<LittleEndian>()?;
            let file = self.reader.read_u16::<LittleEndian>()?;
            chunks.push(Chunk {
                offset,
                compressed_length,
                decompressed_length,
                flags,
                file,
            });
        }
        Ok(chunks)
    }

    pub fn read_global_settings(&mut self) -> Result<RangeSettings> {
        let win_size = self.reader.read_u8()?;
        let flags = self.reader.read_u8()?;
        let offset_table_size = self.reader.read_u8()?;
        let offset_tables = self.reader.read_u8()?;
        let offset_contexts = self.reader.read_u8()?;
        let ref_length_table_size = self.reader.read_u8()?;
        let ref_length_tables = self.reader.read_u8()?;
        let ref_offset_table_size = self.reader.read_u8()?;
        let ref_offset_tables = self.reader.read_u8()?;
        let big_min_match = self.reader.read_u8()?;

        Ok(RangeSettings {
            win_size,
            flags,
            offset_table_size,
            offset_tables,
            offset_contexts,
            ref_length_table_size,
            ref_length_tables,
            ref_offset_table_size,
            ref_offset_tables,
            big_min_match,
        })
    }

    pub fn read_file_list(&mut self, num_archive_files: usize) -> Result<Vec<String>> {
        self.read_strings(num_archive_files)
    }

    pub fn read_raw_file_list(&mut self, num_archive_files: usize) -> Result<Vec<ArchiveString>> {
        self.read_raw_strings(num_archive_files)
    }

    pub fn read_raw_file_list_with_limits(
        &mut self,
        num_archive_files: usize,
        max_string_length: usize,
        max_total_length: usize,
    ) -> Result<Vec<ArchiveString>> {
        self.read_raw_strings_with_limits(num_archive_files, max_string_length, max_total_length)
    }

    pub fn position(&mut self) -> std::io::Result<u64> {
        self.reader.stream_position()
    }

    pub fn stream_len(&mut self) -> std::io::Result<u64> {
        let position = self.reader.stream_position()?;
        let length = self.reader.seek(std::io::SeekFrom::End(0))?;
        self.reader.seek(std::io::SeekFrom::Start(position))?;
        Ok(length)
    }

    pub fn read_chunk_data(&mut self, chunk: &Chunk) -> Result<Vec<u8>> {
        let mut input = Vec::new();
        Self::decompress_chunk_data(
            &mut self.reader,
            chunk,
            None,
            crate::codec::CodecLimits::UNLIMITED,
            &mut input,
            Vec::new(),
        )
    }

    pub fn read_chunk_data_with_volumes(
        &mut self,
        chunk: &Chunk,
        volume_source: &mut dyn VolumeSource,
    ) -> Result<Vec<u8>> {
        self.read_chunk_data_with_context(chunk, volume_source, None)
    }

    pub fn read_chunk_data_with_context(
        &mut self,
        chunk: &Chunk,
        volume_source: &mut dyn VolumeSource,
        dz_context: Option<&DzDecodeContext>,
    ) -> Result<Vec<u8>> {
        self.read_chunk_data_with_context_and_limits(
            chunk,
            volume_source,
            dz_context,
            crate::codec::CodecLimits::UNLIMITED,
        )
    }

    pub fn read_chunk_data_with_context_and_limits(
        &mut self,
        chunk: &Chunk,
        volume_source: &mut dyn VolumeSource,
        dz_context: Option<&DzDecodeContext>,
        limits: crate::codec::CodecLimits,
    ) -> Result<Vec<u8>> {
        let mut input = Vec::new();
        self.read_chunk_data_with_context_and_limits_reusing(
            chunk,
            volume_source,
            dz_context,
            limits,
            &mut input,
            Vec::new(),
        )
    }

    pub(crate) fn read_chunk_data_with_context_and_limits_reusing(
        &mut self,
        chunk: &Chunk,
        volume_source: &mut dyn VolumeSource,
        dz_context: Option<&DzDecodeContext>,
        limits: crate::codec::CodecLimits,
        input: &mut Vec<u8>,
        output: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if chunk.file == 0 {
            Self::decompress_chunk_data(&mut self.reader, chunk, dz_context, limits, input, output)
        } else {
            let reader = volume_source.open_volume(chunk.file)?;
            Self::decompress_chunk_data(reader, chunk, dz_context, limits, input, output)
        }
    }

    pub fn load_dz_context(
        &mut self,
        chunks: &[Chunk],
        settings: RangeSettings,
        volume_source: &mut dyn VolumeSource,
    ) -> Result<DzDecodeContext> {
        self.load_dz_context_with_limits(
            chunks,
            settings,
            volume_source,
            crate::codec::CodecLimits::UNLIMITED,
        )
    }

    pub fn load_dz_context_with_limits(
        &mut self,
        chunks: &[Chunk],
        settings: RangeSettings,
        volume_source: &mut dyn VolumeSource,
        limits: crate::codec::CodecLimits,
    ) -> Result<DzDecodeContext> {
        let settings = settings.validate()?;
        let common_chunks: Vec<_> = chunks
            .iter()
            .filter(|chunk| crate::compat::original::is_dedicated_combuf(chunk.flags))
            .copied()
            .collect();
        if common_chunks.is_empty() {
            return DzDecodeContext::from_encoded_chunks(settings, Vec::new());
        }

        let retained_bytes = common_chunks.iter().try_fold(0usize, |total, chunk| {
            total
                .checked_add(chunk.compressed_length as usize)
                .ok_or_else(|| DzipError::InvalidArchive("COMBUF size overflow".to_string()))
        })?;
        if retained_bytes > limits.max_workspace_size {
            return Err(DzipError::LimitExceeded {
                resource: "COMBUF retained bytes",
                limit: limits.max_workspace_size as u64,
                actual: retained_bytes as u64,
            });
        }

        let mut encoded_chunks = Vec::with_capacity(common_chunks.len());
        for chunk in common_chunks {
            let mut data = vec![0u8; chunk.compressed_length as usize];
            if chunk.file == 0 {
                self.reader
                    .seek(std::io::SeekFrom::Start(u64::from(chunk.offset)))?;
                self.reader.read_exact(&mut data)?;
            } else {
                let reader = volume_source.open_volume(chunk.file)?;
                reader.seek(std::io::SeekFrom::Start(u64::from(chunk.offset)))?;
                reader.read_exact(&mut data)?;
            }
            encoded_chunks.push(data);
        }
        DzDecodeContext::from_encoded_chunks(settings, encoded_chunks)
    }

    fn decompress_chunk_data(
        reader: &mut dyn ReadSeek,
        chunk: &Chunk,
        dz_context: Option<&DzDecodeContext>,
        limits: crate::codec::CodecLimits,
        input: &mut Vec<u8>,
        output: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let encoding = crate::codec::ChunkEncoding::from_flags(chunk.flags)?;

        // Zero chunks have no physical stream and may carry a virtual offset.
        if encoding.compression == crate::codec::Compression::Zero {
            input.clear();
            return crate::codec::decode_with_buffer(
                encoding,
                &[],
                crate::codec::DecodeContext {
                    expected_len: chunk.decompressed_length as usize,
                    dz: dz_context,
                    limits,
                },
                output,
            );
        }

        reader.seek(std::io::SeekFrom::Start(chunk.offset as u64))?;
        // dzip.exe's copy decoder tracks the uncompressed byte count and
        // ignores the stored compressed-length field. Valid copy chunks have
        // equal lengths, but using the original field preserves compatibility
        // with malformed legacy headers.
        let stored_length =
            crate::compat::original::payload_read_length(*chunk, encoding.compression);
        input.clear();
        input.resize(stored_length as usize, 0);
        reader.read_exact(input)?;

        crate::codec::decode_with_buffer(
            encoding,
            input,
            crate::codec::DecodeContext {
                expected_len: chunk.decompressed_length as usize,
                dz: dz_context,
                limits,
            },
            output,
        )
    }
}

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

pub trait VolumeSource {
    /// Open the volume with the given index (1-based, corresponding to the file list)
    fn open_volume(&mut self, id: u16) -> Result<&mut dyn ReadSeek>;

    fn volume_len(&mut self, id: u16) -> Result<Option<u64>> {
        let reader = self.open_volume(id)?;
        let position = reader.stream_position()?;
        let length = reader.seek(std::io::SeekFrom::End(0))?;
        reader.seek(std::io::SeekFrom::Start(position))?;
        Ok(Some(length))
    }
}

/// Corrects chunk sizes based on actual file boundaries.
///
/// Some legacy archives have incorrect compressed-length headers (for example, the uncompressed size).
/// This function clamps compressed lengths to the available space between chunks or EOF.
///
/// # Arguments
/// * `chunks` - The list of chunks to correct.
/// * `file_sizes` - specific file sizes mapped by file ID (0 for main, 1+ for volumes).
pub fn correct_chunk_sizes(
    chunks: &mut [crate::format::Chunk],
    file_sizes: &std::collections::HashMap<u16, u64>,
) {
    let resolved = crate::format::resolve_chunk_layout(chunks, file_sizes);
    for (chunk, resolved) in chunks.iter_mut().zip(resolved) {
        chunk.compressed_length = resolved.physical_length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn copy_chunks_use_decompressed_length_like_dzip_original() {
        let mut reader = DzipReader::new(Cursor::new(b"payload".to_vec()));
        let chunk = Chunk {
            offset: 0,
            compressed_length: 1,
            decompressed_length: 7,
            flags: CHUNK_COPYCOMP | CHUNK_RANDOMACCESS,
            file: 0,
        };

        assert_eq!(reader.read_chunk_data(&chunk).unwrap(), b"payload");
    }

    #[test]
    fn raw_string_reader_preserves_non_utf8_metadata_bytes() {
        let mut reader = DzipReader::new(Cursor::new(vec![0xff, b'a', 0]));
        let values = reader.read_raw_strings(1).unwrap();
        assert_eq!(values[0].as_bytes(), &[0xff, b'a']);

        let mut reader = DzipReader::new(Cursor::new(vec![0xff, 0]));
        assert!(reader.read_strings(1).is_err());
    }
}
