use rsb_archive::{
    Part1Extra, RSG_FOURCC, RSG_MAGIC, RsbError, RsgHeader, UnpackedFile, pack_rsg, unpack_rsg,
};
use std::io::Cursor;

fn sample_files() -> Vec<UnpackedFile> {
    vec![
        UnpackedFile {
            path: "DATA/CONFIG.RTON".into(),
            data: b"RTON sample configuration".to_vec(),
            is_part1: false,
            part1_info: None,
        },
        UnpackedFile {
            path: "DATA/LEVELS/INTRO.RTON".into(),
            data: (0_u8..=127).collect(),
            is_part1: false,
            part1_info: None,
        },
        UnpackedFile {
            path: "IMAGES/EXAMPLE.PTX".into(),
            data: vec![0x5a; 257],
            is_part1: true,
            part1_info: Some(Part1Extra {
                id: 7,
                width: 16,
                height: 32,
            }),
        },
    ]
}

#[test]
fn round_trips_every_compression_flag() {
    for flags in 0..=3 {
        let expected = sample_files();
        let mut packed = Cursor::new(Vec::new());
        pack_rsg(&mut packed, &expected, 4, flags).unwrap();
        assert_eq!(&packed.get_ref()[..4], b"pgsr");
        assert_eq!(RSG_FOURCC, *b"rsgp");
        assert_eq!(RSG_MAGIC, u32::from_be_bytes(RSG_FOURCC));
        assert_eq!(RSG_MAGIC.to_le_bytes(), *b"pgsr");

        packed.set_position(0);
        let actual = unpack_rsg(&mut packed).unwrap();
        assert_eq!(actual.len(), expected.len(), "flags={flags}");

        for expected_file in expected {
            let actual_file = actual
                .iter()
                .find(|file| file.path == expected_file.path)
                .unwrap_or_else(|| panic!("missing {} with flags={flags}", expected_file.path));
            assert_eq!(actual_file.data, expected_file.data, "flags={flags}");
            assert_eq!(
                actual_file.is_part1, expected_file.is_part1,
                "flags={flags}"
            );
            assert_eq!(
                actual_file
                    .part1_info
                    .as_ref()
                    .map(|info| (info.id, info.width, info.height)),
                expected_file
                    .part1_info
                    .as_ref()
                    .map(|info| (info.id, info.width, info.height)),
                "flags={flags}"
            );
        }
    }
}

#[test]
fn reads_the_public_fixed_packet_header() {
    let mut packed = Cursor::new(Vec::new());
    pack_rsg(&mut packed, &sample_files(), 4, 3).unwrap();
    packed.set_position(0);

    let header = RsgHeader::read_from(&mut packed).unwrap();
    assert_eq!(header.magic, RSG_MAGIC);
    assert_eq!(header.version, 4);
    assert_eq!(header.flags, 3);
    assert_eq!(header.file_offset, header.part0_offset);
    assert!(header.file_list_length > 0);
    assert_eq!(header.file_list_offset, 92);
    assert!(header.part0_size > 0);
    assert!(header.part1_size > 0);
}

#[test]
fn rejects_invalid_compression_flags() {
    let mut packed = Cursor::new(Vec::new());
    let error = pack_rsg(&mut packed, &sample_files(), 4, 4).unwrap_err();
    assert!(matches!(error, RsbError::InvalidCompression(4)));
}

#[test]
fn requires_texture_metadata_for_part_one_files() {
    let file = UnpackedFile {
        path: "IMAGES/BROKEN.PTX".into(),
        data: vec![0; 4],
        is_part1: true,
        part1_info: None,
    };
    let mut packed = Cursor::new(Vec::new());
    let error = pack_rsg(&mut packed, &[file], 4, 0).unwrap_err();
    assert!(matches!(error, RsbError::MissingPart1Info(path) if path == "IMAGES/BROKEN.PTX"));
}
