use crate::{PacketPatch, Result, RsbPatch, delta};

use super::{ArchiveEncodeOptions, raw_packet, view::ArchiveView, view::index_packets};

/// Create a complete RSBP patch from two RSB v4 archives.
pub fn create_archive_patch(
    before: &[u8],
    after: &[u8],
    options: ArchiveEncodeOptions,
) -> Result<RsbPatch> {
    let before_archive = ArchiveView::parse(before, "source")?;
    let after_archive = ArchiveView::parse(after, "target")?;
    let before_by_name = index_packets(&before_archive.packets)?;
    let (before_hash, information_patch) = delta::create(
        before_archive.information,
        after_archive.information,
        options.vcdiff,
    )?;

    let mut packets = Vec::with_capacity(after_archive.packets.len());
    for target_info in &after_archive.packets {
        let source_stored = before_by_name
            .get(target_info.name.as_str())
            .map_or(Ok(&[][..]), |index| {
                before_archive.packet_bytes(before, *index)
            })?;
        let target_stored = after_archive.packet_bytes_by_info(after, target_info)?;
        let source = raw_packet::delta_input(source_stored, options.packet_mode)?;
        let target = raw_packet::delta_input(target_stored, options.packet_mode)?;
        let (before_hash, patch) = delta::create(source.as_ref(), target.as_ref(), options.vcdiff)?;
        packets.push(PacketPatch {
            name: target_info.name.clone(),
            before_hash,
            patch,
        });
    }

    Ok(RsbPatch {
        all_after_size: after_archive.end,
        before_hash,
        information_patch,
        packets,
    })
}
