//! Seek-based BNK access for large banks.
//!
//! [`SoundBankReader`] scans only chunk headers up front. Callers can then
//! decode selected chunks or copy one embedded WEM without allocating the
//! complete DATA chunk.

use std::io::{Read, Seek, SeekFrom, Write};

use byteorder::{ByteOrder, LittleEndian};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::binary::{parse_chunk, parse_header, validate_media_pairs, validate_twinning_chunks};
use crate::error::{BnkError, Result};
use crate::limits::{DecodeOptions, ValidationMode};
use crate::types::{BankChunk, BankHeader, ChunkId, MediaIndexEntry, SoundBank};
use crate::version::BankVersion;

/// Location of one chunk body in a seekable BNK stream.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLocation {
    pub id: ChunkId,
    pub header_offset: u64,
    pub data_offset: u64,
    pub size: u32,
}

/// Lazy reader for large BNK files.
pub struct SoundBankReader<R> {
    source: R,
    header: BankHeader,
    chunks: Vec<ChunkLocation>,
    options: DecodeOptions,
}

impl<R: Read + Seek> SoundBankReader<R> {
    /// Scan a seekable BNK using default strict validation.
    pub fn new(source: R) -> Result<Self> {
        Self::with_options(source, DecodeOptions::default())
    }

    /// Scan a seekable BNK without loading chunk bodies other than BKHD.
    pub fn with_options(mut source: R, options: DecodeOptions) -> Result<Self> {
        let file_size = source
            .seek(SeekFrom::End(0))
            .map_err(|error| BnkError::io_at(0, error))?;
        if file_size > options.limits.max_file_bytes {
            return Err(BnkError::LimitExceeded {
                resource: "file bytes",
                requested: file_size,
                limit: options.limits.max_file_bytes,
            });
        }
        source
            .seek(SeekFrom::Start(0))
            .map_err(|error| BnkError::io_at(0, error))?;

        let mut chunk_header = [0_u8; 8];
        read_exact_at(&mut source, 0, &mut chunk_header)?;
        let id = ChunkId(chunk_header[..4].try_into().expect("four-byte chunk id"));
        if id != ChunkId::BKHD {
            return Err(BnkError::InvalidMagic {
                offset: 0,
                found: id.0,
            });
        }
        let header_size = LittleEndian::read_u32(&chunk_header[4..]) as usize;
        check_chunk_size(header_size, options)?;
        ensure_range(file_size, 8, header_size as u64, "BKHD chunk")?;
        let mut header_bytes = vec![0_u8; header_size];
        read_exact_at(&mut source, 8, &mut header_bytes)?;
        let header = parse_header(&header_bytes, 8)?;
        if options.validation.is_strict() {
            header.version.ensure_supported()?;
        }

        let mut chunks = Vec::new();
        let mut position = 8_u64 + header_size as u64;
        while position < file_size {
            if file_size - position < 8 {
                return Err(BnkError::Truncated {
                    context: "chunk header",
                    offset: position,
                    needed: 8,
                    remaining: (file_size - position) as usize,
                });
            }
            if chunks.len() >= options.limits.max_chunks {
                return Err(BnkError::LimitExceeded {
                    resource: "chunk count",
                    requested: chunks.len() as u64 + 1,
                    limit: options.limits.max_chunks as u64,
                });
            }
            read_exact_at(&mut source, position, &mut chunk_header)?;
            let id = ChunkId(chunk_header[..4].try_into().expect("four-byte chunk id"));
            let size = LittleEndian::read_u32(&chunk_header[4..]);
            check_chunk_size(size as usize, options)?;
            let data_offset = position + 8;
            ensure_range(file_size, data_offset, size.into(), "chunk body")?;
            chunks.push(ChunkLocation {
                id,
                header_offset: position,
                data_offset,
                size,
            });
            position = data_offset + u64::from(size);
        }
        validate_chunk_locations(header.version, &chunks, options.validation)?;

        Ok(Self {
            source,
            header,
            chunks,
            options,
        })
    }

    pub fn header(&self) -> &BankHeader {
        &self.header
    }

    pub fn chunk_locations(&self) -> &[ChunkLocation] {
        &self.chunks
    }

    /// Decode a selected chunk. DATA remains owned by the returned chunk, so
    /// prefer [`Self::copy_embedded_media`] when only one WEM is needed.
    pub fn read_chunk(&mut self, index: usize) -> Result<BankChunk> {
        let location = *self.chunks.get(index).ok_or_else(|| {
            BnkError::invalid(
                "chunk index",
                0,
                format!("index {index} is outside {} chunks", self.chunks.len()),
            )
        })?;
        let mut bytes = vec![0_u8; location.size as usize];
        read_exact_at(&mut self.source, location.data_offset, &mut bytes)?;
        parse_chunk(
            location.id,
            &bytes,
            location.data_offset as usize,
            self.header.version,
            self.options,
        )
    }

    /// Load every chunk into the existing owned [`SoundBank`] model.
    pub fn read_sound_bank(&mut self) -> Result<SoundBank> {
        let mut chunks = Vec::with_capacity(self.chunks.len());
        for index in 0..self.chunks.len() {
            chunks.push(self.read_chunk(index)?);
        }
        if self.options.validation.is_strict() {
            validate_media_pairs(&chunks)?;
        }
        if self.options.validation.is_twinning_compatible() {
            validate_twinning_chunks(self.header.version, &chunks)?;
        }
        Ok(SoundBank {
            header: self.header.clone(),
            chunks,
        })
    }

    /// Read one embedded WEM without materializing the complete DATA chunk.
    pub fn read_embedded_media(&mut self, id: u32) -> Result<Option<Vec<u8>>> {
        let Some((entry, data)) = self.find_media(id)? else {
            return Ok(None);
        };
        let mut bytes = vec![0_u8; entry.size as usize];
        read_exact_at(
            &mut self.source,
            data.data_offset + u64::from(entry.offset),
            &mut bytes,
        )?;
        Ok(Some(bytes))
    }

    /// Stream one embedded WEM to a writer without allocating its payload.
    pub fn copy_embedded_media(&mut self, id: u32, mut output: impl Write) -> Result<bool> {
        let Some((entry, data)) = self.find_media(id)? else {
            return Ok(false);
        };
        let offset = data.data_offset + u64::from(entry.offset);
        self.source
            .seek(SeekFrom::Start(offset))
            .map_err(|error| BnkError::io_at(offset, error))?;
        let mut remaining = u64::from(entry.size);
        let mut buffer = [0_u8; 64 * 1024];
        while remaining != 0 {
            let amount = usize::try_from(remaining.min(buffer.len() as u64))
                .expect("bounded by buffer length");
            self.source
                .read_exact(&mut buffer[..amount])
                .map_err(|error| {
                    BnkError::io_at(offset + u64::from(entry.size) - remaining, error)
                })?;
            output
                .write_all(&buffer[..amount])
                .map_err(|error| BnkError::io_at(0, error))?;
            remaining -= amount as u64;
        }
        Ok(true)
    }

    pub fn into_inner(self) -> R {
        self.source
    }

    fn find_media(&mut self, id: u32) -> Result<Option<(MediaIndexEntry, ChunkLocation)>> {
        for index in 0..self.chunks.len() {
            if self.chunks[index].id != ChunkId::DIDX {
                continue;
            }
            let Some(data) = self.chunks.get(index + 1).copied() else {
                return Err(BnkError::MissingChunk {
                    chunk: ChunkId::DATA.0,
                    context: "DIDX must be immediately followed by DATA",
                });
            };
            if data.id != ChunkId::DATA {
                return Err(BnkError::MissingChunk {
                    chunk: ChunkId::DATA.0,
                    context: "DIDX must be immediately followed by DATA",
                });
            }
            let BankChunk::MediaIndex(entries) = self.read_chunk(index)? else {
                unreachable!("DIDX location must decode as MediaIndex");
            };
            let Some(entry) = entries.into_iter().find(|entry| entry.id == id) else {
                continue;
            };
            validate_media_range(entry, data)?;
            return Ok(Some((entry, data)));
        }
        Ok(None)
    }
}

fn validate_chunk_locations(
    version: BankVersion,
    chunks: &[ChunkLocation],
    validation: ValidationMode,
) -> Result<()> {
    if !validation.is_strict() {
        return Ok(());
    }
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.id == ChunkId::DIDX
            && chunks.get(index + 1).map(|next| next.id) != Some(ChunkId::DATA)
        {
            return Err(BnkError::MissingChunk {
                chunk: ChunkId::DATA.0,
                context: "DIDX must be immediately followed by DATA",
            });
        }
    }
    if !validation.is_twinning_compatible() {
        return Ok(());
    }
    crate::container::validate_twinning_chunk_ids(version, chunks.iter().map(|chunk| chunk.id))
}

fn read_exact_at(reader: &mut (impl Read + Seek), offset: u64, bytes: &mut [u8]) -> Result<()> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| BnkError::io_at(offset, error))?;
    reader
        .read_exact(bytes)
        .map_err(|error| BnkError::io_at(offset, error))
}

fn check_chunk_size(size: usize, options: DecodeOptions) -> Result<()> {
    if size as u64 > options.limits.max_chunk_bytes {
        Err(BnkError::LimitExceeded {
            resource: "chunk bytes",
            requested: size as u64,
            limit: options.limits.max_chunk_bytes,
        })
    } else {
        Ok(())
    }
}

fn ensure_range(file_size: u64, offset: u64, size: u64, context: &'static str) -> Result<()> {
    let end = offset
        .checked_add(size)
        .ok_or(BnkError::IntegerOverflow { context })?;
    if end > file_size {
        Err(BnkError::Truncated {
            context,
            offset,
            needed: size as usize,
            remaining: file_size.saturating_sub(offset) as usize,
        })
    } else {
        Ok(())
    }
}

fn validate_media_range(entry: MediaIndexEntry, data: ChunkLocation) -> Result<()> {
    if entry.id == 0 {
        if entry.offset == 1 && entry.size == 0 {
            return Ok(());
        }
        return Err(BnkError::invalid(
            "DIDX reserved media entry",
            data.data_offset as usize,
            format!(
                "identifier zero requires offset 1 and size 0, found offset {} and size {}",
                entry.offset, entry.size
            ),
        ));
    }
    let end = entry
        .offset
        .checked_add(entry.size)
        .ok_or(BnkError::IntegerOverflow {
            context: "DIDX media range",
        })?;
    if end > data.size {
        Err(BnkError::MediaOutOfBounds {
            id: entry.id,
            offset: entry.offset,
            size: entry.size,
            data_len: data.size as usize,
        })
    } else {
        Ok(())
    }
}
