mod block_hash;
mod writer;

use std::io::{Read, Write};

use crate::Result;

use super::{
    EXTENDED_VERSION, EncodeOptions, MAGIC, STANDARD_VERSION, VCD_SOURCE, VcdiffFormat, invalid,
    io::write_varint,
};
use block_hash::{BLOCK_SIZE, build_operations};
use writer::encode_operations;

/// Encode `target` relative to `source` using the Twinning-compatible
/// interleaved representation.
pub fn encode(source: &[u8], target: &[u8]) -> Result<Vec<u8>> {
    encode_with_options(source, target, EncodeOptions::default())
}

/// Encode `target` relative to `source` using the selected VCDIFF layout.
pub fn encode_with_options(
    source: &[u8],
    target: &[u8],
    options: EncodeOptions,
) -> Result<Vec<u8>> {
    if options.minimum_match < BLOCK_SIZE {
        return Err(invalid(format!(
            "minimum match must be at least {BLOCK_SIZE} bytes"
        )));
    }
    if source.len() > i32::MAX as usize
        || source.len().saturating_add(target.len()) > i32::MAX as usize
    {
        return Err(invalid(
            "open-vcdiff address space cannot exceed 0x7FFFFFFF bytes",
        ));
    }

    let version = match options.format {
        VcdiffFormat::Standard => STANDARD_VERSION,
        VcdiffFormat::Interleaved => EXTENDED_VERSION,
    };
    let mut output = Vec::with_capacity(target.len() / 2 + 32);
    output.extend_from_slice(&MAGIC);
    output.push(version);
    output.push(0); // Hdr_Indicator: default code table, no compressor/app header.
    if target.is_empty() {
        return Ok(output);
    }

    let operations = build_operations(source, target, options.minimum_match);
    let (data, instructions, addresses) =
        encode_operations(&operations, options.format, source.len());

    let mut delta = Vec::new();
    write_varint(target.len(), &mut delta);
    delta.push(0); // Delta_Indicator: no secondary compression.
    write_varint(data.len(), &mut delta);
    write_varint(instructions.len(), &mut delta);
    write_varint(addresses.len(), &mut delta);
    delta.extend_from_slice(&data);
    delta.extend_from_slice(&instructions);
    delta.extend_from_slice(&addresses);

    output.push(VCD_SOURCE);
    write_varint(source.len(), &mut output);
    write_varint(0, &mut output);
    write_varint(delta.len(), &mut output);
    output.extend_from_slice(&delta);
    Ok(output)
}

/// Streaming convenience wrapper. The VCDIFF algorithm still needs each
/// complete input in memory so it can locate COPY matches.
pub fn encode_stream<R: Read, T: Read, W: Write>(
    mut source: R,
    mut target: T,
    mut output: W,
    options: EncodeOptions,
) -> Result<()> {
    let mut source_bytes = Vec::new();
    let mut target_bytes = Vec::new();
    source.read_to_end(&mut source_bytes)?;
    target.read_to_end(&mut target_bytes)?;
    output.write_all(&encode_with_options(&source_bytes, &target_bytes, options)?)?;
    Ok(())
}
