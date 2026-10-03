//! High-level deterministic Dzip archive creation.

use crate::codec::Compression;
use crate::format::*;
use crate::path::resolve_relative_path;
use crate::writer::DzipWriter;
use crate::{DzipError, Result};
use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub use crate::options::{EntryOptions, PackOptions};
mod plan;
mod volume;

use plan::{OriginalArchivePlan, PlannedEntry, calculate_header_size, effective_compression};
use volume::SingleVolumeSink;
pub use volume::{FileSystemVolumeSink, MemoryVolumeSink, VolumeSink, WriteSeek};

enum EntrySource {
    Bytes(Vec<u8>),
    Path(PathBuf),
}

struct BuilderEntry {
    archive_path: PathBuf,
    source: EntrySource,
    options: EntryOptions,
}

pub struct ArchiveBuilder {
    options: PackOptions,
    entries: Vec<BuilderEntry>,
}

impl Default for ArchiveBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveBuilder {
    pub fn new() -> Self {
        Self {
            options: PackOptions::default(),
            entries: Vec::new(),
        }
    }

    pub fn with_options(options: PackOptions) -> Self {
        Self {
            options,
            entries: Vec::new(),
        }
    }

    pub fn options(&self) -> &PackOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut PackOptions {
        &mut self.options
    }

    pub fn add_bytes(
        &mut self,
        archive_path: impl AsRef<Path>,
        bytes: impl Into<Vec<u8>>,
        options: EntryOptions,
    ) -> Result<&mut Self> {
        let archive_path = validate_archive_path(archive_path.as_ref())?;
        self.entries.push(BuilderEntry {
            archive_path,
            source: EntrySource::Bytes(bytes.into()),
            options,
        });
        Ok(self)
    }

    pub fn add_path(
        &mut self,
        archive_path: impl AsRef<Path>,
        source_path: impl Into<PathBuf>,
        options: EntryOptions,
    ) -> Result<&mut Self> {
        let archive_path = validate_archive_path(archive_path.as_ref())?;
        self.entries.push(BuilderEntry {
            archive_path,
            source: EntrySource::Path(source_path.into()),
            options,
        });
        Ok(self)
    }

    pub fn write_to<W: Write + Seek>(&self, writer: &mut W) -> Result<BuildReport> {
        if self.options.volume_names.len() != 1
            || self.entries.iter().any(|entry| entry.options.volume != 0)
        {
            return Err(invalid_input(
                "write_to supports one volume; use write_to_sink for split archives",
            ));
        }
        let mut sink = SingleVolumeSink { writer };
        self.write_to_sink(&mut sink)
    }

    pub fn write_to_path(&self, path: impl AsRef<Path>) -> Result<BuildReport> {
        let path = path.as_ref();
        let output_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let main_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid_input("output path has no UTF-8 file name"))?;
        let mut options = self.options.clone();
        if options.volume_names.is_empty() {
            options.volume_names.push(main_name.to_string());
        } else {
            options.volume_names[0] = main_name.to_string();
        }
        let prepared = self.prepare(&options)?;
        let mut sink = FileSystemVolumeSink::new(output_dir);
        write_prepared(&prepared, &options, &mut sink)
    }

    pub fn write_to_directory(&self, output_dir: impl AsRef<Path>) -> Result<BuildReport> {
        let mut sink = FileSystemVolumeSink::new(output_dir.as_ref());
        self.write_to_sink(&mut sink)
    }

    pub fn write_to_sink<S: VolumeSink>(&self, sink: &mut S) -> Result<BuildReport> {
        let prepared = self.prepare(&self.options)?;
        write_prepared(&prepared, &self.options, sink)
    }

    fn prepare<'a>(&'a self, options: &PackOptions) -> Result<Vec<PreparedEntry<'a>>> {
        if options.volume_names.is_empty() {
            return Err(invalid_input("at least one output volume is required"));
        }
        if options.volume_names.len() > u16::MAX as usize {
            return Err(invalid_input("more than 65535 output volumes"));
        }
        for name in &options.volume_names {
            validate_volume_name(name)?;
        }

        let prepare_entry = |entry: &'a BuilderEntry| -> Result<PreparedEntry<'a>> {
            if entry.options.volume as usize >= options.volume_names.len() {
                return Err(DzipError::VolumeNotFound(entry.options.volume));
            }
            let source = match &entry.source {
                EntrySource::Bytes(bytes) => PreparedSource::Bytes(bytes.as_slice()),
                EntrySource::Path(path) => PreparedSource::File {
                    path,
                    file: File::open(path).map_err(|error| source_error(path, error))?,
                },
            };
            effective_compression(entry.options)?;
            Ok(PreparedEntry {
                archive_path: entry.archive_path.clone(),
                source,
                options: entry.options,
            })
        };

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            self.entries.par_iter().map(prepare_entry).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            self.entries.iter().map(prepare_entry).collect()
        }
    }
}

struct PreparedEntry<'a> {
    archive_path: PathBuf,
    source: PreparedSource<'a>,
    options: EntryOptions,
}

enum PreparedSource<'a> {
    Bytes(&'a [u8]),
    File { path: &'a Path, file: File },
}

impl PreparedSource<'_> {
    fn load(&self) -> Result<Cow<'_, [u8]>> {
        match self {
            Self::Bytes(bytes) => Ok(Cow::Borrowed(bytes)),
            Self::File { path, file } => {
                let mut file = file
                    .try_clone()
                    .map_err(|error| source_error(path, error))?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|error| source_error(path, error))?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|error| source_error(path, error))?;
                Ok(Cow::Owned(bytes))
            }
        }
    }
}

struct EncodedEntry<'a> {
    index: usize,
    original_len: usize,
    bytes: Cow<'a, [u8]>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildReport {
    pub entries: usize,
    pub chunks: usize,
    pub input_bytes: u64,
    pub stored_bytes: u64,
    pub volumes: usize,
}

fn write_prepared<S: VolumeSink>(
    prepared: &[PreparedEntry<'_>],
    options: &PackOptions,
    sink: &mut S,
) -> Result<BuildReport> {
    let plan = OriginalArchivePlan::build(prepared)?;
    let logical_files = &plan.logical_files;
    let all_strings = &plan.strings;
    let file_directory_ids = &plan.file_directory_ids;
    let directory_count = plan.directory_count;
    let planned = &plan.entries;
    let file_count = checked_u16(logical_files.len(), "user file count")?;
    let volume_count = checked_u16(options.volume_names.len(), "archive volume count")?;

    sink.begin_archive()?;
    for (id, name) in options.volume_names.iter().enumerate() {
        let id = checked_u16(id, "archive volume index")?;
        sink.open_volume(id, name)?;
    }

    // DZ is the sole archive-scoped codec: all of its source buffers must be
    // alive while the common dictionary is constructed. Other codecs are
    // loaded and encoded later in bounded batches.
    let dz_loaded = planned
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.compression == Compression::Dz)
        .map(|(index, _)| prepared[index].source.load())
        .collect::<Result<Vec<_>>>()?;
    let dz_inputs = dz_loaded
        .iter()
        .map(|bytes| bytes.as_ref())
        .collect::<Vec<_>>();
    let encoded_dz = if dz_inputs.is_empty() {
        None
    } else {
        Some(crate::codec::dz::encode_archive(&dz_inputs, &options.dz)?)
    };
    let mut original_lengths = vec![None; planned.len()];
    for (index, entry) in planned.iter().enumerate() {
        if let Some(dz_index) = entry.dz_index {
            original_lengths[index] = Some(dz_loaded[dz_index].len());
        }
    }
    let mut input_bytes = dz_loaded.iter().try_fold(0u64, |total, bytes| {
        total
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| invalid_input("input byte count overflow"))
    })?;
    drop(dz_inputs);
    drop(dz_loaded);

    // dzip.exe writes global DZ settings only when DZ wins the packer
    // registration order. It still preserves every raw DCL flag in the chunk
    // record, which makes some combined flag sets intentionally asymmetric.
    let has_dz = planned
        .iter()
        .any(|entry| crate::compat::original::compression_writes_range_settings(entry.compression));
    let common_buffer = encoded_dz
        .as_ref()
        .and_then(|archive| archive.common_buffer.as_deref());
    let chunk_count = planned
        .len()
        .checked_add(usize::from(common_buffer.is_some()))
        .ok_or_else(|| invalid_input("chunk count overflow"))?;
    let chunk_count_u16 = checked_u16(chunk_count, "chunk count")?;
    let header_size = calculate_header_size(
        all_strings,
        logical_files,
        chunk_count_u16,
        &options.volume_names,
        has_dz,
    )?;
    sink.open_volume(0, &options.volume_names[0])?
        .seek(SeekFrom::Start(header_size))?;

    // dzip.exe applies `align` to the beginning of each volume's payload
    // region. Chunks within that region remain tightly packed.
    for (id, name) in options.volume_names.iter().enumerate() {
        let id = checked_u16(id, "archive volume index")?;
        pad_writer_to_alignment(sink.open_volume(id, name)?, options.alignment)?;
    }

    let mut chunks = vec![
        Chunk {
            offset: 0,
            compressed_length: 0,
            decompressed_length: 0,
            flags: 0,
            file: 0,
        };
        planned.len()
    ];
    let mut logical_chunk_ids = vec![Vec::new(); logical_files.len()];
    for (index, entry) in planned.iter().enumerate() {
        logical_chunk_ids[entry.logical_index].push(checked_u16(index, "chunk index")?);
    }

    let mut write_order: Vec<usize> = (0..planned.len()).collect();
    write_order.sort_by_key(|index| {
        crate::compat::original::compression_write_rank(planned[*index].compression)
    });
    let non_dz = write_order
        .iter()
        .copied()
        .filter(|index| planned[*index].compression != Compression::Dz)
        .collect::<Vec<_>>();
    let mut stored_bytes = 0u64;
    for batch in non_dz.chunks(encoding_batch_size()) {
        for encoded in encode_batch(batch, prepared, planned, options)? {
            input_bytes = input_bytes
                .checked_add(encoded.original_len as u64)
                .ok_or_else(|| invalid_input("input byte count overflow"))?;
            stored_bytes = stored_bytes
                .checked_add(encoded.bytes.len() as u64)
                .ok_or_else(|| invalid_input("stored byte count overflow"))?;
            write_chunk_payload(
                encoded.index,
                encoded.original_len,
                encoded.bytes.as_ref(),
                planned,
                &mut chunks,
                options,
                sink,
            )?;
        }
    }

    if let Some(common) = common_buffer {
        let writer = sink.open_volume(0, &options.volume_names[0])?;
        let offset = checked_u32(writer.stream_position()? as usize, "COMBUF offset")?;
        writer.write_all(common)?;
        stored_bytes = stored_bytes
            .checked_add(common.len() as u64)
            .ok_or_else(|| invalid_input("stored byte count overflow"))?;
        chunks.push(Chunk {
            offset,
            compressed_length: checked_u32(common.len(), "COMBUF compressed length")?,
            decompressed_length: 0,
            flags: CHUNK_COMBUF,
            file: 0,
        });
    }

    for &index in write_order
        .iter()
        .filter(|index| planned[**index].compression == Compression::Dz)
    {
        let dz_index = planned[index]
            .dz_index
            .ok_or_else(|| invalid_input("missing archive-scoped DZ index"))?;
        let encoded = encoded_dz
            .as_ref()
            .and_then(|archive| archive.chunks.get(dz_index))
            .ok_or_else(|| invalid_input("missing archive-scoped DZ result"))?;
        stored_bytes = stored_bytes
            .checked_add(encoded.len() as u64)
            .ok_or_else(|| invalid_input("stored byte count overflow"))?;
        write_chunk_payload(
            index,
            original_lengths[index]
                .ok_or_else(|| invalid_input("missing archive-scoped DZ input length"))?,
            encoded,
            planned,
            &mut chunks,
            options,
            sink,
        )?;
    }

    let chunk_map: Vec<_> = logical_chunk_ids
        .into_iter()
        .enumerate()
        .map(|(index, chunks)| (file_directory_ids[index], chunks))
        .collect();
    let main = sink.open_volume(0, &options.volume_names[0])?;
    main.seek(SeekFrom::Start(0))?;
    let mut writer = DzipWriter::new(main);
    writer.write_archive_settings(&ArchiveSettings {
        header: 0x5A52_5444,
        num_user_files: file_count,
        num_directories: directory_count,
        version: 0,
    })?;
    writer.write_strings(all_strings)?;
    writer.write_file_chunk_map(&chunk_map)?;
    writer.write_chunk_settings(&ChunkSettings {
        num_archive_files: volume_count,
        num_chunks: chunk_count_u16,
    })?;
    writer.write_chunks(&chunks)?;
    writer.write_strings(&options.volume_names[1..])?;
    if has_dz {
        writer.write_global_settings(&options.dz.settings)?;
    }

    Ok(BuildReport {
        entries: logical_files.len(),
        chunks: chunk_count,
        input_bytes,
        stored_bytes,
        volumes: options.volume_names.len(),
    })
}

fn encoding_batch_size() -> usize {
    #[cfg(feature = "parallel")]
    {
        rayon::current_num_threads().max(1)
    }
    #[cfg(not(feature = "parallel"))]
    {
        1
    }
}

fn encode_batch<'a>(
    indices: &[usize],
    prepared: &'a [PreparedEntry<'a>],
    planned: &[PlannedEntry],
    options: &PackOptions,
) -> Result<Vec<EncodedEntry<'a>>> {
    let encode = |index: &usize| -> Result<EncodedEntry<'a>> {
        let index = *index;
        let compression = planned[index].compression;
        if compression == Compression::Dz {
            return Err(invalid_input(
                "DZ cannot be encoded as an independent chunk",
            ));
        }
        let source = prepared[index].source.load()?;
        let original_len = source.len();
        let bytes = if compression == Compression::Copy {
            source
        } else {
            Cow::Owned(crate::codec::encode(
                compression,
                source.as_ref(),
                options.dz.settings,
            )?)
        };
        Ok(EncodedEntry {
            index,
            original_len,
            bytes,
        })
    };

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        indices.par_iter().map(encode).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        indices.iter().map(encode).collect()
    }
}

fn write_chunk_payload<S: VolumeSink>(
    index: usize,
    original_len: usize,
    encoded: &[u8],
    planned: &[PlannedEntry],
    chunks: &mut [Chunk],
    options: &PackOptions,
    sink: &mut S,
) -> Result<()> {
    let entry = &planned[index];
    let name = options
        .volume_names
        .get(entry.volume as usize)
        .ok_or(DzipError::VolumeNotFound(entry.volume))?;
    let writer = sink.open_volume(entry.volume, name)?;
    let offset_u64 = writer.stream_position()?;
    let offset = u32::try_from(offset_u64)
        .map_err(|_| invalid_input("chunk offset exceeds the 32-bit format field"))?;
    writer.write_all(encoded)?;
    let compressed_length =
        crate::compat::original::stored_length(entry.compression, original_len, encoded.len());
    chunks[index] = Chunk {
        offset,
        compressed_length: checked_u32(compressed_length, "chunk compressed length")?,
        decompressed_length: checked_u32(original_len, "chunk decompressed length")?,
        flags: entry.flags,
        file: entry.volume,
    };
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(invalid_input("archive path must not be empty"));
    }
    let path = path
        .to_str()
        .ok_or_else(|| invalid_input(format!("archive path is not UTF-8: {}", path.display())))?;
    if path.as_bytes().contains(&0) {
        return Err(invalid_input("archive path contains an embedded nul byte"));
    }
    let normalized = resolve_relative_path(path)?;
    if normalized == Path::new(".") {
        return Err(invalid_input(
            "archive path must not resolve to the root directory",
        ));
    }
    Ok(normalized)
}

fn validate_volume_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(invalid_input("archive volume name must not be empty"));
    }
    if name.as_bytes().contains(&0) {
        return Err(invalid_input(
            "archive volume name contains an embedded nul byte",
        ));
    }
    resolve_relative_path(name)?;
    Ok(())
}

fn pad_writer_to_alignment(writer: &mut dyn WriteSeek, alignment: u32) -> std::io::Result<()> {
    if alignment <= 1 {
        return Ok(());
    }
    let position = writer.stream_position()?;
    let alignment = u64::from(alignment);
    let padding = (alignment - position % alignment) % alignment;
    if padding != 0 {
        writer.write_all(&vec![0; padding as usize])?;
    }
    Ok(())
}

fn checked_u16(value: usize, label: &'static str) -> Result<u16> {
    u16::try_from(value).map_err(|_| invalid_input(format!("{label} exceeds 65535")))
}

fn checked_u32(value: usize, label: &'static str) -> Result<u32> {
    u32::try_from(value).map_err(|_| invalid_input(format!("{label} exceeds 32-bit limit")))
}

fn source_error(path: &Path, error: std::io::Error) -> DzipError {
    DzipError::Io(std::io::Error::new(
        error.kind(),
        format!("{}: {error}", path.display()),
    ))
}

fn invalid_input(message: impl Into<String>) -> DzipError {
    DzipError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message.into(),
    ))
}
