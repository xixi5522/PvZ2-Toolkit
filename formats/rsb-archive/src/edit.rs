use crate::{
    AutoPoolInfo, CompositeInfo, CompositePacketInfo, FileListInfo, Rsb, RsbError, RsbHeader,
    RsbPtxInfo, RsbWriter, RsgHeader, RsgInfo, UnpackedFile, pack_rsg,
};
use byteorder::{LE, WriteBytesExt};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Seek, SeekFrom, Write};

const RSB_PACKET_ALIGNMENT: usize = 0x1000;
const RSG_INFO_NAME_LENGTH: usize = 128;
const RSG_PACKET_HEAD_OFFSET: usize = 16;
const RSG_INFO_HEAD_LENGTH: usize = 32;
const AUTOPOOL_NAME_LENGTH: usize = 128;

/// Complete replacement for one RSG packet inside an existing RSB archive.
///
/// `original_paths` identifies the resource-index entries owned by the old
/// packet. `files` becomes the new packet file list. Part-1 texture IDs must be
/// contiguous from zero. When the texture count changes,
/// [`RsbArchiveEdit::ptx_infos`] must contain the complete rebuilt global
/// texture table.
#[derive(Clone, Debug)]
pub struct RsgPacketEdit {
    pub packet_index: usize,
    pub original_paths: Vec<String>,
    pub name: String,
    pub version: u32,
    pub compression_flags: u32,
    pub files: Vec<UnpackedFile>,
}

/// Group placement for an RSG packet added to an existing archive.
///
/// A non-composite group normally contains one packet and is serialized with
/// the `_CompositeShell` suffix used by RSB archives. `category` contains the
/// resolution and locale values stored by the composite table.
#[derive(Clone, Debug)]
pub struct RsgPacketGroup {
    pub name: String,
    pub is_composite: bool,
    pub category: [String; 2],
}

/// A new RSG packet appended while rebuilding an existing RSB archive.
///
/// Part-1 textures are supported when [`RsbArchiveEdit::ptx_infos`] includes
/// their metadata after all original packet ranges.
#[derive(Clone, Debug)]
pub struct RsgPacketAddition {
    pub name: String,
    pub version: u32,
    pub compression_flags: u32,
    pub files: Vec<UnpackedFile>,
    pub group: Option<RsgPacketGroup>,
}

/// Changes applied while rebuilding an existing RSB archive.
#[derive(Clone, Debug, Default)]
pub struct RsbArchiveEdit {
    pub packets: Vec<RsgPacketEdit>,
    /// RSG packets appended to the archive.
    pub added_packets: Vec<RsgPacketAddition>,
    /// Original packet indices removed from the archive.
    ///
    /// Their resource paths, composite placements, autopools, and global
    /// Part-1 texture metadata are removed and the remaining indices are
    /// rebuilt automatically.
    pub removed_packets: Vec<usize>,
    /// Replaces the global PTX metadata table.
    ///
    /// The table may grow or shrink when an edited packet adds or removes
    /// Part-1 textures. Every packet's global texture range is then rebuilt
    /// from its ordered Part-1 files.
    pub ptx_infos: Option<Vec<RsbPtxInfo>>,
}

/// Rebuilds an RSB archive while preserving its unknown metadata and manifests.
///
/// Existing metadata bytes are retained, new resource/RSG path dictionaries
/// are appended before the packet region, and every packet offset is rewritten.
/// Unedited packets are copied byte-for-byte.
pub fn rebuild_rsb(source: &[u8], edit: &RsbArchiveEdit) -> crate::Result<Vec<u8>> {
    if !edit.added_packets.is_empty() || !edit.removed_packets.is_empty() {
        return rebuild_rsb_structure(source, edit);
    }

    let mut archive = Rsb::open(Cursor::new(source))?;
    let mut header = archive.header.clone();
    let original_file_list = archive.read_file_list()?;
    let mut rsg_infos = archive.read_rsg_info()?;
    let mut autopool_infos = archive.read_autopool_info()?;
    let original_ptx_infos = archive.read_ptx_info()?;

    let texture_count_changed = edit.packets.iter().any(|packet| {
        rsg_infos.get(packet.packet_index).is_some_and(|info| {
            texture_count(&packet.files) != usize::try_from(info.ptx_number).unwrap_or(usize::MAX)
        })
    });
    let texture_table_size_changed = edit
        .ptx_infos
        .as_ref()
        .is_some_and(|infos| infos.len() != original_ptx_infos.len());
    if texture_count_changed || texture_table_size_changed {
        return rebuild_rsb_structure(source, edit);
    }

    validate_ptx_infos(
        edit.ptx_infos.as_deref(),
        &original_ptx_infos,
        original_ptx_infos.len(),
    )?;
    let edits = validate_packet_edits(edit, &rsg_infos)?;
    let metadata_source_end = metadata_source_end(source, &header, &rsg_infos)?;
    let file_list = update_resource_file_list(original_file_list, &edits, &rsg_infos)?;

    let mut output = Cursor::new(source[..metadata_source_end].to_vec());
    output.seek(SeekFrom::End(0))?;

    let (resource_path_section_offset, resource_path_section_size) = {
        let mut writer = RsbWriter::new(&mut output);
        writer.write_file_list(&file_list)?
    };
    header.resource_path_section_offset = resource_path_section_offset;
    header.resource_path_section_size = resource_path_section_size;

    for (packet_index, info) in rsg_infos.iter_mut().enumerate() {
        if let Some(packet_edit) = edits.get(&packet_index) {
            info.name.clone_from(&packet_edit.name);
            if let Ok(pool_index) = usize::try_from(info.pool_index)
                && let Some(autopool) = autopool_infos.get_mut(pool_index)
            {
                autopool.name = format!("{}_AutoPool", info.name);
            }
        }
    }

    let rsg_list = rsg_infos
        .iter()
        .map(|info| FileListInfo {
            name_path: info.name.clone(),
            pool_index: info.pool_index,
        })
        .collect::<Vec<_>>();
    let (rsg_list_begin_offset, rsg_list_length) = {
        let mut writer = RsbWriter::new(&mut output);
        writer.write_file_list(&rsg_list)?
    };
    header.rsg_list_begin_offset = rsg_list_begin_offset;
    header.rsg_list_length = rsg_list_length;

    align_cursor(&mut output, RSB_PACKET_ALIGNMENT)?;
    let packet_region_offset = u32::try_from(output.position())
        .map_err(|_| RsbError::Other("rebuilt RSB metadata exceeds 4 GiB".into()))?;
    header.information_section_size = packet_region_offset;
    if header.version >= 4 {
        header.information_without_manifest_section_size = packet_region_offset;
    }

    let mut packet_headers = Vec::with_capacity(rsg_infos.len());
    for (packet_index, info) in rsg_infos.iter_mut().enumerate() {
        let packet_offset = u32::try_from(output.position())
            .map_err(|_| RsbError::Other("rebuilt RSB packet offset exceeds 4 GiB".into()))?;
        let packet = if let Some(packet_edit) = edits.get(&packet_index) {
            let mut packet = Cursor::new(Vec::new());
            pack_rsg(
                &mut packet,
                &packet_edit.files,
                packet_edit.version,
                packet_edit.compression_flags,
            )?;
            packet.into_inner()
        } else {
            packet_slice(source, info)?.to_vec()
        };
        let packet_header = RsgHeader::read_from(&mut Cursor::new(&packet))?;
        output.write_all(&packet)?;
        align_cursor(&mut output, RSB_PACKET_ALIGNMENT)?;
        let packet_length = output
            .position()
            .checked_sub(u64::from(packet_offset))
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| RsbError::Other("rebuilt RSG packet exceeds 4 GiB".into()))?;

        info.rsg_offset = packet_offset;
        info.rsg_length = packet_length;
        info.packet_head_info = Some(packet_head_info(&packet)?);
        packet_headers.push(packet_header);
    }

    patch_rsg_info(&mut output, &header, &rsg_infos)?;
    patch_autopool_info(
        &mut output,
        &header,
        &autopool_infos,
        &rsg_infos,
        &packet_headers,
    )?;
    patch_ptx_info(
        &mut output,
        &header,
        edit.ptx_infos
            .as_deref()
            .unwrap_or(original_ptx_infos.as_slice()),
    )?;

    output.seek(SeekFrom::Start(0))?;
    RsbWriter::new(&mut output).write_header(&header)?;
    Ok(output.into_inner())
}

fn rebuild_rsb_structure(source: &[u8], edit: &RsbArchiveEdit) -> crate::Result<Vec<u8>> {
    let mut archive = Rsb::open(Cursor::new(source))?;
    let mut header = archive.header.clone();
    validate_structural_record_sizes(&header)?;

    let original_file_list = archive.read_file_list()?;
    let original_infos = archive.read_rsg_info()?;
    let original_composites = archive.read_composite_info()?;
    let original_autopools = archive.read_autopool_info()?;
    let original_ptx_infos = archive.read_ptx_info()?;

    let edits = validate_packet_edits(edit, &original_infos)?;
    let removed = validate_removed_packets(edit, &original_infos, &edits)?;
    validate_added_packets(&edit.added_packets)?;
    validate_packet_names(&original_infos, &edits, &removed, &edit.added_packets)?;

    let metadata_source_end = metadata_source_end(source, &header, &original_infos)?;
    let mut old_to_new = HashMap::with_capacity(original_infos.len());
    let mut old_pool_to_new = HashMap::with_capacity(original_infos.len());
    let mut rsg_infos = Vec::with_capacity(
        original_infos
            .len()
            .saturating_sub(removed.len())
            .saturating_add(edit.added_packets.len()),
    );
    let mut packet_sources = Vec::with_capacity(rsg_infos.capacity());
    let mut autopool_infos = Vec::with_capacity(rsg_infos.capacity());

    for (old_index, original) in original_infos.iter().enumerate() {
        if removed.contains(&old_index) {
            continue;
        }
        let new_index = rsg_infos.len();
        old_to_new.insert(old_index, new_index);
        if old_pool_to_new
            .insert(original.pool_index, new_index)
            .is_some()
        {
            return Err(RsbError::Other(format!(
                "more than one RSG packet uses pool index {}",
                original.pool_index
            )));
        }

        let packet_edit = edits.get(&old_index).copied();
        let mut info = original.clone();
        if let Some(packet_edit) = packet_edit {
            info.name.clone_from(&packet_edit.name);
            info.ptx_number = u32::try_from(texture_count(&packet_edit.files))
                .map_err(|_| RsbError::Other("RSG texture count exceeds u32".into()))?;
        }
        info.pool_index = i32::try_from(new_index)
            .map_err(|_| RsbError::Other("RSG packet count exceeds i32".into()))?;
        rsg_infos.push(info.clone());
        packet_sources.push(StructuralPacketSource::Original {
            old_index,
            edit: packet_edit,
        });

        let mut autopool = usize::try_from(original.pool_index)
            .ok()
            .and_then(|index| original_autopools.get(index))
            .cloned()
            .unwrap_or_else(|| AutoPoolInfo {
                name: format!("{}_AutoPool", info.name),
                part0_size: 0,
                part1_size: 0,
            });
        if packet_edit.is_some() {
            autopool.name = format!("{}_AutoPool", info.name);
        }
        autopool_infos.push(autopool);
    }

    for addition in &edit.added_packets {
        let new_index = rsg_infos.len();
        let pool_index = i32::try_from(new_index)
            .map_err(|_| RsbError::Other("RSG packet count exceeds i32".into()))?;
        rsg_infos.push(RsgInfo {
            name: addition.name.clone(),
            rsg_offset: 0,
            rsg_length: 0,
            pool_index,
            ptx_number: u32::try_from(texture_count(&addition.files))
                .map_err(|_| RsbError::Other("RSG texture count exceeds u32".into()))?,
            ptx_before_number: u32::try_from(original_ptx_infos.len())
                .map_err(|_| RsbError::Other("global PTX count exceeds u32".into()))?,
            packet_head_info: None,
        });
        packet_sources.push(StructuralPacketSource::Added(addition));
        autopool_infos.push(AutoPoolInfo {
            name: format!("{}_AutoPool", addition.name),
            part0_size: 0,
            part1_size: 0,
        });
    }

    let mut texture_begin = 0_u32;
    for info in &mut rsg_infos {
        info.ptx_before_number = texture_begin;
        texture_begin = texture_begin
            .checked_add(info.ptx_number)
            .ok_or_else(|| RsbError::Other("global PTX count exceeds u32".into()))?;
    }
    let ptx_infos = structural_ptx_infos(
        edit.ptx_infos.as_deref(),
        &original_ptx_infos,
        &original_infos,
        &removed,
        usize::try_from(texture_begin)
            .map_err(|_| RsbError::Other("global PTX count does not fit in memory".into()))?,
    )?;

    let file_list = rebuild_resource_file_list(
        original_file_list,
        &original_infos,
        &edits,
        &removed,
        &old_pool_to_new,
        &edit.added_packets,
        original_infos.len(),
    )?;
    let composite_infos =
        rebuild_composite_infos(original_composites, &old_to_new, &edit.added_packets)?;

    let mut output = Cursor::new(source[..metadata_source_end].to_vec());
    output.seek(SeekFrom::End(0))?;

    let (offset, length) = RsbWriter::new(&mut output).write_file_list(&file_list)?;
    header.resource_path_section_offset = offset;
    header.resource_path_section_size = length;

    let rsg_list = rsg_infos
        .iter()
        .map(|info| FileListInfo {
            name_path: info.name.clone(),
            pool_index: info.pool_index,
        })
        .collect::<Vec<_>>();
    let (offset, length) = RsbWriter::new(&mut output).write_file_list(&rsg_list)?;
    header.rsg_list_begin_offset = offset;
    header.rsg_list_length = length;

    let composite_list = composite_infos
        .iter()
        .enumerate()
        .map(|(index, info)| FileListInfo {
            name_path: composite_storage_name(info),
            pool_index: i32::try_from(index).unwrap_or(i32::MAX),
        })
        .collect::<Vec<_>>();
    let (offset, length) = RsbWriter::new(&mut output).write_file_list(&composite_list)?;
    header.composite_list_begin_offset = offset;
    header.composite_list_length = length;

    let ptx_counts = rsg_infos
        .iter()
        .map(|info| (info.ptx_number, info.ptx_before_number))
        .collect::<Vec<_>>();
    let (offset, each_length) =
        RsbWriter::new(&mut output).write_rsg_info(&rsg_infos, &ptx_counts)?;
    header.rsg_info_begin_offset = offset;
    header.rsg_info_each_length = each_length;
    header.rsg_number = u32::try_from(rsg_infos.len())
        .map_err(|_| RsbError::Other("RSG packet count exceeds u32".into()))?;

    let (offset, each_length) =
        RsbWriter::new(&mut output).write_composite_info(&composite_infos)?;
    header.composite_info_begin_offset = offset;
    header.composite_info_each_length = each_length;
    header.composite_number = u32::try_from(composite_infos.len())
        .map_err(|_| RsbError::Other("composite group count exceeds u32".into()))?;

    let (offset, each_length) = RsbWriter::new(&mut output).write_autopool_info(&autopool_infos)?;
    header.autopool_info_begin_offset = offset;
    header.autopool_info_each_length = each_length;
    header.autopool_number = u32::try_from(autopool_infos.len())
        .map_err(|_| RsbError::Other("autopool count exceeds u32".into()))?;

    header.ptx_info_begin_offset =
        RsbWriter::new(&mut output).write_ptx_info(&ptx_infos, header.ptx_info_each_length)?;
    header.ptx_number = u32::try_from(ptx_infos.len())
        .map_err(|_| RsbError::Other("global PTX count exceeds u32".into()))?;

    align_cursor(&mut output, RSB_PACKET_ALIGNMENT)?;
    let packet_region_offset = u32::try_from(output.position())
        .map_err(|_| RsbError::Other("rebuilt RSB metadata exceeds 4 GiB".into()))?;
    header.information_section_size = packet_region_offset;
    if header.version >= 4 {
        header.information_without_manifest_section_size = packet_region_offset;
    }

    let mut packet_headers = Vec::with_capacity(packet_sources.len());
    for (new_index, source_packet) in packet_sources.into_iter().enumerate() {
        let packet_offset = u32::try_from(output.position())
            .map_err(|_| RsbError::Other("rebuilt RSB packet offset exceeds 4 GiB".into()))?;
        let packet = match source_packet {
            StructuralPacketSource::Original {
                old_index: _,
                edit: Some(packet_edit),
            } => packed_packet(
                &packet_edit.files,
                packet_edit.version,
                packet_edit.compression_flags,
            )?,
            StructuralPacketSource::Original {
                old_index,
                edit: None,
            } => packet_slice(source, &original_infos[old_index])?.to_vec(),
            StructuralPacketSource::Added(addition) => packed_packet(
                &addition.files,
                addition.version,
                addition.compression_flags,
            )?,
        };
        let packet_header = RsgHeader::read_from(&mut Cursor::new(&packet))?;
        output.write_all(&packet)?;
        align_cursor(&mut output, RSB_PACKET_ALIGNMENT)?;
        let packet_length = output
            .position()
            .checked_sub(u64::from(packet_offset))
            .and_then(|length| u32::try_from(length).ok())
            .ok_or_else(|| RsbError::Other("rebuilt RSG packet exceeds 4 GiB".into()))?;
        let info = &mut rsg_infos[new_index];
        info.rsg_offset = packet_offset;
        info.rsg_length = packet_length;
        info.packet_head_info = Some(packet_head_info(&packet)?);
        packet_headers.push(packet_header);
    }

    patch_rsg_info(&mut output, &header, &rsg_infos)?;
    patch_autopool_info(
        &mut output,
        &header,
        &autopool_infos,
        &rsg_infos,
        &packet_headers,
    )?;
    output.seek(SeekFrom::Start(0))?;
    RsbWriter::new(&mut output).write_header(&header)?;
    Ok(output.into_inner())
}

#[derive(Clone, Copy)]
enum StructuralPacketSource<'a> {
    Original {
        old_index: usize,
        edit: Option<&'a RsgPacketEdit>,
    },
    Added(&'a RsgPacketAddition),
}

fn validate_structural_record_sizes(header: &RsbHeader) -> crate::Result<()> {
    if header.rsg_info_each_length != 204 {
        return Err(RsbError::Other(format!(
            "structural RSG edits require 204-byte RSG records (found {})",
            header.rsg_info_each_length
        )));
    }
    if header.composite_info_each_length != 1156 && header.composite_number != 0 {
        return Err(RsbError::Other(format!(
            "structural RSG edits require 1156-byte composite records (found {})",
            header.composite_info_each_length
        )));
    }
    if header.autopool_info_each_length != 152 {
        return Err(RsbError::Other(format!(
            "structural RSG edits require 152-byte autopool records (found {})",
            header.autopool_info_each_length
        )));
    }
    Ok(())
}

fn validate_removed_packets(
    edit: &RsbArchiveEdit,
    infos: &[RsgInfo],
    edits: &HashMap<usize, &RsgPacketEdit>,
) -> crate::Result<HashSet<usize>> {
    let mut removed = HashSet::with_capacity(edit.removed_packets.len());
    for &packet_index in &edit.removed_packets {
        let Some(_info) = infos.get(packet_index) else {
            return Err(RsbError::Other(format!(
                "removed RSG packet index {packet_index} is out of range"
            )));
        };
        if edits.contains_key(&packet_index) {
            return Err(RsbError::Other(format!(
                "RSG packet {packet_index} cannot be edited and removed in the same rebuild"
            )));
        }
        if !removed.insert(packet_index) {
            return Err(RsbError::Other(format!(
                "RSG packet {packet_index} is removed more than once"
            )));
        }
    }
    Ok(removed)
}

fn validate_added_packets(additions: &[RsgPacketAddition]) -> crate::Result<()> {
    for addition in additions {
        validate_packet_identity(&addition.name, addition.version, addition.compression_flags)?;
        validate_file_paths(&addition.name, &addition.files)?;
        validate_texture_ids(&addition.name, &addition.files)?;
        if let Some(group) = &addition.group {
            if group.name.is_empty() || group.name.len() >= RSG_INFO_NAME_LENGTH {
                return Err(RsbError::Other(format!(
                    "RSG group name must contain 1..{} UTF-8 bytes",
                    RSG_INFO_NAME_LENGTH - 1
                )));
            }
            if group.category[1].len() > 4 {
                return Err(RsbError::Other(
                    "RSG group locale category must contain at most 4 UTF-8 bytes".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_packet_names(
    infos: &[RsgInfo],
    edits: &HashMap<usize, &RsgPacketEdit>,
    removed: &HashSet<usize>,
    additions: &[RsgPacketAddition],
) -> crate::Result<()> {
    let mut names = HashSet::new();
    for (packet_index, info) in infos.iter().enumerate() {
        if removed.contains(&packet_index) {
            continue;
        }
        let name = edits
            .get(&packet_index)
            .map_or(info.name.as_str(), |edit| edit.name.as_str());
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(RsbError::Other(format!(
                "RSG packet name {name} collides with another archive entry"
            )));
        }
    }
    for addition in additions {
        if !names.insert(addition.name.to_ascii_lowercase()) {
            return Err(RsbError::Other(format!(
                "RSG packet name {} collides with another archive entry",
                addition.name
            )));
        }
    }
    Ok(())
}

fn rebuild_resource_file_list(
    original: Vec<FileListInfo>,
    original_infos: &[RsgInfo],
    edits: &HashMap<usize, &RsgPacketEdit>,
    removed: &HashSet<usize>,
    old_pool_to_new: &HashMap<i32, usize>,
    additions: &[RsgPacketAddition],
    original_packet_count: usize,
) -> crate::Result<Vec<FileListInfo>> {
    let pool_to_packet = original_infos
        .iter()
        .enumerate()
        .map(|(index, info)| (info.pool_index, index))
        .collect::<HashMap<_, _>>();
    let removed_paths = edits
        .values()
        .flat_map(|edit| edit.original_paths.iter())
        .map(|path| normalize_path(path).to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut output = Vec::with_capacity(original.len());
    for entry in original {
        let Some(&old_packet_index) = pool_to_packet.get(&entry.pool_index) else {
            return Err(RsbError::Other(format!(
                "resource path {} refers to unknown pool {}",
                entry.name_path, entry.pool_index
            )));
        };
        if removed.contains(&old_packet_index)
            || removed_paths.contains(&normalize_path(&entry.name_path).to_ascii_lowercase())
        {
            continue;
        }
        let Some(&new_index) = old_pool_to_new.get(&entry.pool_index) else {
            return Err(RsbError::Other(format!(
                "resource path {} could not be remapped from pool {}",
                entry.name_path, entry.pool_index
            )));
        };
        output.push(FileListInfo {
            name_path: normalize_path(&entry.name_path),
            pool_index: i32::try_from(new_index)
                .map_err(|_| RsbError::Other("RSG packet count exceeds i32".into()))?,
        });
    }
    for (old_index, packet_edit) in edits {
        let old_pool = original_infos[*old_index].pool_index;
        let new_index = old_pool_to_new[&old_pool];
        output.extend(packet_edit.files.iter().map(|file| FileListInfo {
            name_path: normalize_path(&file.path),
            pool_index: i32::try_from(new_index).unwrap_or(i32::MAX),
        }));
    }
    let retained_count = original_packet_count.saturating_sub(removed.len());
    for (addition_index, addition) in additions.iter().enumerate() {
        let new_index = retained_count.saturating_add(addition_index);
        output.extend(addition.files.iter().map(|file| FileListInfo {
            name_path: normalize_path(&file.path),
            pool_index: i32::try_from(new_index).unwrap_or(i32::MAX),
        }));
    }
    validate_and_sort_file_list(output)
}

fn rebuild_composite_infos(
    original: Vec<CompositeInfo>,
    old_to_new: &HashMap<usize, usize>,
    additions: &[RsgPacketAddition],
) -> crate::Result<Vec<CompositeInfo>> {
    let mut output = Vec::with_capacity(original.len().saturating_add(additions.len()));
    for mut info in original {
        let mut packets = Vec::with_capacity(info.packet_info.len());
        for packet in info.packet_info {
            let Ok(old_index) = usize::try_from(packet.packet_index) else {
                return Err(RsbError::Other(format!(
                    "composite group {} contains a negative packet index",
                    info.name
                )));
            };
            let Some(&new_index) = old_to_new.get(&old_index) else {
                continue;
            };
            packets.push(CompositePacketInfo {
                packet_index: i32::try_from(new_index)
                    .map_err(|_| RsbError::Other("RSG packet count exceeds i32".into()))?,
                category: packet.category,
            });
        }
        if packets.is_empty() {
            continue;
        }
        info.packet_number = u32::try_from(packets.len())
            .map_err(|_| RsbError::Other("composite packet count exceeds u32".into()))?;
        info.packet_info = packets;
        output.push(info);
    }

    let mut group_names = output
        .iter()
        .map(|group| composite_storage_name(group).to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let first_added_index = old_to_new.len();
    for (addition_index, addition) in additions.iter().enumerate() {
        let Some(group) = &addition.group else {
            continue;
        };
        let info = CompositeInfo {
            name: group.name.clone(),
            is_composite: group.is_composite,
            packet_number: 1,
            packet_info: vec![CompositePacketInfo {
                packet_index: i32::try_from(first_added_index.saturating_add(addition_index))
                    .map_err(|_| RsbError::Other("RSG packet count exceeds i32".into()))?,
                category: group.category.clone(),
            }],
        };
        let storage_name = composite_storage_name(&info);
        if !group_names.insert(storage_name.to_ascii_lowercase()) {
            return Err(RsbError::Other(format!(
                "RSG group name {storage_name} collides with another archive group"
            )));
        }
        output.push(info);
    }
    Ok(output)
}

fn composite_storage_name(info: &CompositeInfo) -> String {
    if info.is_composite {
        info.name.clone()
    } else {
        format!("{}_CompositeShell", info.name)
    }
}

fn packed_packet(
    files: &[UnpackedFile],
    version: u32,
    compression_flags: u32,
) -> crate::Result<Vec<u8>> {
    let mut packet = Cursor::new(Vec::new());
    pack_rsg(&mut packet, files, version, compression_flags)?;
    Ok(packet.into_inner())
}

fn validate_ptx_infos(
    replacement: Option<&[RsbPtxInfo]>,
    original: &[RsbPtxInfo],
    expected_count: usize,
) -> crate::Result<()> {
    let actual_count = replacement.map_or(original.len(), <[RsbPtxInfo]>::len);
    if actual_count != expected_count {
        return Err(RsbError::Other(format!(
            "global PTX metadata contains {actual_count} records, but edited packets require {expected_count}"
        )));
    }
    Ok(())
}

fn structural_ptx_infos(
    replacement: Option<&[RsbPtxInfo]>,
    original: &[RsbPtxInfo],
    original_packets: &[RsgInfo],
    removed_packets: &HashSet<usize>,
    expected_count: usize,
) -> crate::Result<Vec<RsbPtxInfo>> {
    let mut infos = if let Some(replacement) = replacement {
        replacement.to_vec()
    } else if removed_packets.is_empty() {
        original.to_vec()
    } else {
        let mut removed_textures = vec![false; original.len()];
        for &packet_index in removed_packets {
            let packet = &original_packets[packet_index];
            let start = usize::try_from(packet.ptx_before_number).map_err(|_| {
                RsbError::Other(format!(
                    "RSG packet {} PTX begin index does not fit in memory",
                    packet.name
                ))
            })?;
            let count = usize::try_from(packet.ptx_number).map_err(|_| {
                RsbError::Other(format!(
                    "RSG packet {} PTX count does not fit in memory",
                    packet.name
                ))
            })?;
            let end = start.checked_add(count).ok_or_else(|| {
                RsbError::Other(format!("RSG packet {} PTX range overflow", packet.name))
            })?;
            let Some(range) = removed_textures.get_mut(start..end) else {
                return Err(RsbError::Other(format!(
                    "RSG packet {} PTX range {start}..{end} is outside the global table",
                    packet.name
                )));
            };
            range.fill(true);
        }
        original
            .iter()
            .enumerate()
            .filter(|(index, _)| !removed_textures[*index])
            .map(|(_, info)| info.clone())
            .collect()
    };

    if infos.len() != expected_count {
        return Err(RsbError::Other(format!(
            "global PTX metadata contains {} records, but rebuilt packets require {expected_count}; provide the complete resized RsbArchiveEdit::ptx_infos table",
            infos.len()
        )));
    }
    for (index, info) in infos.iter_mut().enumerate() {
        info.ptx_index = i32::try_from(index)
            .map_err(|_| RsbError::Other("global PTX count exceeds i32".into()))?;
    }
    Ok(infos)
}

fn validate_packet_edits<'a>(
    edit: &'a RsbArchiveEdit,
    infos: &[RsgInfo],
) -> crate::Result<HashMap<usize, &'a RsgPacketEdit>> {
    let mut edits = HashMap::with_capacity(edit.packets.len());
    for packet in &edit.packets {
        let Some(_info) = infos.get(packet.packet_index) else {
            return Err(RsbError::Other(format!(
                "RSG packet index {} is out of range",
                packet.packet_index
            )));
        };
        validate_packet_identity(&packet.name, packet.version, packet.compression_flags)?;
        validate_packet_files(packet)?;
        if edits.insert(packet.packet_index, packet).is_some() {
            return Err(RsbError::Other(format!(
                "RSG packet {} has more than one edit",
                packet.packet_index
            )));
        }
    }
    Ok(edits)
}

fn validate_packet_identity(name: &str, version: u32, compression_flags: u32) -> crate::Result<()> {
    if name.is_empty() || name.len() >= RSG_INFO_NAME_LENGTH {
        return Err(RsbError::Other(format!(
            "RSG packet name must contain 1..{} UTF-8 bytes",
            RSG_INFO_NAME_LENGTH - 1
        )));
    }
    if version != 3 && version != 4 {
        return Err(RsbError::InvalidVersion(version));
    }
    if compression_flags > 3 {
        return Err(RsbError::InvalidCompression(compression_flags));
    }
    Ok(())
}

fn validate_file_paths(packet_name: &str, files: &[UnpackedFile]) -> crate::Result<()> {
    let mut paths = HashSet::with_capacity(files.len());
    for file in files {
        let path = normalize_path(&file.path);
        if path.is_empty() {
            return Err(RsbError::Other("RSG file path cannot be empty".into()));
        }
        if !paths.insert(path.clone()) {
            return Err(RsbError::Other(format!(
                "RSG packet {packet_name} contains duplicate path {path}"
            )));
        }
    }
    Ok(())
}

fn validate_packet_files(packet: &RsgPacketEdit) -> crate::Result<()> {
    validate_file_paths(&packet.name, &packet.files)?;
    validate_texture_ids(&packet.name, &packet.files)
}

fn validate_texture_ids(packet_name: &str, files: &[UnpackedFile]) -> crate::Result<()> {
    let mut texture_ids = HashSet::new();
    for file in files {
        if file.is_part1 {
            let texture = file
                .part1_info
                .as_ref()
                .ok_or_else(|| RsbError::MissingPart1Info(file.path.clone()))?;
            if !texture_ids.insert(texture.id) {
                return Err(RsbError::Other(format!(
                    "RSG packet contains duplicate texture ID {}",
                    texture.id
                )));
            }
        }
    }
    let texture_count = u32::try_from(texture_ids.len())
        .map_err(|_| RsbError::Other("RSG texture count exceeds u32".into()))?;
    if !(0..texture_count).all(|id| texture_ids.contains(&id)) {
        return Err(RsbError::Other(format!(
            "RSG packet {} must use contiguous Part-1 texture IDs from 0 through {}",
            packet_name,
            texture_count.saturating_sub(1)
        )));
    }
    Ok(())
}

fn texture_count(files: &[UnpackedFile]) -> usize {
    files.iter().filter(|file| file.is_part1).count()
}

fn metadata_source_end(
    source: &[u8],
    header: &RsbHeader,
    infos: &[RsgInfo],
) -> crate::Result<usize> {
    let first_packet = infos
        .iter()
        .map(|info| info.rsg_offset)
        .min()
        .unwrap_or(header.information_section_size);
    let end = usize::try_from(first_packet)
        .map_err(|_| RsbError::Other("RSB metadata offset does not fit in memory".into()))?;
    if end > source.len() || first_packet < header.information_section_size {
        return Err(RsbError::Other(
            "RSB metadata or first packet offset is outside the source archive".into(),
        ));
    }
    Ok(end)
}

fn update_resource_file_list(
    original: Vec<FileListInfo>,
    edits: &HashMap<usize, &RsgPacketEdit>,
    infos: &[RsgInfo],
) -> crate::Result<Vec<FileListInfo>> {
    let removed = edits
        .values()
        .flat_map(|edit| edit.original_paths.iter())
        .map(|path| normalize_path(path))
        .collect::<HashSet<_>>();
    let mut output = original
        .into_iter()
        .filter(|entry| !removed.contains(&normalize_path(&entry.name_path)))
        .collect::<Vec<_>>();
    for (packet_index, edit) in edits {
        let pool_index = infos[*packet_index].pool_index;
        output.extend(edit.files.iter().map(|file| FileListInfo {
            name_path: normalize_path(&file.path),
            pool_index,
        }));
    }
    validate_and_sort_file_list(output)
}

fn validate_and_sort_file_list(mut output: Vec<FileListInfo>) -> crate::Result<Vec<FileListInfo>> {
    output.sort_by(|left, right| {
        normalize_path(&left.name_path)
            .to_ascii_lowercase()
            .cmp(&normalize_path(&right.name_path).to_ascii_lowercase())
    });
    for pair in output.windows(2) {
        if normalize_path(&pair[0].name_path)
            .eq_ignore_ascii_case(&normalize_path(&pair[1].name_path))
        {
            return Err(RsbError::Other(format!(
                "resource path {} collides with another archive entry",
                pair[1].name_path
            )));
        }
    }
    Ok(output)
}

fn packet_slice<'a>(source: &'a [u8], info: &RsgInfo) -> crate::Result<&'a [u8]> {
    let start = usize::try_from(info.rsg_offset)
        .map_err(|_| RsbError::Other("RSG offset does not fit in memory".into()))?;
    let length = usize::try_from(info.rsg_length)
        .map_err(|_| RsbError::Other("RSG length does not fit in memory".into()))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| RsbError::Other("RSG source range overflow".into()))?;
    source
        .get(start..end)
        .ok_or_else(|| RsbError::Other(format!("RSG packet {} is outside the archive", info.name)))
}

fn packet_head_info(packet: &[u8]) -> crate::Result<Vec<u8>> {
    packet
        .get(RSG_PACKET_HEAD_OFFSET..RSG_PACKET_HEAD_OFFSET + RSG_INFO_HEAD_LENGTH)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| RsbError::Other("rebuilt RSG packet is shorter than its header".into()))
}

fn align_cursor(cursor: &mut Cursor<Vec<u8>>, alignment: usize) -> crate::Result<()> {
    let position = usize::try_from(cursor.position())
        .map_err(|_| RsbError::Other("archive position does not fit in memory".into()))?;
    let padding = (alignment - position % alignment) % alignment;
    cursor.write_all(&vec![0; padding])?;
    Ok(())
}

fn patch_rsg_info(
    output: &mut Cursor<Vec<u8>>,
    header: &RsbHeader,
    infos: &[RsgInfo],
) -> crate::Result<()> {
    let each_length = usize::try_from(header.rsg_info_each_length)
        .map_err(|_| RsbError::Other("RSG info record size does not fit in memory".into()))?;
    if each_length < RSG_INFO_NAME_LENGTH + 12 + RSG_INFO_HEAD_LENGTH + 8 {
        return Err(RsbError::Other("RSG info record is too short".into()));
    }
    for (index, info) in infos.iter().enumerate() {
        let start = u64::from(header.rsg_info_begin_offset)
            .checked_add((index * each_length) as u64)
            .ok_or_else(|| RsbError::Other("RSG info offset overflow".into()))?;
        output.seek(SeekFrom::Start(start))?;
        write_fixed_string(output, &info.name, RSG_INFO_NAME_LENGTH)?;
        output.write_u32::<LE>(info.rsg_offset)?;
        output.write_u32::<LE>(info.rsg_length)?;
        output.write_i32::<LE>(info.pool_index)?;
        let head = info.packet_head_info.as_deref().ok_or_else(|| {
            RsbError::Other(format!("RSG packet {} has no header info", info.name))
        })?;
        if head.len() < RSG_INFO_HEAD_LENGTH {
            return Err(RsbError::Other(format!(
                "RSG packet {} has truncated header info",
                info.name
            )));
        }
        output.write_all(&head[..RSG_INFO_HEAD_LENGTH])?;
        output.seek(SeekFrom::Start(start + each_length as u64 - 8))?;
        output.write_u32::<LE>(info.ptx_number)?;
        output.write_u32::<LE>(info.ptx_before_number)?;
    }
    Ok(())
}

fn patch_autopool_info(
    output: &mut Cursor<Vec<u8>>,
    header: &RsbHeader,
    original: &[AutoPoolInfo],
    infos: &[RsgInfo],
    packet_headers: &[RsgHeader],
) -> crate::Result<()> {
    let each_length = usize::try_from(header.autopool_info_each_length)
        .map_err(|_| RsbError::Other("autopool record size does not fit in memory".into()))?;
    if each_length < AUTOPOOL_NAME_LENGTH + 8 {
        return Ok(());
    }
    for (info, packet_header) in infos.iter().zip(packet_headers) {
        let Ok(pool_index) = usize::try_from(info.pool_index) else {
            continue;
        };
        if pool_index >= original.len() {
            continue;
        }
        let start = u64::from(header.autopool_info_begin_offset)
            .checked_add((pool_index * each_length) as u64)
            .ok_or_else(|| RsbError::Other("autopool info offset overflow".into()))?;
        output.seek(SeekFrom::Start(start))?;
        write_fixed_string(output, &original[pool_index].name, AUTOPOOL_NAME_LENGTH)?;
        output.write_u32::<LE>(packet_header.part0_size)?;
        output.write_u32::<LE>(packet_header.part1_size)?;
    }
    Ok(())
}

fn patch_ptx_info(
    output: &mut Cursor<Vec<u8>>,
    header: &RsbHeader,
    infos: &[RsbPtxInfo],
) -> crate::Result<()> {
    output.seek(SeekFrom::Start(u64::from(header.ptx_info_begin_offset)))?;
    let mut writer = RsbWriter::new(output);
    writer.write_ptx_info(infos, header.ptx_info_each_length)?;
    Ok(())
}

fn write_fixed_string(writer: &mut impl Write, value: &str, length: usize) -> crate::Result<()> {
    if value.len() >= length {
        return Err(RsbError::Other(format!(
            "string is too long for a {length}-byte fixed field"
        )));
    }
    writer.write_all(value.as_bytes())?;
    writer.write_all(&vec![0; length - value.len()])?;
    Ok(())
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|component| !component.is_empty() && *component != "." && *component != "..")
        .collect::<Vec<_>>()
        .join("/")
}
