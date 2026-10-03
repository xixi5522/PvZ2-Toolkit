use std::borrow::Cow;
use std::io::Cursor;

use byteorder::{ByteOrder, LittleEndian};
use rsb_archive::{
    RSG_DATA_ALIGNMENT, RsgHeader, compress_rsg_zlib_with_level, decompress_zlib_exact,
};

use crate::Result;

use super::{PacketPatchMode, invalid};

const SUPPORTED_RSG_VERSION: u32 = 4;
const INFORMATION_OFFSET: usize = 20;
const GENERAL_OFFSET: usize = 24;
const GENERAL_STORED_SIZE: usize = 28;
const TEXTURE_OFFSET: usize = 40;
const TEXTURE_STORED_SIZE: usize = 44;
const TWINNING_ZLIB_LEVEL: u32 = 9;

pub(super) fn delta_input(packet: &[u8], mode: PacketPatchMode) -> Result<Cow<'_, [u8]>> {
    match mode {
        PacketPatchMode::Stored => Ok(Cow::Borrowed(packet)),
        PacketPatchMode::Raw if packet.is_empty() => Ok(Cow::Borrowed(packet)),
        PacketPatchMode::Raw => Ok(Cow::Owned(uncompress(packet)?)),
    }
}

pub(super) fn compress(raw: &[u8]) -> Result<Vec<u8>> {
    let header = read_header(raw)?;
    let information_size = usize::try_from(header.file_offset)
        .map_err(|_| invalid("RSG information size does not fit in memory"))?;
    let general_size = usize::try_from(header.part0_size)
        .map_err(|_| invalid("general section size does not fit in memory"))?;
    let texture_size = usize::try_from(header.part1_size)
        .map_err(|_| invalid("texture section size does not fit in memory"))?;
    let general_raw_offset = aligned(information_size)?;
    let texture_raw_offset = aligned(
        general_raw_offset
            .checked_add(general_size)
            .ok_or_else(|| invalid("raw general range overflows usize"))?,
    )?;
    let general = checked_range(raw, general_raw_offset, general_size, "raw general")?;
    let texture = checked_range(raw, texture_raw_offset, texture_size, "raw texture")?;

    let mut output = raw
        .get(..information_size)
        .ok_or_else(|| invalid("raw RSG information section is truncated"))?
        .to_vec();
    align(&mut output)?;
    let general_offset =
        u32::try_from(output.len()).map_err(|_| invalid("general section offset exceeds u32"))?;
    let general_stored = if header.flags & 0b10 != 0 {
        compress_rsg_zlib_with_level(general, TWINNING_ZLIB_LEVEL)?
    } else {
        general.to_vec()
    };
    let general_stored_size = u32::try_from(general_stored.len())
        .map_err(|_| invalid("general stored size exceeds u32"))?;
    output.extend_from_slice(&general_stored);

    let texture_offset =
        u32::try_from(output.len()).map_err(|_| invalid("texture section offset exceeds u32"))?;
    let texture_stored = if header.flags & 0b01 != 0 && !texture.is_empty() {
        compress_rsg_zlib_with_level(texture, TWINNING_ZLIB_LEVEL)?
    } else {
        texture.to_vec()
    };
    let texture_stored_size = u32::try_from(texture_stored.len())
        .map_err(|_| invalid("texture stored size exceeds u32"))?;
    output.extend_from_slice(&texture_stored);

    LittleEndian::write_u32(&mut output[INFORMATION_OFFSET..], header.file_offset);
    LittleEndian::write_u32(&mut output[GENERAL_OFFSET..], general_offset);
    LittleEndian::write_u32(&mut output[GENERAL_STORED_SIZE..], general_stored_size);
    LittleEndian::write_u32(&mut output[TEXTURE_OFFSET..], texture_offset);
    LittleEndian::write_u32(&mut output[TEXTURE_STORED_SIZE..], texture_stored_size);
    Ok(output)
}

fn uncompress(packet: &[u8]) -> Result<Vec<u8>> {
    let header = read_header(packet)?;
    let information_size = usize::try_from(header.file_offset)
        .map_err(|_| invalid("RSG information size does not fit in memory"))?;
    let mut output = packet
        .get(..information_size)
        .ok_or_else(|| invalid("RSG information section is truncated"))?
        .to_vec();
    align(&mut output)?;
    append_section(
        &mut output,
        packet,
        header.part0_offset,
        header.part0_zlib,
        header.part0_size,
        header.flags & 0b10 != 0,
        "general",
    )?;
    align(&mut output)?;
    append_section(
        &mut output,
        packet,
        header.part1_offset,
        header.part1_zlib,
        header.part1_size,
        header.flags & 0b01 != 0,
        "texture",
    )?;
    align(&mut output)?;
    Ok(output)
}

fn read_header(packet: &[u8]) -> Result<RsgHeader> {
    let header = RsgHeader::read_from(&mut Cursor::new(packet))?;
    if header.version != SUPPORTED_RSG_VERSION {
        return Err(invalid(format!(
            "raw packet mode requires RSG version {SUPPORTED_RSG_VERSION}, found {}",
            header.version
        )));
    }
    Ok(header)
}

fn append_section(
    output: &mut Vec<u8>,
    packet: &[u8],
    offset: u32,
    stored_size: u32,
    original_size: u32,
    compressed: bool,
    name: &str,
) -> Result<()> {
    let offset = usize::try_from(offset)
        .map_err(|_| invalid(format!("{name} offset does not fit in memory")))?;
    let stored_size = usize::try_from(stored_size)
        .map_err(|_| invalid(format!("{name} stored size does not fit in memory")))?;
    let original_size = usize::try_from(original_size)
        .map_err(|_| invalid(format!("{name} original size does not fit in memory")))?;
    // Empty RSG sections have no zlib stream even when the packet-level flag
    // marks that section type as compressed. Match rsb-archive's packet reader
    // and do not ask the decoder to consume an empty input.
    if original_size == 0 {
        return Ok(());
    }
    let stored = checked_range(packet, offset, stored_size, name)?;
    if compressed {
        output.extend_from_slice(&decompress_zlib_exact(stored, original_size)?);
    } else {
        if stored_size < original_size {
            return Err(invalid(format!(
                "uncompressed {name} section is shorter than its original size"
            )));
        }
        output.extend_from_slice(&stored[..original_size]);
    }
    Ok(())
}

fn checked_range<'a>(data: &'a [u8], offset: usize, size: usize, name: &str) -> Result<&'a [u8]> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| invalid(format!("{name} range overflows usize")))?;
    data.get(offset..end)
        .ok_or_else(|| invalid(format!("{name} section is truncated")))
}

fn aligned(length: usize) -> Result<usize> {
    if length == 0 {
        return Ok(0);
    }
    length
        .checked_add(RSG_DATA_ALIGNMENT - 1)
        .map(|value| value / RSG_DATA_ALIGNMENT * RSG_DATA_ALIGNMENT)
        .ok_or_else(|| invalid("RSG alignment overflows usize"))
}

fn align(output: &mut Vec<u8>) -> Result<()> {
    output.resize(aligned(output.len())?, 0);
    Ok(())
}
