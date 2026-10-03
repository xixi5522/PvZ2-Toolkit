use std::io::{Read, Seek, SeekFrom, Write};

use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::ZlibDecoder;

use crate::{Result, SMF_MAGIC, SmfError, SmfMetadata, SmfVariant};

/// Inspect an SMF stream and restore the reader to its original position.
pub fn inspect<R: Read + Seek>(reader: &mut R) -> Result<SmfMetadata> {
    let start = reader.stream_position()?;
    let end = reader.seek(SeekFrom::End(0))?;
    let total_size = end.saturating_sub(start);
    reader.seek(SeekFrom::Start(start))?;

    let result = inspect_at(reader, start, total_size);
    reader.seek(SeekFrom::Start(start))?;
    result
}

/// Decode an SMF stream into a byte vector, detecting its header layout.
pub fn decode<R: Read + Seek>(mut reader: R) -> Result<Vec<u8>> {
    let metadata = inspect(&mut reader)?;
    let capacity = usize::try_from(metadata.uncompressed_size).unwrap_or(0);
    let mut output = Vec::with_capacity(capacity);
    decode_payload(&mut reader, &mut output, metadata)?;
    Ok(output)
}

/// Decode an SMF stream directly into a writer.
pub fn decode_to<R: Read + Seek, W: Write>(mut reader: R, mut writer: W) -> Result<SmfMetadata> {
    let metadata = inspect(&mut reader)?;
    decode_payload(&mut reader, &mut writer, metadata)?;
    Ok(metadata)
}

fn inspect_at<R: Read + Seek>(reader: &mut R, start: u64, total_size: u64) -> Result<SmfMetadata> {
    if total_size < SmfVariant::Compact32.header_len() {
        return Err(SmfError::InputTooShort {
            minimum: SmfVariant::Compact32.header_len(),
            actual: total_size,
        });
    }

    let magic = reader.read_u32::<LittleEndian>()?;
    if magic != SMF_MAGIC {
        return Err(SmfError::InvalidMagic {
            expected: SMF_MAGIC,
            actual: magic,
        });
    }

    let second_word = reader.read_u32::<LittleEndian>()?;
    if zlib_header_at(
        reader,
        start + SmfVariant::Compact32.header_len(),
        end(start, total_size),
    )? {
        return Ok(metadata(
            SmfVariant::Compact32,
            u64::from(second_word),
            total_size,
        ));
    }

    if second_word == 0 && total_size >= SmfVariant::Extended64.header_len() {
        reader.seek(SeekFrom::Start(start + 8))?;
        let declared_size = reader.read_u64::<LittleEndian>()?;
        if zlib_header_at(
            reader,
            start + SmfVariant::Extended64.header_len(),
            end(start, total_size),
        )? {
            return Ok(metadata(SmfVariant::Extended64, declared_size, total_size));
        }
    }

    Err(SmfError::InvalidZlibHeader {
        offset: if second_word == 0 { 16 } else { 8 },
    })
}

fn decode_payload<R: Read + Seek, W: Write>(
    reader: &mut R,
    writer: &mut W,
    metadata: SmfMetadata,
) -> Result<()> {
    let start = reader.stream_position()?;
    reader.seek(SeekFrom::Start(start + metadata.variant.header_len()))?;
    let mut decoder = ZlibDecoder::new(reader);
    let actual = std::io::copy(&mut decoder, writer)?;
    if actual != metadata.uncompressed_size {
        return Err(SmfError::SizeMismatch {
            declared: metadata.uncompressed_size,
            actual,
        });
    }
    Ok(())
}

fn metadata(variant: SmfVariant, uncompressed_size: u64, total_size: u64) -> SmfMetadata {
    SmfMetadata {
        variant,
        uncompressed_size,
        compressed_size: total_size.saturating_sub(variant.header_len()),
        total_size,
    }
}

fn zlib_header_at<R: Read + Seek>(reader: &mut R, offset: u64, end: u64) -> Result<bool> {
    if end.saturating_sub(offset) < 2 {
        return Ok(false);
    }
    reader.seek(SeekFrom::Start(offset))?;
    let cmf = reader.read_u8()?;
    let flg = reader.read_u8()?;
    Ok(cmf & 0x0F == 8 && cmf >> 4 <= 7 && (u16::from(cmf) * 256 + u16::from(flg)) % 31 == 0)
}

fn end(start: u64, total_size: u64) -> u64 {
    start.saturating_add(total_size)
}
