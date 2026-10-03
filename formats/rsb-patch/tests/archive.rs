#![cfg(feature = "rsb")]

use std::io::{Cursor, Seek, SeekFrom, Write};

use rsb_archive::{
    AutoPoolInfo, FileListInfo, RSB_MAGIC, Rsb, RsbArchiveEdit, RsbHeader, RsbWriter, RsgInfo,
    RsgPacketAddition, RsgPacketEdit, RsgPacketGroup, UnpackedFile, pack_rsg, rebuild_rsb,
    unpack_rsg,
};
use rsb_patch::{
    ArchiveDecodeOptions, ArchiveEncodeOptions, RsbPatch, apply_archive_patch, create_archive_patch,
};

const PACKET_OFFSET: u32 = 0x1000;

#[test]
fn stored_mode_reconstructs_a_changed_archive_byte_exactly() {
    let before = sample_archive();
    let after = edit_sample(&before, b"changed packet contents");
    let patch = RsbPatch::create(&before, &after, ArchiveEncodeOptions::stored()).unwrap();
    let serialized = patch.to_bytes().unwrap();
    let patch = RsbPatch::read(serialized.as_slice()).unwrap();
    let reconstructed = patch
        .apply_to_archive(&before, ArchiveDecodeOptions::stored())
        .unwrap();
    assert_eq!(reconstructed, after);
}

#[test]
fn stored_mode_supports_removed_and_new_packets() {
    let before = sample_archive();
    let after = rebuild_rsb(
        &before,
        &RsbArchiveEdit {
            removed_packets: vec![0],
            added_packets: vec![RsgPacketAddition {
                name: "Replacement_Common".into(),
                version: 4,
                compression_flags: 3,
                files: vec![sample_file("DATA/REPLACEMENT.RTON", b"replacement")],
                group: Some(RsgPacketGroup {
                    name: "Replacement_Common".into(),
                    is_composite: false,
                    category: ["0".into(), String::new()],
                }),
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let patch = create_archive_patch(&before, &after, ArchiveEncodeOptions::stored()).unwrap();
    assert_eq!(patch.packets.len(), 1);
    assert_eq!(patch.packets[0].name, "Replacement_Common");
    assert_eq!(
        apply_archive_patch(&before, &patch, ArchiveDecodeOptions::stored()).unwrap(),
        after
    );
}

#[test]
fn raw_mode_recompresses_and_rewrites_packet_metadata() {
    let before = sample_archive();
    let after = edit_sample(&before, b"raw packet contents with a different length");
    let patch = create_archive_patch(&before, &after, ArchiveEncodeOptions::raw()).unwrap();
    let reconstructed = apply_archive_patch(&before, &patch, ArchiveDecodeOptions::raw()).unwrap();

    let mut archive = Rsb::open(Cursor::new(&reconstructed)).unwrap();
    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].rsg_offset, archive.header.information_section_size);
    assert_eq!(infos[0].rsg_offset % 0x1000, 0);
    assert_eq!(infos[0].rsg_length % 0x1000, 0);
    let files = unpack_rsg(&mut Cursor::new(archive.extract_packet(&infos[0]).unwrap())).unwrap();
    assert_eq!(
        files[0].data,
        b"raw packet contents with a different length"
    );
}

fn edit_sample(source: &[u8], contents: &[u8]) -> Vec<u8> {
    rebuild_rsb(
        source,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index: 0,
                original_paths: vec!["DATA/OLD.RTON".into()],
                name: "Sample_Common".into(),
                version: 4,
                compression_flags: 3,
                files: vec![sample_file("DATA/OLD.RTON", contents)],
            }],
            ..Default::default()
        },
    )
    .unwrap()
}

fn sample_file(path: &str, data: &[u8]) -> UnpackedFile {
    UnpackedFile {
        path: path.into(),
        data: data.into(),
        is_part1: false,
        part1_info: None,
    }
}

fn sample_archive() -> Vec<u8> {
    let mut packet = Cursor::new(Vec::new());
    pack_rsg(
        &mut packet,
        &[sample_file("DATA/OLD.RTON", b"old packet contents")],
        4,
        3,
    )
    .unwrap();
    let mut packet = packet.into_inner();
    let packet_head = packet[16..48].to_vec();
    packet.resize(packet.len().next_multiple_of(0x1000), 0);

    let mut header = RsbHeader {
        magic: RSB_MAGIC,
        version: 4,
        information_section_size: PACKET_OFFSET,
        resource_path_section_size: 0,
        resource_path_section_offset: 0x100,
        rsg_list_length: 0,
        rsg_list_begin_offset: 0x200,
        rsg_number: 1,
        rsg_info_begin_offset: 0x300,
        rsg_info_each_length: 204,
        composite_number: 0,
        composite_info_begin_offset: 0,
        composite_info_each_length: 1156,
        composite_list_length: 0,
        composite_list_begin_offset: 0,
        autopool_number: 1,
        autopool_info_begin_offset: 0x500,
        autopool_info_each_length: 152,
        ptx_number: 0,
        ptx_info_begin_offset: 0x600,
        ptx_info_each_length: 16,
        part1_begin_offset: 0,
        part2_begin_offset: 0,
        part3_begin_offset: 0,
        information_without_manifest_section_size: PACKET_OFFSET,
    };
    let info = RsgInfo {
        name: "Sample_Common".into(),
        rsg_offset: PACKET_OFFSET,
        rsg_length: packet.len() as u32,
        pool_index: 0,
        ptx_number: 0,
        ptx_before_number: 0,
        packet_head_info: Some(packet_head),
    };

    let mut output = Cursor::new(vec![0; PACKET_OFFSET as usize]);
    output.seek(SeekFrom::Start(0x100)).unwrap();
    let (offset, size) = RsbWriter::new(&mut output)
        .write_file_list(&[FileListInfo {
            name_path: "DATA/OLD.RTON".into(),
            pool_index: 0,
        }])
        .unwrap();
    header.resource_path_section_offset = offset;
    header.resource_path_section_size = size;
    output.seek(SeekFrom::Start(0x200)).unwrap();
    let (offset, size) = RsbWriter::new(&mut output)
        .write_file_list(&[FileListInfo {
            name_path: info.name.clone(),
            pool_index: 0,
        }])
        .unwrap();
    header.rsg_list_begin_offset = offset;
    header.rsg_list_length = size;
    output.seek(SeekFrom::Start(0x300)).unwrap();
    RsbWriter::new(&mut output)
        .write_rsg_info(std::slice::from_ref(&info), &[(0, 0)])
        .unwrap();
    output.seek(SeekFrom::Start(0x500)).unwrap();
    RsbWriter::new(&mut output)
        .write_autopool_info(&[AutoPoolInfo {
            name: "Sample_Common_AutoPool".into(),
            part0_size: 0x1000,
            part1_size: 0,
        }])
        .unwrap();
    output.seek(SeekFrom::Start(0)).unwrap();
    RsbWriter::new(&mut output).write_header(&header).unwrap();
    output
        .seek(SeekFrom::Start(u64::from(PACKET_OFFSET)))
        .unwrap();
    output.write_all(&packet).unwrap();
    output.into_inner()
}
