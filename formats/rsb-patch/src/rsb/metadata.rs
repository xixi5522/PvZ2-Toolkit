use byteorder::{ByteOrder, LittleEndian};
use rsb_archive::RsgInfo;

use crate::Result;

use super::invalid;

const SUBGROUP_OFFSET: usize = 128;
const SUBGROUP_SIZE: usize = 132;
const SUBGROUP_PACKET_HEADER: usize = 140;
const SUBGROUP_PACKET_HEADER_SIZE: usize = 32;
const RSG_PACKET_HEADER_SOURCE: usize = 16;

#[derive(Clone, Copy)]
pub(super) struct SubgroupTable {
    offset: usize,
    record_size: usize,
}

impl SubgroupTable {
    pub fn new(offset: u32, record_size: u32) -> Result<Self> {
        Ok(Self {
            offset: usize::try_from(offset)
                .map_err(|_| invalid("RSG information offset does not fit in memory"))?,
            record_size: usize::try_from(record_size)
                .map_err(|_| invalid("RSG record size does not fit in memory"))?,
        })
    }

    pub fn update(
        self,
        information: &mut [u8],
        index: usize,
        info: &RsgInfo,
        packet: &[u8],
    ) -> Result<()> {
        let record_offset = index
            .checked_mul(self.record_size)
            .and_then(|value| self.offset.checked_add(value))
            .ok_or_else(|| invalid("RSG metadata record offset overflows usize"))?;
        let record_end = record_offset
            .checked_add(self.record_size)
            .ok_or_else(|| invalid("RSG metadata record range overflows usize"))?;
        if record_end > information.len()
            || self.record_size < SUBGROUP_PACKET_HEADER + SUBGROUP_PACKET_HEADER_SIZE
        {
            return Err(invalid("RSG metadata record is truncated"));
        }

        LittleEndian::write_u32(
            &mut information[record_offset + SUBGROUP_OFFSET..],
            info.rsg_offset,
        );
        LittleEndian::write_u32(
            &mut information[record_offset + SUBGROUP_SIZE..],
            info.rsg_length,
        );
        let packet_header = packet
            .get(RSG_PACKET_HEADER_SOURCE..RSG_PACKET_HEADER_SOURCE + SUBGROUP_PACKET_HEADER_SIZE)
            .ok_or_else(|| invalid("rebuilt RSG header is truncated"))?;
        information[record_offset + SUBGROUP_PACKET_HEADER
            ..record_offset + SUBGROUP_PACKET_HEADER + SUBGROUP_PACKET_HEADER_SIZE]
            .copy_from_slice(packet_header);
        Ok(())
    }
}
