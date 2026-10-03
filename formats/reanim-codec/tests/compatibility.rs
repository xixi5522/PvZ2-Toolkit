use reanim_codec::{
    Reanim, ReanimError, ReanimTrack, ReanimTransform, ReanimVersion, decode, decode_pc,
    decode_with_version, decode_xfl, encode, encode_xfl,
};

const KNOWN_PC_REANIM: &[u8] = &[
    0xd4, 0xfe, 0xad, 0xde, 0xfa, 0x00, 0x00, 0x00, 0x78, 0x9c, 0x3b, 0xb0, 0x65, 0xf2, 0x66, 0x06,
    0x20, 0x60, 0x04, 0x11, 0x0c, 0x1f, 0x1c, 0x41, 0x24, 0x0f, 0x03, 0x02, 0x30, 0x03, 0x31, 0x0b,
    0x10, 0x67, 0xa4, 0x26, 0xa6, 0xe8, 0x80, 0x45, 0x1a, 0xec, 0x19, 0x1c, 0x64, 0x8e, 0xe1, 0xc4,
    0xa8, 0xc0, 0x81, 0x81, 0x61, 0x01, 0x10, 0x33, 0xd8, 0x43, 0xf0, 0x01, 0x7b, 0x9c, 0x6a, 0x71,
    0x9a, 0xd7, 0xb0, 0x1f, 0x5d, 0xad, 0x20, 0x10, 0x7b, 0xfa, 0x3a, 0xba, 0xbb, 0xc6, 0x47, 0xf9,
    0xfb, 0x3a, 0x79, 0xba, 0xc6, 0x7b, 0xb8, 0x3a, 0xba, 0x10, 0x25, 0x89, 0x0c, 0x00, 0xf5, 0x39,
    0x27, 0xaf,
];

#[test]
fn decodes_known_pc_fixture() {
    let (reanim, version) = decode_with_version(KNOWN_PC_REANIM).unwrap();
    assert_eq!(version, ReanimVersion::PC);
    assert_eq!(reanim.fps, 30.0);
    assert_eq!(reanim.tracks.len(), 1);
    assert_eq!(reanim.tracks[0].name, "head");
    assert_eq!(reanim.tracks[0].transforms.len(), 3);
    assert_eq!(reanim.tracks[0].transforms[0].x, Some(1.0));
    assert_eq!(
        reanim.tracks[0].transforms[0].i.as_deref(),
        Some("IMAGE_ZOMBIE_HEAD")
    );
}

#[test]
fn rejects_negative_lengths_and_bad_wrapper_sizes() {
    let mut negative_track_count = vec![0; 8];
    negative_track_count.extend_from_slice(&(-1_i32).to_le_bytes());
    assert!(matches!(
        decode_pc(&negative_track_count),
        Err(ReanimError::InvalidLength { kind: "track", .. })
    ));

    let mut encoded = encode(&Reanim::default(), ReanimVersion::PC).unwrap();
    let declared = u32::from_le_bytes(encoded[4..8].try_into().unwrap());
    encoded[4..8].copy_from_slice(&(declared + 1).to_le_bytes());
    assert!(matches!(
        decode_pc(&encoded),
        Err(ReanimError::SizeMismatch { .. })
    ));
}

#[test]
fn xfl_directory_roundtrip_preserves_timeline_shape() {
    let source = Reanim {
        do_scale: None,
        fps: 24.0,
        tracks: vec![ReanimTrack {
            name: "body".to_string(),
            transforms: vec![
                ReanimTransform {
                    x: Some(12.0),
                    y: Some(8.0),
                    i: Some("IMAGE_REANIM_SUNFLOWER".to_string()),
                    ..Default::default()
                },
                ReanimTransform {
                    a: Some(0.5),
                    ..Default::default()
                },
            ],
        }],
    };
    let directory = tempfile::tempdir().unwrap();

    encode_xfl(&source, directory.path()).unwrap();
    let decoded = decode_xfl(directory.path()).unwrap();

    assert_eq!(decoded.fps, source.fps);
    assert_eq!(decoded.tracks.len(), 1);
    assert_eq!(decoded.tracks[0].name, "body");
    assert_eq!(decoded.tracks[0].transforms.len(), 2);
    assert!(directory.path().join("main.xfl").is_file());
    assert!(directory.path().join("DOMDocument.xml").is_file());
}

#[test]
fn all_compiled_layouts_are_detected() {
    let source = decode(KNOWN_PC_REANIM).unwrap();
    for version in [
        ReanimVersion::PC,
        ReanimVersion::Phone32,
        ReanimVersion::Phone64,
    ] {
        let encoded = encode(&source, version).unwrap();
        let (_, detected) = decode_with_version(&encoded).unwrap();
        assert_eq!(detected, version);
    }
}
