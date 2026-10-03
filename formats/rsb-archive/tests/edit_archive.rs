use rsb_archive::{
    AutoPoolInfo, FileListInfo, Part1Extra, RSB_MAGIC, RSG_MAGIC, Rsb, RsbArchiveEdit, RsbHeader,
    RsbPtxInfo, RsbWriter, RsgInfo, RsgPacketAddition, RsgPacketEdit, RsgPacketGroup, UnpackedFile,
    pack_rsg, rebuild_rsb, unpack_rsg,
};
use std::io::{Cursor, Seek, SeekFrom, Write};

const PACKET_OFFSET: u32 = 0x1000;

fn pad_to(bytes: &mut Vec<u8>, length: usize) {
    bytes.resize(length, 0);
}

fn sample_file(path: &str, data: &[u8]) -> UnpackedFile {
    UnpackedFile {
        path: path.into(),
        data: data.into(),
        is_part1: false,
        part1_info: None,
    }
}

fn sample_texture(path: &str, id: u32, width: u32, height: u32) -> UnpackedFile {
    UnpackedFile {
        path: path.into(),
        data: vec![0; usize::try_from(width * height * 4).unwrap()],
        is_part1: true,
        part1_info: Some(Part1Extra { id, width, height }),
    }
}

fn sample_archive() -> Vec<u8> {
    let files = vec![
        sample_file("DATA/OLD.RTON", b"old"),
        sample_file("DATA/REMOVE.TXT", b"remove"),
    ];
    let mut packet = Cursor::new(Vec::new());
    pack_rsg(&mut packet, &files, 4, 3).unwrap();
    let mut packet = packet.into_inner();
    let packet_head = packet[16..48].to_vec();
    let packet_length = packet.len().next_multiple_of(0x1000);
    pad_to(&mut packet, packet_length);

    let header = RsbHeader {
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
        rsg_length: packet_length as u32,
        pool_index: 0,
        ptx_number: 0,
        ptx_before_number: 0,
        packet_head_info: Some(packet_head),
    };

    let mut output = Cursor::new(vec![0; PACKET_OFFSET as usize]);
    output.seek(SeekFrom::Start(0x100)).unwrap();
    let (file_list_offset, file_list_length) = RsbWriter::new(&mut output)
        .write_file_list(&[
            FileListInfo {
                name_path: "DATA/OLD.RTON".into(),
                pool_index: 0,
            },
            FileListInfo {
                name_path: "DATA/REMOVE.TXT".into(),
                pool_index: 0,
            },
        ])
        .unwrap();
    output.seek(SeekFrom::Start(0x200)).unwrap();
    let (rsg_list_offset, rsg_list_length) = RsbWriter::new(&mut output)
        .write_file_list(&[FileListInfo {
            name_path: info.name.clone(),
            pool_index: 0,
        }])
        .unwrap();
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

    let mut header = header;
    header.resource_path_section_offset = file_list_offset;
    header.resource_path_section_size = file_list_length;
    header.rsg_list_begin_offset = rsg_list_offset;
    header.rsg_list_length = rsg_list_length;
    output.seek(SeekFrom::Start(0)).unwrap();
    RsbWriter::new(&mut output).write_header(&header).unwrap();
    output
        .seek(SeekFrom::Start(u64::from(PACKET_OFFSET)))
        .unwrap();
    output.write_all(&packet).unwrap();
    output.into_inner()
}

#[test]
fn rebuilds_packet_contents_and_archive_indexes() {
    let source = sample_archive();
    let replacement = RsgPacketEdit {
        packet_index: 0,
        original_paths: vec!["DATA/OLD.RTON".into(), "DATA/REMOVE.TXT".into()],
        name: "Sample_Renamed".into(),
        version: 4,
        compression_flags: 2,
        files: vec![
            sample_file("DATA/RENAMED.RTON", b"new contents"),
            sample_file("DATA/ADDED.BIN", &[1, 2, 3, 4]),
        ],
    };
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            packets: vec![replacement],
            ptx_infos: None,
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.magic, RSB_MAGIC);
    assert!(archive.header.information_section_size > PACKET_OFFSET);
    assert_eq!(
        archive.header.information_without_manifest_section_size,
        archive.header.information_section_size
    );

    let file_list = archive.read_file_list().unwrap();
    let paths = file_list
        .iter()
        .map(|entry| entry.name_path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"DATA/RENAMED.RTON"));
    assert!(paths.contains(&"DATA/ADDED.BIN"));
    assert!(!paths.contains(&"DATA/OLD.RTON"));
    assert!(!paths.contains(&"DATA/REMOVE.TXT"));

    let info = archive.read_rsg_info().unwrap().remove(0);
    assert_eq!(info.name, "Sample_Renamed");
    assert_eq!(
        archive.read_autopool_info().unwrap().remove(0).name,
        "Sample_Renamed_AutoPool"
    );
    assert_eq!(info.rsg_offset, archive.header.information_section_size);
    assert_eq!(info.rsg_offset % 0x1000, 0);
    assert_eq!(info.rsg_length % 0x1000, 0);
    let raw = archive.extract_packet(&info).unwrap();
    assert_eq!(&raw[..4], RSG_MAGIC.to_le_bytes().as_slice());

    let mut raw_cursor = Cursor::new(raw);
    let files = unpack_rsg(&mut raw_cursor).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files
            .iter()
            .find(|file| file.path == "DATA/RENAMED.RTON")
            .unwrap()
            .data,
        b"new contents"
    );
    assert_eq!(
        files
            .iter()
            .find(|file| file.path == "DATA/ADDED.BIN")
            .unwrap()
            .data,
        [1, 2, 3, 4]
    );
}

#[test]
fn adds_and_removes_part0_rsg_packets_and_reindexes_tables() {
    let source = sample_archive();
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            added_packets: vec![RsgPacketAddition {
                name: "Added_Common".into(),
                version: 4,
                compression_flags: 2,
                files: vec![sample_file("DATA/ADDED.RTON", b"added packet")],
                group: Some(RsgPacketGroup {
                    name: "Added_Common".into(),
                    is_composite: false,
                    category: ["0".into(), String::new()],
                }),
            }],
            removed_packets: vec![0],
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.rsg_number, 1);
    assert_eq!(archive.header.autopool_number, 1);
    assert_eq!(archive.header.composite_number, 1);
    let file_list = archive.read_file_list().unwrap();
    assert_eq!(file_list.len(), 1);
    assert_eq!(file_list[0].name_path, "DATA/ADDED.RTON");
    assert_eq!(file_list[0].pool_index, 0);

    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].name, "Added_Common");
    assert_eq!(infos[0].pool_index, 0);
    assert_eq!(infos[0].ptx_number, 0);
    let files = unpack_rsg(&mut Cursor::new(archive.extract_packet(&infos[0]).unwrap())).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "DATA/ADDED.RTON");
    assert_eq!(files[0].data, b"added packet");

    let groups = archive.read_composite_info().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Added_Common");
    assert!(!groups[0].is_composite);
    assert_eq!(groups[0].packet_info[0].packet_index, 0);
}

#[test]
fn adds_an_empty_part0_rsg_packet() {
    let source = sample_archive();
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            added_packets: vec![RsgPacketAddition {
                name: "Empty_Common".into(),
                version: 4,
                compression_flags: 2,
                files: Vec::new(),
                group: Some(RsgPacketGroup {
                    name: "Empty_Common".into(),
                    is_composite: false,
                    category: ["0".into(), String::new()],
                }),
            }],
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.rsg_number, 2);
    assert_eq!(archive.header.autopool_number, 2);
    assert_eq!(archive.header.composite_number, 1);
    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(infos[1].name, "Empty_Common");
    assert_eq!(infos[1].ptx_number, 0);
    let files = unpack_rsg(&mut Cursor::new(archive.extract_packet(&infos[1]).unwrap())).unwrap();
    assert!(files.is_empty());
}

#[test]
fn adds_a_new_rsg_packet_with_a_part1_texture() {
    let source = sample_archive();
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            added_packets: vec![RsgPacketAddition {
                name: "Added_Texture".into(),
                version: 4,
                compression_flags: 1,
                files: vec![sample_texture("ATLASES/ADDED.PTX", 0, 2, 2)],
                group: Some(RsgPacketGroup {
                    name: "Added_Texture".into(),
                    is_composite: false,
                    category: ["0".into(), String::new()],
                }),
            }],
            ptx_infos: Some(vec![RsbPtxInfo {
                ptx_index: 0,
                width: 2,
                height: 2,
                pitch: 8,
                format: 0,
                alpha_size: None,
                alpha_format: None,
            }]),
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.rsg_number, 2);
    assert_eq!(archive.header.ptx_number, 1);
    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(infos[1].name, "Added_Texture");
    assert_eq!(infos[1].ptx_number, 1);
    assert_eq!(infos[1].ptx_before_number, 0);
    let files = unpack_rsg(&mut Cursor::new(archive.extract_packet(&infos[1]).unwrap())).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0].is_part1);
    assert_eq!(files[0].part1_info.as_ref().unwrap().id, 0);
}

#[test]
fn adds_and_deletes_part1_textures_and_resizes_the_global_table() {
    let source = sample_archive();
    let added_files = vec![
        sample_file("DATA/OLD.RTON", b"old"),
        sample_file("DATA/REMOVE.TXT", b"remove"),
        sample_texture("ATLASES/NEW_TEXTURE.PTX", 0, 2, 2),
    ];
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index: 0,
                original_paths: vec!["DATA/OLD.RTON".into(), "DATA/REMOVE.TXT".into()],
                name: "Sample_Common".into(),
                version: 4,
                compression_flags: 3,
                files: added_files.clone(),
            }],
            ptx_infos: Some(vec![RsbPtxInfo {
                ptx_index: 0,
                width: 2,
                height: 2,
                pitch: 8,
                format: 0,
                alpha_size: None,
                alpha_format: None,
            }]),
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.ptx_number, 1);
    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(infos[0].ptx_number, 1);
    assert_eq!(infos[0].ptx_before_number, 0);
    let ptx_infos = archive.read_ptx_info().unwrap();
    assert_eq!(ptx_infos.len(), 1);
    assert_eq!((ptx_infos[0].width, ptx_infos[0].height), (2, 2));
    let files = unpack_rsg(&mut Cursor::new(archive.extract_packet(&infos[0]).unwrap())).unwrap();
    let texture = files
        .iter()
        .find(|file| file.path == "ATLASES/NEW_TEXTURE.PTX")
        .unwrap();
    assert!(texture.is_part1);
    assert_eq!(texture.part1_info.as_ref().unwrap().id, 0);

    let without_texture = files
        .iter()
        .filter(|file| !file.is_part1)
        .cloned()
        .collect::<Vec<_>>();
    let rebuilt_again = rebuild_rsb(
        &rebuilt,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index: 0,
                original_paths: files.iter().map(|file| file.path.clone()).collect(),
                name: "Sample_Common".into(),
                version: 4,
                compression_flags: 3,
                files: without_texture,
            }],
            ptx_infos: Some(Vec::new()),
            ..Default::default()
        },
    )
    .unwrap();

    let mut reopened = Rsb::open(Cursor::new(&rebuilt_again)).unwrap();
    assert_eq!(reopened.header.ptx_number, 0);
    let infos = reopened.read_rsg_info().unwrap();
    assert_eq!(infos[0].ptx_number, 0);
    assert_eq!(infos[0].ptx_before_number, 0);
    assert!(reopened.read_ptx_info().unwrap().is_empty());
    assert!(
        unpack_rsg(&mut Cursor::new(
            reopened.extract_packet(&infos[0]).unwrap()
        ))
        .unwrap()
        .iter()
        .all(|file| !file.is_part1)
    );
}

#[test]
fn removes_texture_rsg_packets_and_reindexes_global_metadata() {
    let source = sample_archive();
    let textured = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            added_packets: vec![
                RsgPacketAddition {
                    name: "Texture_A".into(),
                    version: 4,
                    compression_flags: 1,
                    files: vec![sample_texture("ATLASES/A.PTX", 0, 2, 2)],
                    group: Some(RsgPacketGroup {
                        name: "Texture_A".into(),
                        is_composite: false,
                        category: ["0".into(), String::new()],
                    }),
                },
                RsgPacketAddition {
                    name: "Texture_B".into(),
                    version: 4,
                    compression_flags: 1,
                    files: vec![sample_texture("ATLASES/B.PTX", 0, 4, 4)],
                    group: Some(RsgPacketGroup {
                        name: "Texture_B".into(),
                        is_composite: false,
                        category: ["0".into(), String::new()],
                    }),
                },
            ],
            ptx_infos: Some(vec![
                RsbPtxInfo {
                    ptx_index: 0,
                    width: 2,
                    height: 2,
                    pitch: 8,
                    format: 0,
                    alpha_size: None,
                    alpha_format: None,
                },
                RsbPtxInfo {
                    ptx_index: 1,
                    width: 4,
                    height: 4,
                    pitch: 16,
                    format: 0,
                    alpha_size: None,
                    alpha_format: None,
                },
            ]),
            ..Default::default()
        },
    )
    .unwrap();

    let rebuilt = rebuild_rsb(
        &textured,
        &RsbArchiveEdit {
            removed_packets: vec![1],
            ..Default::default()
        },
    )
    .unwrap();

    let mut archive = Rsb::open(Cursor::new(&rebuilt)).unwrap();
    assert_eq!(archive.header.rsg_number, 2);
    assert_eq!(archive.header.ptx_number, 1);

    let infos = archive.read_rsg_info().unwrap();
    assert_eq!(
        infos
            .iter()
            .map(|info| info.name.as_str())
            .collect::<Vec<_>>(),
        ["Sample_Common", "Texture_B"]
    );
    assert_eq!(infos[1].ptx_number, 1);
    assert_eq!(infos[1].ptx_before_number, 0);

    let ptx_infos = archive.read_ptx_info().unwrap();
    assert_eq!(ptx_infos.len(), 1);
    assert_eq!(ptx_infos[0].ptx_index, 0);
    assert_eq!((ptx_infos[0].width, ptx_infos[0].height), (4, 4));

    let file_list = archive.read_file_list().unwrap();
    assert!(
        !file_list
            .iter()
            .any(|entry| entry.name_path == "ATLASES/A.PTX")
    );
    assert!(
        file_list
            .iter()
            .any(|entry| entry.name_path == "ATLASES/B.PTX" && entry.pool_index == 1)
    );

    let groups = archive.read_composite_info().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Texture_B");
    assert_eq!(groups[0].packet_info[0].packet_index, 1);
}
