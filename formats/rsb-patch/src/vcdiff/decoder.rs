use std::io::{Read, Write};

use crate::{PatchError, Result};

use super::{
    DecodeOptions, EXTENDED_VERSION, MAGIC, STANDARD_VERSION, VCD_CHECKSUM, VCD_CODETABLE,
    VCD_DECOMPRESS, VCD_SOURCE, VCD_TARGET,
    address_cache::{AddressCache, DEFAULT_NEAR_CACHE_SIZE, DEFAULT_SAME_CACHE_COUNT},
    code_table::{
        DecoderCodeTable, InstructionKind, SERIALIZED_CODE_TABLE_SIZE, validate_cache_sizes,
    },
    invalid,
    io::{Cursor, checked_segment},
};

const MAX_CODE_TABLE_DEPTH: usize = 8;

/// Decode a standard or interleaved VCDIFF delta.
pub fn decode(source: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    decode_with_options(source, patch, DecodeOptions::default())
}

/// Decode a VCDIFF delta while enforcing the supplied resource limits.
pub fn decode_with_options(source: &[u8], patch: &[u8], options: DecodeOptions) -> Result<Vec<u8>> {
    let mut patch = Cursor::new(patch);
    let output = decode_file(source, &mut patch, options, None, 0)?;
    patch.finish("VCDIFF file")?;
    Ok(output)
}

fn decode_file(
    source: &[u8],
    patch: &mut Cursor<'_>,
    options: DecodeOptions,
    planned_output_size: Option<usize>,
    code_table_depth: usize,
) -> Result<Vec<u8>> {
    if code_table_depth > MAX_CODE_TABLE_DEPTH {
        return Err(invalid("custom VCDIFF code tables are nested too deeply"));
    }
    if patch.read_exact(3)? != MAGIC {
        return Err(invalid("invalid VCDIFF magic"));
    }
    let version = patch.read_u8()?;
    if !matches!(version, STANDARD_VERSION | EXTENDED_VERSION) {
        return Err(PatchError::UnsupportedVcdiff(format!(
            "version byte 0x{version:02X}"
        )));
    }
    let header_indicator = patch.read_u8()?;
    if header_indicator & VCD_DECOMPRESS != 0 {
        return Err(PatchError::UnsupportedVcdiff(format!(
            "secondary compression in header indicator 0x{header_indicator:02X}"
        )));
    }
    let (table, near_cache_size, same_cache_size) = if header_indicator & VCD_CODETABLE != 0 {
        let near_cache_size = patch.read_varint()?;
        let same_cache_size = patch.read_varint()?;
        validate_cache_sizes(near_cache_size, same_cache_size)?;
        let default_table = DecoderCodeTable::default();
        let serialized_default = default_table.serialize();
        let serialized_custom = decode_file(
            &serialized_default,
            patch,
            DecodeOptions::new(SERIALIZED_CODE_TABLE_SIZE),
            Some(SERIALIZED_CODE_TABLE_SIZE),
            code_table_depth + 1,
        )?;
        (
            DecoderCodeTable::from_serialized(
                &serialized_custom,
                near_cache_size,
                same_cache_size,
            )?,
            near_cache_size,
            same_cache_size,
        )
    } else {
        (
            DecoderCodeTable::default(),
            DEFAULT_NEAR_CACHE_SIZE,
            DEFAULT_SAME_CACHE_COUNT,
        )
    };

    let mut output = Vec::new();
    loop {
        if planned_output_size.is_some_and(|planned| output.len() >= planned) {
            break;
        }
        if patch.remaining() == 0 {
            match planned_output_size {
                Some(planned) => {
                    return Err(invalid(format!(
                        "VCDIFF stream ended after {} of {planned} planned output bytes",
                        output.len()
                    )));
                }
                None => break,
            }
        }
        let window_indicator = patch.read_u8()?;
        let allowed_flags = if version == EXTENDED_VERSION {
            VCD_SOURCE | VCD_TARGET | VCD_CHECKSUM
        } else {
            VCD_SOURCE | VCD_TARGET
        };
        if window_indicator & !allowed_flags != 0 {
            return Err(PatchError::UnsupportedVcdiff(format!(
                "window indicator 0x{window_indicator:02X}"
            )));
        }
        if window_indicator & (VCD_SOURCE | VCD_TARGET) == (VCD_SOURCE | VCD_TARGET) {
            return Err(invalid(
                "a window cannot use VCD_SOURCE and VCD_TARGET together",
            ));
        }

        let (source_length, source_position) = if window_indicator & (VCD_SOURCE | VCD_TARGET) != 0
        {
            (patch.read_varint()?, patch.read_varint()?)
        } else {
            (0, 0)
        };
        let delta_length = patch.read_varint()?;
        let mut delta = patch.take_cursor(delta_length)?;
        let target_length = delta.read_varint()?;
        let total_length = output
            .len()
            .checked_add(target_length)
            .ok_or_else(|| invalid("decoded target length overflows usize"))?;
        if total_length > options.max_output_size {
            return Err(PatchError::OutputLimitExceeded {
                limit: options.max_output_size,
            });
        }
        if planned_output_size.is_some_and(|planned| total_length > planned) {
            return Err(invalid("VCDIFF window exceeds the planned output size"));
        }

        let delta_indicator = delta.read_u8()?;
        if delta_indicator != 0 {
            return Err(PatchError::UnsupportedVcdiff(format!(
                "delta compression indicator 0x{delta_indicator:02X}"
            )));
        }
        let data_length = delta.read_varint()?;
        let instruction_length = delta.read_varint()?;
        let address_length = delta.read_varint()?;
        let checksum = if window_indicator & VCD_CHECKSUM != 0 {
            let value = delta.read_varint()?;
            Some(u32::try_from(value).map_err(|_| invalid("Adler32 value exceeds u32"))?)
        } else {
            None
        };

        let data = delta.take(data_length)?;
        let instructions = delta.take(instruction_length)?;
        let addresses = delta.take(address_length)?;
        delta.finish("delta window")?;

        let interleaved = version == EXTENDED_VERSION && data.is_empty() && addresses.is_empty();
        let source_segment = if window_indicator & VCD_SOURCE != 0 {
            checked_segment(source, source_position, source_length, "dictionary")?
        } else if window_indicator & VCD_TARGET != 0 {
            checked_segment(&output, source_position, source_length, "decoded target")?
        } else {
            &[]
        };

        let target_window = decode_window(
            source_segment,
            target_length,
            WindowSections {
                data,
                instructions,
                addresses,
                interleaved,
            },
            &table,
            (near_cache_size, same_cache_size),
        )?;
        if let Some(expected) = checksum {
            let actual = adler32(&target_window);
            if actual != expected {
                return Err(invalid(format!(
                    "Adler32 mismatch: expected {expected:08X}, got {actual:08X}"
                )));
            }
        }
        output.extend_from_slice(&target_window);
    }

    Ok(output)
}

/// Streaming convenience wrapper with a configurable output limit.
pub fn decode_stream<R: Read, P: Read, W: Write>(
    mut source: R,
    mut patch: P,
    mut output: W,
    options: DecodeOptions,
) -> Result<()> {
    let mut source_bytes = Vec::new();
    let mut patch_bytes = Vec::new();
    source.read_to_end(&mut source_bytes)?;
    patch.read_to_end(&mut patch_bytes)?;
    output.write_all(&decode_with_options(&source_bytes, &patch_bytes, options)?)?;
    Ok(())
}

struct WindowSections<'a> {
    data: &'a [u8],
    instructions: &'a [u8],
    addresses: &'a [u8],
    interleaved: bool,
}

fn decode_window(
    source: &[u8],
    target_length: usize,
    input: WindowSections<'_>,
    table: &DecoderCodeTable,
    cache_sizes: (usize, usize),
) -> Result<Vec<u8>> {
    let mut sections = Sections::new(
        input.data,
        input.instructions,
        input.addresses,
        input.interleaved,
    );
    let mut target = Vec::with_capacity(target_length);
    let mut cache = AddressCache::new(cache_sizes.0, cache_sizes.1);

    while target.len() < target_length {
        let opcode = sections.instructions.read_u8()?;
        for instruction in table.instructions(opcode)? {
            let Some((kind, fixed_size)) = instruction else {
                continue;
            };
            let size = if fixed_size == 0 {
                sections.instructions.read_varint()?
            } else {
                fixed_size
            };
            if size == 0 {
                return Err(invalid("zero-sized VCDIFF instruction"));
            }
            let new_length = target
                .len()
                .checked_add(size)
                .ok_or_else(|| invalid("instruction size overflows usize"))?;
            if new_length > target_length {
                return Err(invalid("instruction exceeds target window length"));
            }

            match kind {
                InstructionKind::Add => {
                    let bytes = sections.read_data(size)?;
                    target.extend_from_slice(bytes);
                }
                InstructionKind::Run => {
                    let byte = sections.read_data(1)?[0];
                    target.resize(new_length, byte);
                }
                InstructionKind::Copy(mode) => {
                    let here = source
                        .len()
                        .checked_add(target.len())
                        .ok_or_else(|| invalid("COPY address space overflows usize"))?;
                    let encoded = sections.read_address(cache.is_same_mode(mode))?;
                    let address = cache.decode(mode, encoded, here)?;
                    if address >= here {
                        return Err(invalid("COPY address points beyond available data"));
                    }
                    for offset in 0..size {
                        let index = address
                            .checked_add(offset)
                            .ok_or_else(|| invalid("COPY address overflows usize"))?;
                        let byte = if index < source.len() {
                            source[index]
                        } else {
                            let target_index = index - source.len();
                            *target
                                .get(target_index)
                                .ok_or_else(|| invalid("COPY reads unavailable target data"))?
                        };
                        target.push(byte);
                    }
                }
            }
        }
    }

    sections.finish()?;
    Ok(target)
}

struct Sections<'a> {
    data: Cursor<'a>,
    instructions: Cursor<'a>,
    addresses: Cursor<'a>,
    interleaved: bool,
}

impl<'a> Sections<'a> {
    fn new(data: &'a [u8], instructions: &'a [u8], addresses: &'a [u8], interleaved: bool) -> Self {
        Self {
            data: Cursor::new(data),
            instructions: Cursor::new(instructions),
            addresses: Cursor::new(addresses),
            interleaved,
        }
    }

    fn read_data(&mut self, size: usize) -> Result<&'a [u8]> {
        if self.interleaved {
            self.instructions.read_exact(size)
        } else {
            self.data.read_exact(size)
        }
    }

    fn read_address(&mut self, same_mode: bool) -> Result<usize> {
        let cursor = if self.interleaved {
            &mut self.instructions
        } else {
            &mut self.addresses
        };
        if same_mode {
            Ok(usize::from(cursor.read_u8()?))
        } else {
            cursor.read_varint()
        }
    }

    fn finish(self) -> Result<()> {
        self.instructions.finish("instruction section")?;
        self.data.finish("data section")?;
        self.addresses.finish("address section")
    }
}

fn adler32(data: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    let mut first = 1_u32;
    let mut second = 0_u32;
    for byte in data {
        first = (first + u32::from(*byte)) % MODULUS;
        second = (second + first) % MODULUS;
    }
    (second << 16) | first
}
