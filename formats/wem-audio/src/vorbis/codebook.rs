//! Vorbis codebook library for rebuilding Wwise audio.
//!
//! Wwise audio files use a stripped Vorbis format that references external codebook
//! libraries instead of embedding the full codebook data. This module loads and provides
//! access to these codebook libraries.
//!
//! # Codebook Selection
//!
//! Different games use different codebook libraries. If conversion produces garbled
//! audio or fails with size mismatch errors, try a different codebook:
//!
//! - [`CodebookLibrary::standard()`] - Standard Vorbis codebooks
//! - [`CodebookLibrary::aotuv_603()`] - aoTuV 6.03 tuned codebooks
//! - [`CodebookLibrary::from_file()`] - Load custom codebooks from a file
//!
//! # Example
//!
//! ```no_run
//! use wem_audio::CodebookLibrary;
//!
//! let codebooks = CodebookLibrary::standard();
//!
//! // PvZ2 commonly uses the aoTuV 6.03 set.
//! let codebooks = CodebookLibrary::aotuv_603();
//!
//! // Or load from a custom file
//! let codebooks = CodebookLibrary::from_file("custom_codebooks.bin")?;
//! # Ok::<(), wem_audio::WemError>(())
//! ```

use crate::bit_stream::BitWriter;
use crate::bit_stream::{BitRead, BitSliceReader};
use crate::error::{WemError, WemResult};
use crate::vorbis::helpers::{book_map_type1_quantvals, ilog};
use std::path::Path;
use std::sync::Arc;

/// Packed Vorbis codebooks used to restore stripped Wwise setup packets.
#[derive(Clone)]
pub struct CodebookLibrary {
    storage: CodebookStorage,
}

#[derive(Clone)]
enum CodebookStorage {
    Static(&'static [&'static [u8]]),
    Packed {
        data: Arc<[u8]>,
        offsets: Arc<[usize]>,
    },
}

impl CodebookLibrary {
    /// Returns the embedded standard codebook set without allocating.
    pub const fn standard() -> Self {
        use crate::vorbis::embedded_codebooks::standard::CODEBOOKS;
        Self {
            storage: CodebookStorage::Static(CODEBOOKS),
        }
    }

    /// Returns the embedded aoTuV 6.03 codebook set without allocating.
    pub const fn aotuv_603() -> Self {
        use crate::vorbis::embedded_codebooks::aotuv603::CODEBOOKS;
        Self {
            storage: CodebookStorage::Static(CODEBOOKS),
        }
    }

    /// Load codebooks from a file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> WemResult<Self> {
        let data = std::fs::read(path).map_err(WemError::Io)?;
        Self::from_bytes(&data)
    }

    /// Load codebooks from a byte slice.
    pub fn from_bytes(data: &[u8]) -> WemResult<Self> {
        if data.len() < 4 {
            return Err(WemError::parse("codebook library too short"));
        }

        let len = data.len();
        // Offset to the offset table is in the last 4 bytes
        let table_offset_bytes = [data[len - 4], data[len - 3], data[len - 2], data[len - 1]];
        let table_offset = u32::from_le_bytes(table_offset_bytes) as usize;

        if table_offset >= len {
            return Err(WemError::parse("invalid codebook library offset table"));
        }

        // Table continues until the offset-to-table (last 4 bytes)
        let table_len = len - 4 - table_offset;

        if !table_len.is_multiple_of(4) {
            return Err(WemError::parse("invalid codebook library table size"));
        }

        let count = table_len / 4;
        let mut offsets = Vec::with_capacity(count);
        let table_bytes = &data[table_offset..len - 4];

        for i in 0..count {
            let entry = i * 4;
            let entry_bytes = [
                table_bytes[entry],
                table_bytes[entry + 1],
                table_bytes[entry + 2],
                table_bytes[entry + 3],
            ];
            let offset = u32::from_le_bytes(entry_bytes) as usize;

            if offset > table_offset {
                return Err(WemError::parse("invalid codebook offset"));
            }
            offsets.push(offset);
        }

        validate_offsets(&offsets, table_offset)?;
        Ok(Self {
            storage: CodebookStorage::Packed {
                data: Arc::from(&data[..table_offset]),
                offsets: offsets.into(),
            },
        })
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            CodebookStorage::Static(codebooks) => codebooks.len(),
            CodebookStorage::Packed { offsets, .. } => offsets.len().saturating_sub(1),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_codebook(&self, index: usize) -> WemResult<&[u8]> {
        match &self.storage {
            CodebookStorage::Static(codebooks) => codebooks
                .get(index)
                .copied()
                .ok_or_else(|| invalid_codebook_index(index)),
            CodebookStorage::Packed { data, offsets } => {
                if index >= offsets.len().saturating_sub(1) {
                    return Err(invalid_codebook_index(index));
                }
                let start = offsets[index];
                let end = offsets[index + 1];
                data.get(start..end)
                    .ok_or_else(|| WemError::codebook("invalid packed codebook range"))
            }
        }
    }

    pub fn codebook_size(&self, index: usize) -> WemResult<usize> {
        Ok(self.get_codebook(index)?.len())
    }

    /// Rebuild a codebook from the library by index and write to output.
    pub(crate) fn rebuild(&self, index: usize, output: &mut BitWriter) -> WemResult<()> {
        let codebook = self.get_codebook(index)?;
        let mut reader = BitSliceReader::new(codebook);
        self.rebuild_internal(&mut reader, Some(codebook.len() as u32), output)
    }

    /// Rebuild a codebook from stripped format using a bit reader.
    ///
    /// This is used for inline codebooks that are not in the library.
    pub(crate) fn rebuild_from_reader<B: BitRead>(
        &self,
        input: &mut B,
        output: &mut BitWriter,
    ) -> WemResult<()> {
        self.rebuild_internal(input, None, output)
    }

    /// Internal rebuild method with optional size validation.
    fn rebuild_internal<B: BitRead>(
        &self,
        input: &mut B,
        codebook_size: Option<u32>,
        output: &mut BitWriter,
    ) -> WemResult<()> {
        // IN: 4 bit dimensions, 14 bit entry count
        let dimensions = input.read_bits(4)?;
        let entries = input.read_bits(14)?;

        // OUT: 24 bit identifier, 16 bit dimensions, 24 bit entry count
        output.write_bits(0x564342, 24); // "BCV"
        output.write_bits(dimensions, 16);
        output.write_bits(entries, 24);

        self.rebuild_codebook_data(input, output, entries, dimensions, codebook_size)
    }

    fn rebuild_codebook_data<B: BitRead>(
        &self,
        input: &mut B,
        output: &mut BitWriter,
        entries: u32,
        dimensions: u32,
        codebook_size: Option<u32>,
    ) -> WemResult<()> {
        // IN/OUT: 1 bit ordered flag
        let ordered = input.read_bits(1)?;
        output.write_bits(ordered, 1);

        if ordered != 0 {
            let initial_length = input.read_bits(5)?;
            output.write_bits(initial_length, 5);

            let mut current_entry = 0u32;
            while current_entry < entries {
                let num_bits = ilog(entries - current_entry);
                let number = input.read_bits(num_bits)?;
                output.write_bits(number, num_bits);
                current_entry += number;
            }

            if current_entry > entries {
                return Err(WemError::parse("current_entry out of range"));
            }
        } else {
            // IN: 3 bit codeword length length, 1 bit sparse flag
            let codeword_length_length = input.read_bits(3)?;
            let sparse = input.read_bits(1)?;

            if codeword_length_length == 0 || codeword_length_length > 5 {
                return Err(WemError::parse("nonsense codeword length"));
            }

            // OUT: 1 bit sparse flag
            output.write_bits(sparse, 1);

            for _ in 0..entries {
                let mut present_bool = true;

                if sparse != 0 {
                    let present = input.read_bits(1)?;
                    output.write_bits(present, 1);
                    present_bool = present != 0;
                }

                if present_bool {
                    // IN: n bit codeword length-1
                    let codeword_length = input.read_bits(codeword_length_length as u8)?;
                    // OUT: 5 bit codeword length-1
                    output.write_bits(codeword_length, 5);
                }
            }
        }

        // Lookup table
        // IN: 1 bit lookup type
        let lookup_type = input.read_bits(1)?;
        // OUT: 4 bit lookup type
        output.write_bits(lookup_type, 4);

        self.handle_lookup_table(input, output, entries, dimensions, lookup_type, true)?;

        // Check size if specified
        if let Some(size) = codebook_size
            && size != 0
        {
            let bytes_read = input.total_bits_read() / 8 + 1;
            if bytes_read != size as u64 {
                return Err(WemError::size_mismatch(size as u64, bytes_read));
            }
        }

        Ok(())
    }

    fn handle_lookup_table<B: BitRead>(
        &self,
        input: &mut B,
        output: &mut BitWriter,
        entries: u32,
        dimensions: u32,
        lookup_type: u32,
        is_rebuild: bool,
    ) -> WemResult<()> {
        if lookup_type == 1 {
            let min = input.read_bits(32)?;
            let max = input.read_bits(32)?;
            let value_length = input.read_bits(4)?;
            let sequence_flag = input.read_bits(1)?;
            output.write_bits(min, 32);
            output.write_bits(max, 32);
            output.write_bits(value_length, 4);
            output.write_bits(sequence_flag, 1);

            let quantvals = book_map_type1_quantvals(entries, dimensions);
            for _ in 0..quantvals {
                let val = input.read_bits((value_length + 1) as u8)?;
                output.write_bits(val, (value_length + 1) as u8);
            }
        } else if lookup_type == 2 {
            if !is_rebuild {
                return Err(WemError::parse("didn't expect lookup type 2"));
            } else {
                return Err(WemError::parse("invalid lookup type"));
            }
        } else if lookup_type != 0 {
            return Err(WemError::parse("invalid lookup type"));
        }

        Ok(())
    }
}

fn validate_offsets(offsets: &[usize], data_len: usize) -> WemResult<()> {
    if offsets.is_empty() {
        if data_len == 0 {
            return Ok(());
        }
        return Err(WemError::codebook(
            "packed codebook table has no end offset",
        ));
    }
    if offsets[0] != 0 {
        return Err(WemError::codebook(
            "first packed codebook offset is not zero",
        ));
    }
    if offsets.last().copied() != Some(data_len) {
        return Err(WemError::codebook(
            "last packed codebook offset does not match data length",
        ));
    }
    if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(WemError::codebook(
            "packed codebook offsets are not monotonic",
        ));
    }
    Ok(())
}

fn invalid_codebook_index(index: usize) -> WemError {
    let id = i32::try_from(index).unwrap_or(i32::MAX);
    WemError::invalid_codebook_id(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sets_are_non_empty_and_zero_copy_when_cloned() {
        let standard = CodebookLibrary::standard();
        let cloned = standard.clone();
        let aotuv = CodebookLibrary::aotuv_603();
        assert!(!standard.is_empty());
        assert_eq!(standard.len(), aotuv.len());
        assert!(std::ptr::eq(
            standard.get_codebook(0).unwrap().as_ptr(),
            cloned.get_codebook(0).unwrap().as_ptr()
        ));
        assert!((0..standard.len()).any(
            |index| standard.get_codebook(index).unwrap() != aotuv.get_codebook(index).unwrap()
        ));
    }

    #[test]
    fn lookup_uses_results_instead_of_sentinel_sizes() {
        let standard = CodebookLibrary::standard();
        assert_eq!(
            standard.codebook_size(0).unwrap(),
            standard.get_codebook(0).unwrap().len()
        );
        assert!(standard.get_codebook(standard.len()).is_err());
        assert!(standard.codebook_size(usize::MAX).is_err());
    }

    #[test]
    fn rejects_malformed_packed_tables() {
        assert!(CodebookLibrary::from_bytes(&[0, 1, 2]).is_err());
        let mut data = vec![0u8; 8];
        data[4] = 100;
        assert!(CodebookLibrary::from_bytes(&data).is_err());
    }
}
