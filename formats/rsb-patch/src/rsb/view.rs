use std::collections::HashMap;
use std::io::Cursor;

use rsb_archive::{Rsb, RsgInfo};

use crate::{PatchError, Result};

use super::{invalid, metadata::SubgroupTable};

const SUPPORTED_RSB_VERSION: u32 = 4;

pub(super) struct ArchiveView<'a> {
    pub information: &'a [u8],
    pub packets: Vec<RsgInfo>,
    pub subgroups: SubgroupTable,
    pub end: u32,
}

impl<'a> ArchiveView<'a> {
    pub fn parse(data: &'a [u8], context: &'static str) -> Result<Self> {
        let mut archive = Rsb::open(Cursor::new(data))?;
        if archive.header.version != SUPPORTED_RSB_VERSION {
            return Err(PatchError::UnsupportedRsbVersion {
                context,
                actual: archive.header.version,
            });
        }
        let information_size = usize::try_from(archive.header.information_section_size)
            .map_err(|_| invalid("RSB information size does not fit in memory"))?;
        let information = data
            .get(..information_size)
            .ok_or_else(|| invalid(format!("{context} information section is truncated")))?;
        let packets = archive.read_rsg_info()?;
        let end =
            packets
                .iter()
                .try_fold(archive.header.information_section_size, |end, packet| {
                    packet
                        .rsg_offset
                        .checked_add(packet.rsg_length)
                        .map(|packet_end| end.max(packet_end))
                        .ok_or_else(|| invalid("RSG range overflows u32"))
                })?;
        let subgroups = SubgroupTable::new(
            archive.header.rsg_info_begin_offset,
            archive.header.rsg_info_each_length,
        )?;
        Ok(Self {
            information,
            packets,
            subgroups,
            end,
        })
    }

    pub fn packet_bytes<'b>(&self, archive: &'b [u8], index: usize) -> Result<&'b [u8]> {
        let info = self
            .packets
            .get(index)
            .ok_or_else(|| invalid("RSG packet index is out of range"))?;
        self.packet_bytes_by_info(archive, info)
    }

    pub fn packet_bytes_by_info<'b>(&self, archive: &'b [u8], info: &RsgInfo) -> Result<&'b [u8]> {
        let start = usize::try_from(info.rsg_offset)
            .map_err(|_| invalid("RSG offset does not fit in memory"))?;
        let length = usize::try_from(info.rsg_length)
            .map_err(|_| invalid("RSG length does not fit in memory"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("RSG range overflows usize"))?;
        archive
            .get(start..end)
            .ok_or_else(|| invalid(format!("RSG packet {} is outside the archive", info.name)))
    }
}

pub(super) fn index_packets(packets: &[RsgInfo]) -> Result<HashMap<&str, usize>> {
    let mut result = HashMap::with_capacity(packets.len());
    for (index, packet) in packets.iter().enumerate() {
        if result.insert(packet.name.as_str(), index).is_some() {
            return Err(invalid(format!(
                "RSB contains duplicate packet name {}",
                packet.name
            )));
        }
    }
    Ok(result)
}
