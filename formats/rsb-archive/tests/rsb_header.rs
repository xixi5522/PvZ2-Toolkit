use rsb_archive::{RSB_MAGIC, Rsb, RsbHeader, RsbWriter};
use std::io::Cursor;

#[test]
fn writes_and_reads_version_four_header() {
    let header = RsbHeader {
        magic: RSB_MAGIC,
        version: 4,
        information_section_size: 0x4000,
        resource_path_section_size: 0x120,
        resource_path_section_offset: 0x70,
        rsg_list_length: 0x40,
        rsg_list_begin_offset: 0x190,
        rsg_number: 3,
        rsg_info_begin_offset: 0x1d0,
        rsg_info_each_length: 204,
        composite_number: 2,
        composite_info_begin_offset: 0x440,
        composite_info_each_length: 1156,
        composite_list_length: 0x80,
        composite_list_begin_offset: 0x520,
        autopool_number: 1,
        autopool_info_begin_offset: 0x5a0,
        autopool_info_each_length: 152,
        ptx_number: 5,
        ptx_info_begin_offset: 0x638,
        ptx_info_each_length: 24,
        part1_begin_offset: 0x1000,
        part2_begin_offset: 0x2000,
        part3_begin_offset: 0x3000,
        information_without_manifest_section_size: 0x3800,
    };

    let mut writer = RsbWriter::new(Cursor::new(Vec::new()));
    writer.write_header(&header).unwrap();

    let bytes = writer.writer.into_inner();
    assert_eq!(&bytes[..4], b"1bsr");

    let rsb = Rsb::open(Cursor::new(bytes)).unwrap();
    assert_eq!(rsb.header.magic, header.magic);
    assert_eq!(rsb.header.magic.to_be_bytes(), *b"rsb1");
    assert_eq!(rsb.header.version, header.version);
    assert_eq!(
        rsb.header.information_section_size,
        header.information_section_size
    );
    assert_eq!(
        rsb.header.resource_path_section_offset,
        header.resource_path_section_offset
    );
    assert_eq!(rsb.header.rsg_number, header.rsg_number);
    assert_eq!(rsb.header.rsg_info_each_length, header.rsg_info_each_length);
    assert_eq!(rsb.header.ptx_number, header.ptx_number);
    assert_eq!(
        rsb.header.information_without_manifest_section_size,
        header.information_without_manifest_section_size
    );
    assert_eq!(rsb.header.metadata_size(true), 0x4000);
    assert_eq!(rsb.header.metadata_size(false), 0x3800);
}
