use crate::error::{Result, RsbError};
use crate::rsg::types::{Part0Info, Part1Info, RSG_MAGIC, RsgPayload, UnpackedFile};
use crate::rsg::zlib::{RSG_DATA_ALIGNMENT, compress_rsg_zlib, padding_for_data_len};
use byteorder::{LE, WriteBytesExt};
use std::io::{Seek, SeekFrom, Write};

pub fn pack_rsg<W: Write + Seek>(
    writer: &mut W,
    files: &[UnpackedFile],
    version: u32,
    flags: u32,
) -> Result<()> {
    if flags > 3 {
        return Err(RsbError::InvalidCompression(flags));
    }

    let start_pos = writer.stream_position()?;

    let mut part0_buffer = Vec::new();
    let mut part1_buffer = Vec::new();

    let mut part0_items: Vec<(usize, u32, u32)> = Vec::new(); // (file_index, offset, size)
    let mut part1_items: Vec<(usize, u32, u32)> = Vec::new(); // (file_index, offset, size)

    for (i, file) in files.iter().enumerate() {
        let data = &file.data;
        let padding = padding_for_data_len(data.len())?;

        if file.is_part1 {
            let offset = part1_buffer.len() as u32;
            part1_buffer.extend_from_slice(data);
            part1_buffer.extend(std::iter::repeat_n(0, padding));
            part1_items.push((i, offset, data.len() as u32));
        } else {
            let offset = part0_buffer.len() as u32;
            part0_buffer.extend_from_slice(data);
            part0_buffer.extend(std::iter::repeat_n(0, padding));
            part0_items.push((i, offset, data.len() as u32));
        }
    }

    let mut part0_lookup = std::collections::HashMap::new();
    for (idx, off, sz) in part0_items {
        part0_lookup.insert(idx, (off, sz));
    }
    let mut part1_lookup = std::collections::HashMap::new();
    for (idx, off, sz) in part1_items {
        part1_lookup.insert(idx, (off, sz));
    }

    let mut file_list_entries: Vec<(String, RsgPayload)> = Vec::with_capacity(files.len());

    for (i, file) in files.iter().enumerate() {
        let payload = if file.is_part1 {
            let (off, sz) = part1_lookup[&i];
            let info = file
                .part1_info
                .as_ref()
                .ok_or_else(|| RsbError::MissingPart1Info(file.path.clone()))?;
            RsgPayload::Part1(Part1Info {
                offset: off,
                size: sz,
                id: info.id,
                width: info.width,
                height: info.height,
            })
        } else {
            let (off, sz) = part0_lookup[&i];
            RsgPayload::Part0(Part0Info {
                offset: off,
                size: sz,
            })
        };
        file_list_entries.push((file.path.clone(), payload));
    }

    writer.write_u32::<LE>(RSG_MAGIC)?;
    writer.write_u32::<LE>(version)?;
    writer.write_u64::<LE>(0)?;
    writer.write_u32::<LE>(flags)?;

    let _header_offsets_pos = writer.stream_position()?;
    // Header fields (60B) + 12B gap = 72B total before file_list.
    // PvZ2 RSG places file_list at offset 0x5C from RSG start.
    writer.write_all(&[0u8; 72])?;

    let file_list_offset = (writer.stream_position()? - start_pos) as u32;

    let file_list_begin = writer.stream_position()?;
    use crate::schema::file_list::write_file_list;
    write_file_list(writer, file_list_begin, &file_list_entries)?;
    let file_list_end = writer.stream_position()?;
    let file_list_len = (file_list_end - file_list_begin) as u32;

    fn align_file<W: Write + Seek>(w: &mut W) -> Result<()> {
        let pos = w.stream_position()?;
        let alignment = RSG_DATA_ALIGNMENT as u64;
        if pos % alignment != 0 {
            let pad = alignment - (pos % alignment);
            w.write_all(&vec![0u8; pad as usize])?;
        }
        Ok(())
    }
    align_file(writer)?;

    let part0_start_offset = (writer.stream_position()? - start_pos) as u32;
    let mut part0_zlib_len = part0_buffer.len() as u32;
    let part0_final_len = part0_buffer.len() as u32;

    if flags & 0b10 != 0 && !part0_buffer.is_empty() {
        let compressed = compress_rsg_zlib(&part0_buffer)?;
        writer.write_all(&compressed)?;
        part0_zlib_len = compressed.len() as u32;
    } else {
        writer.write_all(&part0_buffer)?;
    }

    let part1_start_offset = (writer.stream_position()? - start_pos) as u32;
    let mut part1_zlib_len = part1_buffer.len() as u32;
    let part1_final_len = part1_buffer.len() as u32;

    if flags & 0b01 != 0 && !part1_buffer.is_empty() {
        let compressed = compress_rsg_zlib(&part1_buffer)?;
        writer.write_all(&compressed)?;
        part1_zlib_len = compressed.len() as u32;
    } else {
        writer.write_all(&part1_buffer)?;
    }

    let end_pos = writer.stream_position()?;
    writer.seek(SeekFrom::Start(start_pos + 20))?;

    // This field is the aligned end of the RSG information/file-list section,
    // not the complete packet length. Real PvZ2 packets name it `file_offset`
    // and normally store the same value as the Part-0 start offset.
    writer.write_u32::<LE>(part0_start_offset)?;

    writer.write_u32::<LE>(part0_start_offset)?;
    writer.write_u32::<LE>(part0_zlib_len)?;
    writer.write_u32::<LE>(part0_final_len)?;

    writer.write_u32::<LE>(0)?;

    writer.write_u32::<LE>(part1_start_offset)?;
    writer.write_u32::<LE>(part1_zlib_len)?;
    writer.write_u32::<LE>(part1_final_len)?;

    writer.write_all(&[0u8; 20])?;

    writer.write_u32::<LE>(file_list_len)?;
    writer.write_u32::<LE>(file_list_offset)?;

    writer.seek(SeekFrom::Start(end_pos))?;
    Ok(())
}
