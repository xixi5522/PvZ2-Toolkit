use super::ReadOptions;
use crate::format::{ArchiveString, Chunk, RawArchive, RawFileRecord};
use crate::reader::DzipReader;
use crate::{DzipError, Result};
use std::io::{Read, Seek};

pub(super) fn parse_metadata<R: Read + Seek>(
    reader: &mut DzipReader<R>,
    options: &ReadOptions,
) -> Result<RawArchive> {
    let settings = reader.read_archive_settings()?;
    check_limit(
        "entry count",
        options.limits.max_entries as u64,
        settings.num_user_files as u64,
    )?;
    let strings_count = usize::from(settings.num_user_files)
        .checked_add(usize::from(settings.num_directories))
        .and_then(|count| count.checked_sub(1))
        .ok_or_else(|| invalid_archive("invalid metadata string count"))?;
    let strings = reader.read_raw_strings_with_limits(
        strings_count,
        options.limits.max_string_length,
        options.limits.max_metadata_bytes,
    )?;
    let files = reader
        .read_file_chunk_map_with_limits(
            settings.num_user_files as usize,
            options.limits.max_chunks,
            options.limits.max_chunk_references,
        )?
        .into_iter()
        .map(|(directory_id, chunk_ids)| RawFileRecord {
            directory_id,
            chunk_ids,
        })
        .collect::<Vec<_>>();
    let chunk_settings = reader.read_chunk_settings()?;
    check_limit(
        "chunk count",
        options.limits.max_chunks as u64,
        chunk_settings.num_chunks as u64,
    )?;
    if chunk_settings.num_archive_files == 0 {
        return Err(invalid_archive("archive declares zero volumes"));
    }
    let chunks = reader.read_chunks(chunk_settings.num_chunks as usize)?;
    let metadata_before_volumes = metadata_size_before_volumes(&strings, &files, &chunks)?;
    check_limit(
        "metadata bytes",
        options.limits.max_metadata_bytes as u64,
        metadata_before_volumes as u64,
    )?;
    let remaining_metadata = options
        .limits
        .max_metadata_bytes
        .saturating_sub(metadata_before_volumes);
    let volume_files = reader.read_raw_file_list_with_limits(
        chunk_settings.num_archive_files.saturating_sub(1) as usize,
        options.limits.max_string_length,
        remaining_metadata,
    )?;
    let range_settings = if chunks
        .iter()
        .any(|chunk| crate::compat::original::flags_require_range_settings(chunk.flags))
    {
        // Keep the raw ten-byte field lossless here. Semantic validation is
        // performed when the high-level archive constructs its DZ context.
        Some(reader.read_global_settings()?)
    } else {
        None
    };
    let volume_name_bytes = volume_files.iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.encoded_len())
            .ok_or_else(|| invalid_archive("metadata size overflow"))
    })?;
    let metadata_bytes = metadata_before_volumes
        .checked_add(volume_name_bytes)
        .and_then(|size| size.checked_add(usize::from(range_settings.is_some()) * 10))
        .ok_or_else(|| invalid_archive("metadata size overflow"))?;
    check_limit(
        "metadata bytes",
        options.limits.max_metadata_bytes as u64,
        metadata_bytes as u64,
    )?;

    Ok(RawArchive {
        settings,
        strings,
        files,
        chunk_settings,
        chunks,
        volume_files,
        range_settings,
    })
}

pub(super) fn decode_archive_strings(strings: &[ArchiveString]) -> Result<Vec<String>> {
    strings
        .iter()
        .map(|value| value.as_str().map(str::to_owned))
        .collect()
}

fn metadata_size_before_volumes(
    strings: &[ArchiveString],
    files: &[RawFileRecord],
    chunks: &[Chunk],
) -> Result<usize> {
    let string_bytes = strings.iter().try_fold(0usize, |total, string| {
        total
            .checked_add(string.encoded_len())
            .ok_or_else(|| invalid_archive("metadata size overflow"))
    })?;
    let map_bytes = files.iter().try_fold(0usize, |total, file| {
        total
            .checked_add(4)
            .and_then(|value| value.checked_add(file.chunk_ids.len().checked_mul(2)?))
            .ok_or_else(|| invalid_archive("metadata size overflow"))
    })?;
    let chunk_bytes = chunks
        .len()
        .checked_mul(16)
        .ok_or_else(|| invalid_archive("metadata size overflow"))?;
    9usize
        .checked_add(string_bytes)
        .and_then(|value| value.checked_add(map_bytes))
        .and_then(|value| value.checked_add(4))
        .and_then(|value| value.checked_add(chunk_bytes))
        .ok_or_else(|| invalid_archive("metadata size overflow"))
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
