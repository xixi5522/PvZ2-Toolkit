use crate::{PatchError, Result, RsbPatch, delta};

use super::{
    ArchiveDecodeOptions, PacketPatchMode, invalid, raw_packet, view::ArchiveView,
    view::index_packets,
};

/// Apply an RSBP patch to an RSB v4 archive and reconstruct the target archive.
pub fn apply_archive_patch(
    before: &[u8],
    patch: &RsbPatch,
    options: ArchiveDecodeOptions,
) -> Result<Vec<u8>> {
    let before_archive = ArchiveView::parse(before, "source")?;
    let target_information = delta::apply(
        "RSB information section",
        before_archive.information,
        patch.before_hash,
        patch.information_patch.as_deref(),
        options.vcdiff,
    )?;
    let mut target_archive = ArchiveView::parse(&target_information, "patched target")?;
    if patch.packets.len() != target_archive.packets.len() {
        return Err(PatchError::PacketCountMismatch {
            patch: patch.packets.len(),
            target: target_archive.packets.len(),
        });
    }
    let before_by_name = index_packets(&before_archive.packets)?;
    let capacity = match options.packet_mode {
        PacketPatchMode::Stored => usize::try_from(patch.all_after_size)
            .map_err(|_| invalid("target RSB size does not fit in memory"))?,
        PacketPatchMode::Raw => target_information.len(),
    };
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&target_information);

    for (index, (packet_patch, target_info)) in patch
        .packets
        .iter()
        .zip(target_archive.packets.iter_mut())
        .enumerate()
    {
        if !packet_patch.name.eq_ignore_ascii_case(&target_info.name) {
            return Err(PatchError::PacketNameMismatch {
                index,
                patch: packet_patch.name.clone(),
                target: target_info.name.clone(),
            });
        }
        let source_stored = before_by_name
            .get(packet_patch.name.as_str())
            .map_or(Ok(&[][..]), |source_index| {
                before_archive.packet_bytes(before, *source_index)
            })?;
        let source = raw_packet::delta_input(source_stored, options.packet_mode)?;
        let reconstructed = delta::apply(
            format!("packet {}", packet_patch.name),
            source.as_ref(),
            packet_patch.before_hash,
            packet_patch.patch.as_deref(),
            options.vcdiff,
        )?;
        let stored = match options.packet_mode {
            PacketPatchMode::Stored => reconstructed,
            PacketPatchMode::Raw => raw_packet::compress(&reconstructed)?,
        };

        if options.packet_mode == PacketPatchMode::Stored {
            validate_stored_size(&packet_patch.name, stored.len(), target_info.rsg_length)?;
        } else {
            target_info.rsg_offset = u32::try_from(output.len())
                .map_err(|_| invalid("rebuilt RSB offset exceeds u32"))?;
            target_info.rsg_length =
                u32::try_from(stored.len()).map_err(|_| invalid("rebuilt RSG size exceeds u32"))?;
            target_archive
                .subgroups
                .update(&mut output, index, target_info, &stored)?;
        }
        output.extend_from_slice(&stored);
    }

    if options.packet_mode == PacketPatchMode::Stored && output.len() != capacity {
        return Err(invalid(format!(
            "reconstructed RSB has {} bytes, patch declares {capacity}",
            output.len()
        )));
    }
    Ok(output)
}

fn validate_stored_size(name: &str, actual: usize, expected: u32) -> Result<()> {
    let expected = usize::try_from(expected)
        .map_err(|_| invalid("target RSG length does not fit in memory"))?;
    if actual != expected {
        return Err(invalid(format!(
            "packet {name} reconstructed to {actual} bytes, target metadata declares {expected}"
        )));
    }
    Ok(())
}
