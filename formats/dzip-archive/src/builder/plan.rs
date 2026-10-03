//! Pure planning for the original dzip.exe archive layout.

use super::{PreparedEntry, checked_u16, invalid_input};
use crate::Result;
use crate::codec::{ChunkEncoding, Compression, ContentHint};
use crate::format::{CHUNK_JPEG, CHUNK_MP3, CHUNK_RANDOMACCESS};
use crate::options::EntryOptions;
use crate::path::{ArchivePathKey, to_archive_format};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) struct LogicalFile {
    pub(super) path: PathBuf,
    pub(super) segment_indices: Vec<usize>,
}

pub(super) struct PlannedEntry {
    pub(super) logical_index: usize,
    pub(super) volume: u16,
    pub(super) compression: Compression,
    pub(super) flags: u16,
    pub(super) dz_index: Option<usize>,
}

pub(super) struct OriginalArchivePlan {
    pub(super) logical_files: Vec<LogicalFile>,
    pub(super) strings: Vec<String>,
    pub(super) file_directory_ids: Vec<u16>,
    pub(super) directory_count: u16,
    pub(super) entries: Vec<PlannedEntry>,
}

impl OriginalArchivePlan {
    pub(super) fn build(prepared: &[PreparedEntry<'_>]) -> Result<Self> {
        let compressions = prepared
            .iter()
            .map(|entry| effective_compression(entry.options))
            .collect::<Result<Vec<_>>>()?;
        let (logical_files, segment_to_logical) = group_logical_files(prepared);
        let (strings, file_directory_ids, directory_count) = build_string_table(&logical_files)?;
        let mut next_dz = 0usize;
        let entries = prepared
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let compression = compressions[index];
                let dz_index = (compression == Compression::Dz).then(|| {
                    let index = next_dz;
                    next_dz += 1;
                    index
                });
                let flags = entry.options.raw_flags.unwrap_or_else(|| {
                    let mut flags = compression.flag();
                    if entry.options.random_access {
                        flags |= CHUNK_RANDOMACCESS;
                    }
                    flags |= match entry.options.content_hint {
                        Some(ContentHint::Mp3) => CHUNK_MP3,
                        Some(ContentHint::Jpeg) => CHUNK_JPEG,
                        Some(ContentHint::Mp3AndJpeg) => CHUNK_MP3 | CHUNK_JPEG,
                        None => 0,
                    };
                    flags
                });
                PlannedEntry {
                    logical_index: segment_to_logical[index],
                    volume: entry.options.volume,
                    compression,
                    flags,
                    dz_index,
                }
            })
            .collect();
        Ok(Self {
            logical_files,
            strings,
            file_directory_ids,
            directory_count,
            entries,
        })
    }
}

pub(super) fn effective_compression(options: EntryOptions) -> Result<Compression> {
    match options.raw_flags {
        Some(flags) => Ok(ChunkEncoding::from_packer_flags(flags)?.compression),
        None => Ok(options.compression),
    }
}

pub(super) fn group_logical_files(
    prepared: &[PreparedEntry<'_>],
) -> (Vec<LogicalFile>, Vec<usize>) {
    let mut logical_files = Vec::<LogicalFile>::new();
    let mut lookup = HashMap::new();
    let mut segment_to_logical = Vec::with_capacity(prepared.len());
    for (segment_index, entry) in prepared.iter().enumerate() {
        let key = ArchivePathKey::from_path(&entry.archive_path);
        let logical_index = if let Some(index) = lookup.get(&key).copied() {
            index
        } else {
            let index = logical_files.len();
            logical_files.push(LogicalFile {
                path: entry.archive_path.clone(),
                segment_indices: Vec::new(),
            });
            lookup.insert(key, index);
            index
        };
        logical_files[logical_index]
            .segment_indices
            .push(segment_index);
        segment_to_logical.push(logical_index);
    }
    (logical_files, segment_to_logical)
}

pub(super) fn build_string_table(logical: &[LogicalFile]) -> Result<(Vec<String>, Vec<u16>, u16)> {
    let mut file_names = Vec::with_capacity(logical.len());
    let mut directories = Vec::new();
    let mut directory_lookup = HashMap::new();
    let mut directory_ids = Vec::with_capacity(logical.len());
    for file in logical {
        let file_name = file
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid_input(format!("invalid UTF-8 path {}", file.path.display())))?;
        file_names.push(file_name.to_string());
        let parent = file.path.parent().unwrap_or_else(|| Path::new(""));
        let parent = to_archive_format(parent);
        if parent.is_empty() || parent == "." {
            directory_ids.push(0);
        } else {
            let key = ArchivePathKey::from_archive_str(&parent);
            let id = if let Some(id) = directory_lookup.get(&key).copied() {
                id
            } else {
                directories.push(parent);
                let id = checked_u16(directories.len(), "directory count")?;
                directory_lookup.insert(key, id);
                id
            };
            directory_ids.push(id);
        }
    }
    let directory_count = checked_u16(
        directories
            .len()
            .checked_add(1)
            .ok_or_else(|| invalid_input("directory count overflow"))?,
        "directory count",
    )?;
    file_names.extend(directories);
    Ok((file_names, directory_ids, directory_count))
}

pub(super) fn calculate_header_size(
    strings: &[String],
    logical: &[LogicalFile],
    chunk_count: u16,
    volume_names: &[String],
    has_dz: bool,
) -> Result<u64> {
    let mut size = 9u64;
    for string in strings {
        size = size
            .checked_add(string.len() as u64 + 1)
            .ok_or_else(|| invalid_input("header size overflow"))?;
    }
    for file in logical {
        size = size
            .checked_add(4 + file.segment_indices.len() as u64 * 2)
            .ok_or_else(|| invalid_input("header size overflow"))?;
    }
    size = size
        .checked_add(4 + u64::from(chunk_count) * 16)
        .ok_or_else(|| invalid_input("header size overflow"))?;
    for name in &volume_names[1..] {
        size = size
            .checked_add(name.len() as u64 + 1)
            .ok_or_else(|| invalid_input("header size overflow"))?;
    }
    if has_dz {
        size = size
            .checked_add(10)
            .ok_or_else(|| invalid_input("header size overflow"))?;
    }
    Ok(size)
}
