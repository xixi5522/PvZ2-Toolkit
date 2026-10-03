use crate::error::{Result, RsbError};
use crate::rsg::types::{Part1Extra, RsgHeader, RsgPayload, UnpackedFile};
use crate::rsg::zlib::{decompress_zlib_exact, is_zlib_stream};
use crate::schema::file_list::read_file_list;
use std::io::{Read, Seek, SeekFrom};

pub fn unpack_rsg(reader: &mut (impl Read + Seek)) -> Result<Vec<UnpackedFile>> {
    let start_pos = reader.stream_position()?;
    let header = RsgHeader::read_from(reader)?;

    let files = read_file_list::<RsgPayload, _>(
        reader,
        start_pos + header.file_list_offset as u64,
        header.file_list_length as u64,
    )?;

    // Helper to get data
    let mut outputs = Vec::new();

    // Read Data Blobs
    // Part0 Data
    let part0_data = read_packet_data(
        reader,
        start_pos,
        header.part0_offset as u64,
        header.part0_size as u64,
        header.part0_zlib as u64,
        header.flags,
        false,
    )?;

    // Part1 Data (Atlas/Textures)
    let part1_data = read_packet_data(
        reader,
        start_pos,
        header.part1_offset as u64,
        header.part1_size as u64,
        header.part1_zlib as u64,
        header.flags,
        true,
    )?;

    for (path, payload) in files {
        let (data_slice, info_offset, info_size, is_part1, part1_info) = match payload {
            RsgPayload::Part0(info) => (&part0_data, info.offset, info.size, false, None),
            RsgPayload::Part1(info) => (
                &part1_data,
                info.offset,
                info.size,
                true,
                Some(Part1Extra {
                    id: info.id,
                    width: info.width,
                    height: info.height,
                }),
            ),
        };

        let end_idx = info_offset as usize + info_size as usize;
        if end_idx <= data_slice.len() {
            let file_bytes = data_slice[info_offset as usize..end_idx].to_vec();
            outputs.push(UnpackedFile {
                path,
                data: file_bytes,
                is_part1,
                part1_info,
            });
        } else {
            return Err(RsbError::PacketDataOutOfBounds { path });
        }
    }

    Ok(outputs)
}

pub(crate) fn read_packet_data(
    reader: &mut (impl Read + Seek),
    start_pos: u64,
    offset: u64,
    size: u64,
    z_size: u64,
    flags: u32,
    is_atlas: bool,
) -> Result<Vec<u8>> {
    if size == 0 {
        return Ok(Vec::new());
    }

    let read_offset = start_pos + offset;
    reader.seek(SeekFrom::Start(read_offset))?;

    let mut raw_data = vec![0u8; z_size as usize];
    reader.read_exact(&mut raw_data)?;

    let actually_zlib = if is_atlas {
        if flags == 0 || flags == 2 {
            // Check if it LOOKS like zlib despite flags
            is_zlib_stream(&raw_data)
        } else {
            true
        }
    } else if flags < 2 {
        is_zlib_stream(&raw_data)
    } else {
        true
    };

    if actually_zlib {
        let expected_size = usize::try_from(size)
            .map_err(|_| RsbError::Other("RSG packet data size does not fit in memory".into()))?;
        decompress_zlib_exact(&raw_data, expected_size)
    } else {
        Ok(raw_data)
    }
}
