//! High-level archive reading, indexing, and safe extraction.

use crate::codec::{ChunkEncoding, CodecLimits, Compression, DzDecodeContext};
use crate::format::{
    ArchivePath, ArchiveSettings, CHUNK_COMBUF, Chunk, RangeSettings, RawArchive, ResolvedChunk,
    resolve_chunk_layout, resolve_volume_chunk_layout,
};
use crate::path::{ArchivePathKey, resolve_relative_path};
use crate::reader::{DzipReader, VolumeSource};
use crate::volume::FileSystemVolumeManager;
use crate::{DzipError, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

pub use crate::options::{ReadLimits, ReadOptions};

mod metadata;

use metadata::{decode_archive_strings, parse_metadata};

impl RawArchive {
    /// Parse lossless metadata without resolving host paths or opening
    /// auxiliary volumes.
    pub fn read_from<R: Read + Seek>(reader: R) -> Result<Self> {
        Self::read_from_with_options(reader, ReadOptions::default())
    }

    pub fn read_from_with_options<R: Read + Seek>(reader: R, options: ReadOptions) -> Result<Self> {
        parse_metadata(&mut DzipReader::new(reader), &options)
    }

    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_path_with_options(path, ReadOptions::default())
    }

    pub fn open_path_with_options(path: impl AsRef<Path>, options: ReadOptions) -> Result<Self> {
        Self::read_from_with_options(File::open(path)?, options)
    }
}

/// A main volume whose metadata has been parsed exactly once and is waiting
/// for an auxiliary-volume provider.
pub struct ArchivePreparation<R: Read + Seek> {
    reader: DzipReader<R>,
    metadata: RawArchive,
    options: ReadOptions,
}

impl<R: Read + Seek> ArchivePreparation<R> {
    pub fn read(reader: R, options: ReadOptions) -> Result<Self> {
        let mut reader = DzipReader::new(reader);
        let metadata = parse_metadata(&mut reader, &options)?;
        Ok(Self {
            reader,
            metadata,
            options,
        })
    }

    pub const fn metadata(&self) -> &RawArchive {
        &self.metadata
    }

    pub fn open<V: VolumeSource>(self, volumes: V) -> Result<Archive<R, V>> {
        finish_open(self.reader, volumes, self.metadata, &self.options)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryId(pub usize);

#[derive(Debug, Clone)]
pub struct Entry {
    id: EntryId,
    path: PathBuf,
    raw_path: ArchivePath,
    chunk_ids: Vec<u16>,
    segments: Vec<EntrySegment>,
    decompressed_size: u64,
    compression: Compression,
    volume: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySegment {
    chunk_id: u16,
    decoded_range: std::ops::Range<u64>,
    encoding: ChunkEncoding,
    volume: u16,
}

impl EntrySegment {
    pub const fn chunk_id(&self) -> u16 {
        self.chunk_id
    }

    pub const fn decoded_range(&self) -> &std::ops::Range<u64> {
        &self.decoded_range
    }

    pub const fn encoding(&self) -> ChunkEncoding {
        self.encoding
    }

    pub const fn volume(&self) -> u16 {
        self.volume
    }
}

impl Entry {
    pub const fn id(&self) -> EntryId {
        self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Exact path bytes reconstructed from the archive string table.
    pub const fn raw_path(&self) -> &ArchivePath {
        &self.raw_path
    }

    pub fn chunk_ids(&self) -> &[u16] {
        &self.chunk_ids
    }

    pub fn segments(&self) -> &[EntrySegment] {
        &self.segments
    }

    pub const fn decompressed_size(&self) -> u64 {
        self.decompressed_size
    }

    pub const fn compression(&self) -> Compression {
        self.compression
    }

    pub const fn volume(&self) -> u16 {
        self.volume
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveIndex {
    raw: RawArchive,
    entries: Vec<Entry>,
    /// Compatibility view whose compressed lengths are resolved physical
    /// lengths. The final chunk of an unopened auxiliary volume retains its
    /// stored length until that volume is first accessed.
    resolved_chunks: Vec<ResolvedChunk>,
    volume_files: Vec<String>,
}

impl ArchiveIndex {
    pub const fn archive_settings(&self) -> &ArchiveSettings {
        &self.raw.settings
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn chunks(&self) -> impl ExactSizeIterator<Item = Chunk> + '_ {
        self.resolved_chunks
            .iter()
            .copied()
            .map(ResolvedChunk::decoder_chunk)
    }

    pub fn chunk(&self, index: usize) -> Option<Chunk> {
        self.resolved_chunks
            .get(index)
            .copied()
            .map(ResolvedChunk::decoder_chunk)
    }

    pub fn stored_chunks(&self) -> &[Chunk] {
        &self.raw.chunks
    }

    pub fn resolved_chunks(&self) -> &[ResolvedChunk] {
        &self.resolved_chunks
    }

    pub const fn raw_metadata(&self) -> &RawArchive {
        &self.raw
    }

    pub fn volume_files(&self) -> &[String] {
        &self.volume_files
    }

    pub const fn range_settings(&self) -> Option<RangeSettings> {
        self.raw.range_settings
    }

    /// Whether the archive contains the standalone dictionary record used by
    /// the original DZ common-buffer mode.
    pub fn has_dz_common_buffer(&self) -> bool {
        self.raw
            .chunks
            .iter()
            .any(|chunk| crate::compat::original::is_dedicated_combuf(chunk.flags))
    }

    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.entries.get(id.0)
    }

    pub fn find(&self, path: impl AsRef<Path>) -> Option<&Entry> {
        let needle = ArchivePathKey::from_path(path.as_ref());
        self.entries
            .iter()
            .find(|entry| ArchivePathKey::from_path(&entry.path) == needle)
    }

    pub fn find_raw(&self, path: &[u8]) -> Option<&Entry> {
        let needle = ArchivePathKey::from_archive_bytes(path);
        self.entries
            .iter()
            .find(|entry| ArchivePathKey::from_archive_bytes(entry.raw_path.as_bytes()) == needle)
    }
}

pub struct Archive<R: Read + Seek, V: VolumeSource> {
    reader: DzipReader<R>,
    volumes: V,
    index: ArchiveIndex,
    dz_context: Option<DzDecodeContext>,
    volume_sizes: HashMap<u16, u64>,
    volume_chunk_indices: HashMap<u16, Vec<usize>>,
    codec_limits: CodecLimits,
    input_buffer: Vec<u8>,
    decode_buffer: Vec<u8>,
}

impl<R: Read + Seek, V: VolumeSource> Archive<R, V> {
    pub fn open_with_volumes(reader: R, volumes: V) -> Result<Self> {
        Self::open_with_options(reader, volumes, ReadOptions::default())
    }

    pub fn open_with_options(reader: R, volumes: V, options: ReadOptions) -> Result<Self> {
        ArchivePreparation::read(reader, options)?.open(volumes)
    }

    pub fn index(&self) -> &ArchiveIndex {
        &self.index
    }

    pub fn entries(&self) -> &[Entry] {
        self.index.entries()
    }

    pub fn volume_source_mut(&mut self) -> &mut V {
        &mut self.volumes
    }

    pub const fn volume_source(&self) -> &V {
        &self.volumes
    }

    pub fn is_volume_resolved(&self, id: u16) -> bool {
        self.volume_sizes.contains_key(&id)
    }

    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.index.entry(id)
    }

    pub fn find_entry(&self, path: impl AsRef<Path>) -> Option<&Entry> {
        self.index.find(path)
    }

    pub fn find_entry_raw(&self, path: &[u8]) -> Option<&Entry> {
        self.index.find_raw(path)
    }

    pub fn read_entry(&mut self, id: EntryId) -> Result<Vec<u8>> {
        let expected = self
            .entry(id)
            .ok_or_else(|| DzipError::EntryNotFound(id.0.to_string()))?
            .decompressed_size;
        let capacity = usize::try_from(expected).map_err(|_| DzipError::LimitExceeded {
            resource: "entry size",
            limit: usize::MAX as u64,
            actual: expected,
        })?;
        let mut output = Vec::with_capacity(capacity);
        self.read_entry_to(id, &mut output)?;
        Ok(output)
    }

    pub fn read_entry_by_path(&mut self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        let id = self
            .find_entry(path)
            .map(Entry::id)
            .ok_or_else(|| DzipError::EntryNotFound(path.display().to_string()))?;
        self.read_entry(id)
    }

    pub fn read_entry_to<W: Write>(&mut self, id: EntryId, mut output: W) -> Result<u64> {
        let entry = self
            .entry(id)
            .ok_or_else(|| DzipError::EntryNotFound(id.0.to_string()))?;
        let expected = entry.decompressed_size;
        let codec = entry.compression;
        let chunk_ids = entry.chunk_ids.clone();
        let mut written = 0u64;
        for chunk_id in chunk_ids {
            let volume = self
                .index
                .raw
                .chunks
                .get(chunk_id as usize)
                .ok_or_else(|| invalid_archive(format!("invalid chunk index {chunk_id}")))?
                .file;
            self.resolve_volume_layout(volume)?;
            let chunk = self
                .index
                .resolved_chunks
                .get(chunk_id as usize)
                .copied()
                .map(ResolvedChunk::decoder_chunk)
                .ok_or_else(|| invalid_archive(format!("invalid chunk index {chunk_id}")))?;
            let encoding = ChunkEncoding::from_flags(chunk.flags)?;
            if encoding.compression == Compression::Dz {
                self.ensure_dz_context()?;
            }
            let bytes = self
                .reader
                .read_chunk_data_with_context_and_limits_reusing(
                    &chunk,
                    &mut self.volumes,
                    self.dz_context.as_ref(),
                    self.codec_limits,
                    &mut self.input_buffer,
                    std::mem::take(&mut self.decode_buffer),
                )?;
            output.write_all(&bytes)?;
            written = written
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| invalid_archive("entry output size overflow"))?;
            self.decode_buffer = bytes;
        }
        if written != expected {
            return Err(invalid_archive(format!(
                "{codec} entry length mismatch: expected {expected}, got {written}"
            )));
        }
        Ok(written)
    }

    /// Resolve the physical span of every referenced auxiliary-volume chunk.
    ///
    /// Normal reading remains lazy. Frontends that display exact packed sizes
    /// can opt into this pass once while opening an archive.
    pub fn resolve_all_volumes(&mut self) -> Result<()> {
        let mut volumes = self
            .index
            .raw
            .chunks
            .iter()
            .map(|chunk| chunk.file)
            .collect::<Vec<_>>();
        volumes.sort_unstable();
        volumes.dedup();
        for volume in volumes {
            self.resolve_volume_layout(volume)?;
        }
        Ok(())
    }

    /// Resolve the physical spans for one available volume without decoding
    /// any payload. This is useful for frontends that already hold the volume
    /// bytes and need exact packed-size summaries.
    pub fn resolve_volume(&mut self, id: u16) -> Result<()> {
        self.resolve_volume_layout(id)
    }

    fn resolve_volume_layout(&mut self, id: u16) -> Result<()> {
        if self.volume_sizes.contains_key(&id) {
            return Ok(());
        }
        let length = self
            .volumes
            .volume_len(id)?
            .ok_or(DzipError::VolumeNotFound(id))?;
        if let Some(indices) = self.volume_chunk_indices.get(&id) {
            resolve_volume_chunk_layout(
                &self.index.raw.chunks,
                indices,
                length,
                &mut self.index.resolved_chunks,
            );
            for &index in indices {
                let decoder_chunk = self.index.resolved_chunks[index].decoder_chunk();
                validate_chunk_limits(
                    std::slice::from_ref(&decoder_chunk),
                    &ReadLimits {
                        max_chunk_input_size: self.codec_limits.max_input_size,
                        max_chunk_output_size: self.codec_limits.max_output_size,
                        ..ReadLimits::unlimited()
                    },
                )?;
            }
        }
        self.volume_sizes.insert(id, length);
        Ok(())
    }

    fn ensure_dz_context(&mut self) -> Result<()> {
        if self.dz_context.is_some() {
            return Ok(());
        }
        let settings = self
            .index
            .range_settings()
            .ok_or_else(|| invalid_archive("DZ chunks require archive-wide settings"))?;
        let common_volumes = self
            .index
            .raw
            .chunks
            .iter()
            .filter(|chunk| crate::compat::original::is_dedicated_combuf(chunk.flags))
            .map(|chunk| chunk.file)
            .collect::<Vec<_>>();
        for volume in common_volumes {
            self.resolve_volume_layout(volume)?;
        }
        let decoder_chunks = self.index.chunks().collect::<Vec<_>>();
        let context = self.reader.load_dz_context_with_limits(
            &decoder_chunks,
            settings,
            &mut self.volumes,
            self.codec_limits,
        )?;
        self.codec_limits.max_workspace_size = self
            .codec_limits
            .max_workspace_size
            .saturating_sub(context.retained_bytes());
        self.dz_context = Some(context);
        Ok(())
    }
}

impl Archive<File, FileSystemVolumeManager> {
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_path_with_options(path, ReadOptions::default())
    }

    pub fn open_path_with_options(path: impl AsRef<Path>, options: ReadOptions) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let mut reader = DzipReader::new(file);
        let parsed = parse_metadata(&mut reader, &options)?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let volume_files = decode_archive_strings(&parsed.volume_files)?;
        let volumes = FileSystemVolumeManager::new(base_dir.to_path_buf(), volume_files);
        finish_open(reader, volumes, parsed, &options)
    }
}

fn finish_open<R: Read + Seek, V: VolumeSource>(
    mut reader: DzipReader<R>,
    volumes: V,
    parsed: RawArchive,
    options: &ReadOptions,
) -> Result<Archive<R, V>> {
    let mut sizes = HashMap::new();
    sizes.insert(0, reader.stream_len()?);
    for chunk in &parsed.chunks {
        if !crate::compat::original::semantic_reader_supports_flags(chunk.flags) {
            return Err(DzipError::UnsupportedCompression(chunk.flags));
        }
    }
    if let Some(settings) = parsed.range_settings {
        settings.validate()?;
    }

    let resolved_chunks = resolve_chunk_layout(&parsed.chunks, &sizes);
    let decoder_chunks = resolved_chunks
        .iter()
        .copied()
        .map(ResolvedChunk::decoder_chunk)
        .collect::<Vec<_>>();
    validate_chunk_limits(&decoder_chunks, &options.limits)?;

    let volume_files = decode_archive_strings(&parsed.volume_files)?;
    let mut volume_chunk_indices = HashMap::<u16, Vec<usize>>::new();
    for (index, chunk) in parsed.chunks.iter().enumerate() {
        volume_chunk_indices
            .entry(chunk.file)
            .or_default()
            .push(index);
    }
    let index = build_index(parsed, resolved_chunks, volume_files, options)?;

    Ok(Archive {
        reader,
        volumes,
        index,
        dz_context: None,
        volume_sizes: sizes,
        volume_chunk_indices,
        codec_limits: CodecLimits {
            max_input_size: options.limits.max_chunk_input_size,
            max_output_size: options.limits.max_chunk_output_size,
            max_workspace_size: options.limits.max_codec_workspace,
        },
        input_buffer: Vec::new(),
        decode_buffer: Vec::new(),
    })
}

fn validate_chunk_limits(chunks: &[Chunk], limits: &ReadLimits) -> Result<()> {
    for chunk in chunks {
        check_limit(
            "compressed chunk size",
            limits.max_chunk_input_size as u64,
            chunk.compressed_length as u64,
        )?;
        check_limit(
            "decompressed chunk size",
            limits.max_chunk_output_size as u64,
            chunk.decompressed_length as u64,
        )?;
    }
    Ok(())
}

fn build_index(
    parsed: RawArchive,
    resolved_chunks: Vec<ResolvedChunk>,
    volume_files: Vec<String>,
    options: &ReadOptions,
) -> Result<ArchiveIndex> {
    let file_count = parsed.settings.num_user_files as usize;
    let mut entries = Vec::with_capacity(parsed.files.len());
    let mut total_output = 0u64;

    for (index, file) in parsed.files.iter().enumerate() {
        let directory_id = file.directory_id;
        let chunk_ids = &file.chunk_ids;
        let file_name = parsed
            .strings
            .get(index)
            .ok_or_else(|| invalid_archive(format!("missing file name at index {index}")))?;
        let mut archive_path = Vec::new();
        if directory_id > 0 {
            let directory_index = file_count
                .checked_add(directory_id as usize - 1)
                .ok_or_else(|| invalid_archive("directory index overflow"))?;
            let directory = parsed.strings.get(directory_index).ok_or_else(|| {
                invalid_archive(format!(
                    "invalid directory ID {directory_id} for {}",
                    file_name.to_string_lossy()
                ))
            })?;
            archive_path.extend_from_slice(directory.as_bytes());
            if !archive_path.ends_with(b"/") && !archive_path.ends_with(b"\\") {
                archive_path.push(b'\\');
            }
        }
        archive_path.extend_from_slice(file_name.as_bytes());
        let raw_path = ArchivePath::new(archive_path)?;
        let path = resolve_relative_path(raw_path.as_str()?)?;

        let mut decompressed_size = 0u64;
        let mut compression = Compression::Copy;
        let mut volume = 0;
        let mut segments = Vec::with_capacity(chunk_ids.len());
        for (part, chunk_id) in chunk_ids.iter().enumerate() {
            let chunk = parsed
                .chunks
                .get(*chunk_id as usize)
                .ok_or_else(|| invalid_archive(format!("invalid chunk index {chunk_id}")))?;
            let encoding = ChunkEncoding::from_flags(chunk.flags)?;
            if chunk.flags & CHUNK_COMBUF != 0 && encoding.compression == Compression::Dz {
                return Err(invalid_archive(format!(
                    "entry {} uses the unsafe DZ | COMBUF combination",
                    path.display()
                )));
            }
            if chunk.file as usize > parsed.volume_files.len() {
                return Err(DzipError::VolumeNotFound(chunk.file));
            }
            if part == 0 {
                compression = encoding.compression;
                volume = chunk.file;
            }
            let start = decompressed_size;
            decompressed_size = decompressed_size
                .checked_add(chunk.decompressed_length as u64)
                .ok_or_else(|| invalid_archive("entry size overflow"))?;
            segments.push(EntrySegment {
                chunk_id: *chunk_id,
                decoded_range: start..decompressed_size,
                encoding,
                volume: chunk.file,
            });
        }
        check_limit(
            "entry size",
            options.limits.max_entry_size,
            decompressed_size,
        )?;
        total_output = total_output
            .checked_add(decompressed_size)
            .ok_or_else(|| invalid_archive("total output size overflow"))?;

        entries.push(Entry {
            id: EntryId(index),
            path,
            raw_path,
            chunk_ids: chunk_ids.clone(),
            segments,
            decompressed_size,
            compression,
            volume,
        });
    }
    check_limit(
        "total output size",
        options.limits.max_total_output,
        total_output,
    )?;

    Ok(ArchiveIndex {
        raw: parsed,
        entries,
        resolved_chunks,
        volume_files,
    })
}

fn check_limit(resource: &'static str, limit: u64, actual: u64) -> Result<()> {
    if actual > limit {
        return Err(DzipError::LimitExceeded {
            resource,
            limit,
            actual,
        });
    }
    Ok(())
}

fn invalid_archive(message: impl Into<String>) -> DzipError {
    DzipError::InvalidArchive(message.into())
}
