use super::helpers::ilog;
use crate::bit_stream::{BitRead, BitWriter};
use crate::error::{WemError, WemResult};

pub(crate) fn rebuild_setup<B: BitRead>(
    channels: u16,
    reader: &mut B,
    writer: &mut BitWriter,
    codebook_count: u32,
) -> WemResult<Vec<bool>> {
    let floor_count_less1 = reader.read_bits(6)?;
    let floor_count = floor_count_less1 + 1;
    writer.write_bits(floor_count_less1, 6);
    for _ in 0..floor_count {
        writer.write_bits(1, 16);
        rebuild_floor(reader, codebook_count, writer)?;
    }

    let residue_count_less1 = reader.read_bits(6)?;
    let residue_count = residue_count_less1 + 1;
    writer.write_bits(residue_count_less1, 6);
    for _ in 0..residue_count {
        rebuild_residue(reader, codebook_count, writer)?;
    }

    let mapping_count_less1 = reader.read_bits(6)?;
    let mapping_count = mapping_count_less1 + 1;
    writer.write_bits(mapping_count_less1, 6);
    for _ in 0..mapping_count {
        rebuild_mapping(channels, reader, floor_count, residue_count, writer)?;
    }

    let mode_count_less1 = reader.read_bits(6)?;
    let mode_count = mode_count_less1 + 1;
    writer.write_bits(mode_count_less1, 6);
    let mut mode_blockflag = Vec::with_capacity(mode_count as usize);
    for _ in 0..mode_count {
        let block_flag = reader.read_bits(1)?;
        writer.write_bits(block_flag, 1);
        mode_blockflag.push(block_flag != 0);
        writer.write_bits(0, 16);
        writer.write_bits(0, 16);
        let mapping = reader.read_bits(8)?;
        writer.write_bits(mapping, 8);
        if mapping >= mapping_count {
            return Err(WemError::parse("invalid Vorbis mode mapping"));
        }
    }
    writer.write_bits(1, 1);
    Ok(mode_blockflag)
}

fn rebuild_floor<B: BitRead>(
    reader: &mut B,
    codebook_count: u32,
    writer: &mut BitWriter,
) -> WemResult<()> {
    let partition_count = reader.read_bits(5)?;
    writer.write_bits(partition_count, 5);
    let mut partition_classes = vec![0_u32; partition_count as usize];
    let mut maximum_class = 0_u32;
    for class in &mut partition_classes {
        *class = reader.read_bits(4)?;
        writer.write_bits(*class, 4);
        maximum_class = maximum_class.max(*class);
    }

    let mut dimensions = vec![0_u32; (maximum_class + 1) as usize];
    for dimension in &mut dimensions {
        let less_one = reader.read_bits(3)?;
        writer.write_bits(less_one, 3);
        *dimension = less_one + 1;
        let subclasses = reader.read_bits(2)?;
        writer.write_bits(subclasses, 2);
        if subclasses != 0 {
            let masterbook = reader.read_bits(8)?;
            writer.write_bits(masterbook, 8);
            if masterbook >= codebook_count {
                return Err(WemError::parse("invalid Vorbis floor masterbook"));
            }
        }
        for _ in 0..(1_u32 << subclasses) {
            let book_plus_one = reader.read_bits(8)?;
            writer.write_bits(book_plus_one, 8);
            if book_plus_one > codebook_count {
                return Err(WemError::parse("invalid Vorbis floor subclass book"));
            }
        }
    }

    let multiplier_less_one = reader.read_bits(2)?;
    writer.write_bits(multiplier_less_one, 2);
    let range_bits = reader.read_bits(4)?;
    writer.write_bits(range_bits, 4);
    for class in partition_classes {
        for _ in 0..dimensions[class as usize] {
            let value = reader.read_bits(range_bits as u8)?;
            writer.write_bits(value, range_bits as u8);
        }
    }
    Ok(())
}

fn rebuild_residue<B: BitRead>(
    reader: &mut B,
    codebook_count: u32,
    writer: &mut BitWriter,
) -> WemResult<()> {
    let residue_type = reader.read_bits(2)?;
    writer.write_bits(residue_type, 16);
    if residue_type > 2 {
        return Err(WemError::parse("invalid Vorbis residue type"));
    }
    let begin = reader.read_bits(24)?;
    let end = reader.read_bits(24)?;
    let partition_size_less_one = reader.read_bits(24)?;
    let classifications_less_one = reader.read_bits(6)?;
    let classbook = reader.read_bits(8)?;
    writer.write_bits(begin, 24);
    writer.write_bits(end, 24);
    writer.write_bits(partition_size_less_one, 24);
    writer.write_bits(classifications_less_one, 6);
    writer.write_bits(classbook, 8);
    if classbook >= codebook_count {
        return Err(WemError::parse("invalid Vorbis residue classbook"));
    }

    let mut cascades = vec![0_u32; (classifications_less_one + 1) as usize];
    for cascade in &mut cascades {
        let low = reader.read_bits(3)?;
        writer.write_bits(low, 3);
        let has_high = reader.read_bits(1)?;
        writer.write_bits(has_high, 1);
        let high = if has_high != 0 {
            let value = reader.read_bits(5)?;
            writer.write_bits(value, 5);
            value
        } else {
            0
        };
        *cascade = high * 8 + low;
    }
    for cascade in cascades {
        for bit in 0..8 {
            if cascade & (1 << bit) != 0 {
                let book = reader.read_bits(8)?;
                writer.write_bits(book, 8);
                if book >= codebook_count {
                    return Err(WemError::parse("invalid Vorbis residue book"));
                }
            }
        }
    }
    Ok(())
}

fn rebuild_mapping<B: BitRead>(
    channels: u16,
    reader: &mut B,
    floor_count: u32,
    residue_count: u32,
    writer: &mut BitWriter,
) -> WemResult<()> {
    writer.write_bits(0, 16);
    let has_submaps = reader.read_bits(1)?;
    writer.write_bits(has_submaps, 1);
    let submaps = if has_submaps != 0 {
        let less_one = reader.read_bits(4)?;
        writer.write_bits(less_one, 4);
        less_one + 1
    } else {
        1
    };

    let has_coupling = reader.read_bits(1)?;
    writer.write_bits(has_coupling, 1);
    if has_coupling != 0 {
        let steps_less_one = reader.read_bits(8)?;
        writer.write_bits(steps_less_one, 8);
        let coupling_bits = ilog(u32::from(channels) - 1);
        for _ in 0..=steps_less_one {
            let magnitude = reader.read_bits(coupling_bits)?;
            let angle = reader.read_bits(coupling_bits)?;
            writer.write_bits(magnitude, coupling_bits);
            writer.write_bits(angle, coupling_bits);
            if magnitude == angle
                || magnitude >= u32::from(channels)
                || angle >= u32::from(channels)
            {
                return Err(WemError::parse("invalid Vorbis channel coupling"));
            }
        }
    }

    let reserved = reader.read_bits(2)?;
    writer.write_bits(reserved, 2);
    if reserved != 0 {
        return Err(WemError::parse("Vorbis mapping reserved field is non-zero"));
    }
    if submaps > 1 {
        for _ in 0..channels {
            let mux = reader.read_bits(4)?;
            writer.write_bits(mux, 4);
            if mux >= submaps {
                return Err(WemError::parse("invalid Vorbis mapping mux"));
            }
        }
    }
    for _ in 0..submaps {
        let time = reader.read_bits(8)?;
        writer.write_bits(time, 8);
        let floor = reader.read_bits(8)?;
        writer.write_bits(floor, 8);
        if floor >= floor_count {
            return Err(WemError::parse("invalid Vorbis floor mapping"));
        }
        let residue = reader.read_bits(8)?;
        writer.write_bits(residue, 8);
        if residue >= residue_count {
            return Err(WemError::parse("invalid Vorbis residue mapping"));
        }
    }
    Ok(())
}
