//! Optional lookup indexes for editor and inspection workloads.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{BnkError, Result};
use crate::types::{BankChunk, ChunkId, EmbeddedMedia, HierarchyObject, Identifier, SoundBank};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HierarchyObjectLocation {
    pub chunk_index: usize,
    pub object_index: usize,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedMediaLocation {
    pub index_chunk: usize,
    pub data_chunk: usize,
    pub entry_index: usize,
}

/// Rebuildable indexes that keep the serialized [`SoundBank`] model compact.
#[derive(Debug, Clone, Default)]
pub struct SoundBankIndex {
    chunk_positions: HashMap<ChunkId, Vec<usize>>,
    hierarchy_objects: HashMap<Identifier, Vec<HierarchyObjectLocation>>,
    embedded_media: HashMap<Identifier, Vec<EmbeddedMediaLocation>>,
}

impl SoundBankIndex {
    pub fn new(bank: &SoundBank) -> Self {
        let mut index = Self::default();
        for (chunk_index, chunk) in bank.chunks.iter().enumerate() {
            index
                .chunk_positions
                .entry(chunk.id())
                .or_default()
                .push(chunk_index);
            match chunk {
                BankChunk::Hierarchy(objects) => {
                    for (object_index, object) in objects.iter().enumerate() {
                        index.hierarchy_objects.entry(object.id).or_default().push(
                            HierarchyObjectLocation {
                                chunk_index,
                                object_index,
                            },
                        );
                    }
                }
                BankChunk::MediaIndex(entries)
                    if matches!(
                        bank.chunks.get(chunk_index + 1),
                        Some(BankChunk::MediaData(_))
                    ) =>
                {
                    for (entry_index, entry) in entries.iter().enumerate() {
                        index.embedded_media.entry(entry.id).or_default().push(
                            EmbeddedMediaLocation {
                                index_chunk: chunk_index,
                                data_chunk: chunk_index + 1,
                                entry_index,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        index
    }

    pub fn chunk_positions(&self, id: ChunkId) -> &[usize] {
        self.chunk_positions
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn hierarchy_locations(&self, id: Identifier) -> &[HierarchyObjectLocation] {
        self.hierarchy_objects
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn hierarchy_object<'a>(
        &self,
        bank: &'a SoundBank,
        id: Identifier,
    ) -> Option<&'a HierarchyObject> {
        let location = self.hierarchy_locations(id).first()?;
        let BankChunk::Hierarchy(objects) = bank.chunks.get(location.chunk_index)? else {
            return None;
        };
        objects.get(location.object_index)
    }

    pub fn embedded_media_locations(&self, id: Identifier) -> &[EmbeddedMediaLocation] {
        self.embedded_media
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn embedded_media<'a>(
        &self,
        bank: &'a SoundBank,
        id: Identifier,
    ) -> Result<Option<EmbeddedMedia<'a>>> {
        let Some(location) = self.embedded_media_locations(id).first() else {
            return Ok(None);
        };
        let BankChunk::MediaIndex(entries) = bank
            .chunks
            .get(location.index_chunk)
            .ok_or_else(|| BnkError::invalid("media index", 0, "indexed DIDX chunk is missing"))?
        else {
            return Err(BnkError::invalid(
                "media index",
                0,
                "indexed chunk is no longer DIDX; rebuild SoundBankIndex after mutations",
            ));
        };
        let entry = entries.get(location.entry_index).ok_or_else(|| {
            BnkError::invalid(
                "media index",
                0,
                "indexed DIDX entry is missing; rebuild SoundBankIndex after mutations",
            )
        })?;
        let BankChunk::MediaData(data) =
            bank.chunks
                .get(location.data_chunk)
                .ok_or(BnkError::MissingChunk {
                    chunk: ChunkId::DATA.0,
                    context: "indexed DIDX entry",
                })?
        else {
            return Err(BnkError::invalid(
                "media index",
                0,
                "indexed chunk is no longer DATA; rebuild SoundBankIndex after mutations",
            ));
        };
        if entry.id == 0 && entry.offset == 1 && entry.size == 0 {
            return Ok(Some(EmbeddedMedia {
                id: entry.id,
                data: &[],
            }));
        }
        let begin = entry.offset as usize;
        let end = begin
            .checked_add(entry.size as usize)
            .ok_or(BnkError::IntegerOverflow {
                context: "DIDX media range",
            })?;
        let payload = data.get(begin..end).ok_or(BnkError::MediaOutOfBounds {
            id: entry.id,
            offset: entry.offset,
            size: entry.size,
            data_len: data.len(),
        })?;
        Ok(Some(EmbeddedMedia {
            id: entry.id,
            data: payload,
        }))
    }
}

impl SoundBank {
    pub fn build_index(&self) -> SoundBankIndex {
        SoundBankIndex::new(self)
    }
}
