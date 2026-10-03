use rsb_patch::{
    ContainerDecodeOptions, PacketPatch, PatchError, RSB_PATCH_HEADER_SIZE, RSB_PATCH_MAGIC,
    RSB_PATCH_MAGIC_BYTES, RSB_PATCH_PACKET_RECORD_SIZE, RsbPatch, md5_hash,
    vcdiff::{self, DecodeOptions, EncodeOptions},
};

#[test]
fn standard_and_interleaved_vcdiff_round_trip() {
    let source = b"PopCap resource packet: lawn/common/texture";
    let target = b"PopCap resource packet: lawn/common/texture texture texture texture";

    let standard = vcdiff::encode_with_options(source, target, EncodeOptions::standard()).unwrap();
    assert_eq!(&standard[..5], &[0xD6, 0xC3, 0xC4, 0, 0]);
    assert_eq!(vcdiff::decode(source, &standard).unwrap(), target);

    let interleaved = vcdiff::encode(source, target).unwrap();
    assert_eq!(&interleaved[..5], &[0xD6, 0xC3, 0xC4, b'S', 0]);
    assert_eq!(vcdiff::decode(source, &interleaved).unwrap(), target);
    assert!(interleaved.len() < target.len());
}

#[test]
fn encoder_matches_twinning_open_vcdiff_output() {
    let source = b"HEADER: PopCap resource stream bundle patch comparison\n\
PACKET_ALPHA: abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n\
PACKET_BETA:  the quick brown fox jumps over the lazy dog; the quick brown fox jumps over the lazy dog\n\
TEXTURE_DATA: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n\
FOOTER: stable ending for source and target comparison\n";
    let target = b"HEADER: PopCap resource stream bundle patch comparison\n\
INSERTED: this range exists only in the target document\n\
PACKET_ALPHA: abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\n\
PACKET_BETA:  the quick brown fox jumps over the energetic zombie; the quick brown fox jumps over the lazy dog\n\
TEXTURE_DATA: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n\
FOOTER: stable ending for source and target comparison\n";
    // Generated independently by the open-vcdiff revision bundled with
    // Twinning, with interleaving and target matching enabled.
    let expected = [
        0xd6, 0xc3, 0xc4, 0x53, 0x00, 0x01, 0x82, 0x71, 0x00, 0x5b, 0x83, 0x31, 0x00, 0x00, 0x55,
        0x00, 0x73, 0x37, 0x00, 0x01, 0x37, 0x49, 0x4e, 0x53, 0x45, 0x52, 0x54, 0x45, 0x44, 0x3a,
        0x20, 0x74, 0x68, 0x69, 0x73, 0x20, 0x72, 0x61, 0x6e, 0x67, 0x65, 0x20, 0x65, 0x78, 0x69,
        0x73, 0x74, 0x73, 0x20, 0x6f, 0x6e, 0x6c, 0x79, 0x20, 0x69, 0x6e, 0x20, 0x74, 0x68, 0x65,
        0x20, 0x74, 0x61, 0x72, 0x67, 0x65, 0x74, 0x20, 0x64, 0x6f, 0x63, 0x75, 0x6d, 0x65, 0x6e,
        0x74, 0x13, 0x7f, 0x36, 0x11, 0x65, 0x6e, 0x65, 0x72, 0x67, 0x65, 0x74, 0x69, 0x63, 0x20,
        0x7a, 0x6f, 0x6d, 0x62, 0x69, 0x65, 0x43, 0x81, 0x34, 0x81, 0x07,
    ];

    assert_eq!(vcdiff::encode(source, target).unwrap(), expected);
}

#[test]
fn decodes_independent_standard_and_interleaved_add_windows() {
    let standard = delta_file(
        0,
        0,
        &[],
        3,
        b"abc",
        &[4], // ADD with an implicit size of 3.
        &[],
        None,
    );
    assert_eq!(vcdiff::decode(&[], &standard).unwrap(), b"abc");

    let interleaved = delta_file(b'S', 0, &[], 3, &[], &[4, b'a', b'b', b'c'], &[], None);
    assert_eq!(vcdiff::decode(&[], &interleaved).unwrap(), b"abc");
}

#[test]
fn decodes_an_rfc_custom_code_table() {
    let mut source_fields = Vec::new();
    varint(6 * 256, &mut source_fields);
    varint(0, &mut source_fields);
    let mut copy_size = vec![19]; // variable-size COPY in SELF mode
    varint(6 * 256, &mut copy_size);
    let embedded_table_delta = delta_file(
        0,
        0x01,
        &source_fields,
        6 * 256,
        &[],
        &copy_size,
        &[0],
        None,
    );

    let mut patch = vec![0xD6, 0xC3, 0xC4, 0, 0x02, 4, 3];
    patch.extend_from_slice(&embedded_table_delta);
    patch.extend_from_slice(&window(0, &[], 3, b"abc", &[4], &[], None));

    assert_eq!(vcdiff::decode(&[], &patch).unwrap(), b"abc");
}

#[test]
fn decodes_vcd_target_and_overlapping_copy() {
    let mut patch = vec![0xD6, 0xC3, 0xC4, 0, 0];
    patch.extend_from_slice(&window(0, &[], 2, b"ab", &[3], &[], None));
    // The second window uses the first target window as its source and COPYs
    // four bytes from a two-byte segment. The last two bytes exercise the
    // overlapping-target behavior required by RFC 3284.
    patch.extend_from_slice(&window(0x02, &[2, 0], 4, &[], &[20], &[0], None));
    assert_eq!(vcdiff::decode(&[], &patch).unwrap(), b"ababab");
}

#[test]
fn validates_extended_adler32() {
    let checksum = adler32(b"abc");
    let patch = delta_file(
        b'S',
        0x04,
        &[],
        3,
        &[],
        &[4, b'a', b'b', b'c'],
        &[],
        Some(checksum),
    );
    assert_eq!(vcdiff::decode(&[], &patch).unwrap(), b"abc");

    let mut corrupt = patch;
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        vcdiff::decode(&[], &corrupt),
        Err(PatchError::InvalidVcdiff(message)) if message.contains("Adler32 mismatch")
    ));
}

#[test]
fn rsb_patch_container_round_trip_uses_twinning_layout() {
    let information_before = b"old information";
    let information_after = b"new information";
    let packet_before = b"packet before";
    let packet_after = b"packet after with repeated repeated repeated bytes";
    let information_delta = vcdiff::encode(information_before, information_after).unwrap();
    let packet_delta = vcdiff::encode(packet_before, packet_after).unwrap();
    let patch = RsbPatch {
        all_after_size: 0x1234_5678,
        before_hash: md5_hash(information_before),
        information_patch: Some(information_delta.clone()),
        packets: vec![
            PacketPatch {
                name: "PACKET_1".into(),
                before_hash: md5_hash(packet_before),
                patch: Some(packet_delta.clone()),
            },
            PacketPatch {
                name: "UNCHANGED".into(),
                before_hash: md5_hash(b"same"),
                patch: None,
            },
        ],
    };

    let encoded = patch.to_bytes().unwrap();
    assert_eq!(RSB_PATCH_MAGIC, 0x5253_4250);
    assert_eq!(RSB_PATCH_MAGIC_BYTES, *b"PBSR");
    assert_eq!(&encoded[0..4], &RSB_PATCH_MAGIC.to_le_bytes());
    assert_eq!(u32::from_le_bytes(encoded[4..8].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(encoded[8..12].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(encoded[16..20].try_into().unwrap()), 0);
    assert_eq!(
        u32::from_le_bytes(encoded[20..24].try_into().unwrap()),
        information_delta.len() as u32
    );
    assert_eq!(u32::from_le_bytes(encoded[40..44].try_into().unwrap()), 2);
    assert_eq!(u32::from_le_bytes(encoded[44..48].try_into().unwrap()), 1);
    let first_packet = RSB_PATCH_HEADER_SIZE + information_delta.len();
    assert_eq!(
        u32::from_le_bytes(encoded[first_packet..first_packet + 4].try_into().unwrap()),
        1
    );
    assert_eq!(&encoded[first_packet + 8..first_packet + 16], b"PACKET_1");
    let second_packet = first_packet + RSB_PATCH_PACKET_RECORD_SIZE + packet_delta.len();
    assert_eq!(
        u32::from_le_bytes(
            encoded[second_packet..second_packet + 4]
                .try_into()
                .unwrap()
        ),
        0
    );

    let decoded = RsbPatch::read(encoded.as_slice()).unwrap();
    assert_eq!(decoded, patch);
    assert_eq!(
        decoded.apply_information(information_before).unwrap(),
        information_after
    );
    assert_eq!(
        decoded.packets[0].apply(packet_before).unwrap(),
        packet_after
    );
    assert_eq!(decoded.packets[1].apply(b"same").unwrap(), b"same");
}

#[test]
fn rejects_bad_container_state_and_untrusted_output_size() {
    let patch = RsbPatch {
        information_patch: Some(Vec::new()),
        ..RsbPatch::default()
    };
    assert!(matches!(
        patch.to_bytes(),
        Err(PatchError::InconsistentPatchState { .. })
    ));

    let delta = vcdiff::encode(&[], b"too large").unwrap();
    assert!(matches!(
        vcdiff::decode_with_options(&[], &delta, DecodeOptions::new(4)),
        Err(PatchError::OutputLimitExceeded { limit: 4 })
    ));
}

#[test]
fn enforces_untrusted_container_allocation_limits() {
    let patch = RsbPatch {
        information_patch: Some(vec![1, 2, 3]),
        packets: vec![PacketPatch {
            name: "PACKET".into(),
            patch: Some(vec![4, 5, 6, 7]),
            ..PacketPatch::default()
        }],
        ..RsbPatch::default()
    };
    let encoded = patch.to_bytes().unwrap();

    assert!(matches!(
        RsbPatch::read_with_options(
            encoded.as_slice(),
            ContainerDecodeOptions::new(0, usize::MAX, usize::MAX),
        ),
        Err(PatchError::ContainerLimitExceeded {
            field: "packet count",
            limit: 0,
            actual: 1,
        })
    ));
    assert!(matches!(
        RsbPatch::read_with_options(
            encoded.as_slice(),
            ContainerDecodeOptions::new(1, 2, usize::MAX),
        ),
        Err(PatchError::ContainerLimitExceeded {
            field: "delta size",
            limit: 2,
            actual: 3,
        })
    ));
    assert!(matches!(
        RsbPatch::read_with_options(encoded.as_slice(), ContainerDecodeOptions::new(1, 4, 6),),
        Err(PatchError::ContainerLimitExceeded {
            field: "total delta size",
            limit: 6,
            actual: 7,
        })
    ));
}

#[test]
fn verifies_md5_before_applying_a_patch() {
    let patch = PacketPatch {
        name: "PACKET".into(),
        before_hash: md5_hash(b"expected"),
        patch: None,
    };
    assert!(matches!(
        patch.apply(b"different"),
        Err(PatchError::HashMismatch { .. })
    ));
}

#[test]
fn preserves_a_full_width_packet_name() {
    let packet_name = "P".repeat(128);
    let patch = RsbPatch {
        packets: vec![PacketPatch {
            name: packet_name.clone(),
            before_hash: md5_hash(&[]),
            patch: None,
        }],
        ..RsbPatch::default()
    };
    let decoded = RsbPatch::read(patch.to_bytes().unwrap().as_slice()).unwrap();
    assert_eq!(decoded.packets[0].name, packet_name);
}

#[allow(clippy::too_many_arguments)]
fn delta_file(
    version: u8,
    indicator: u8,
    source_fields: &[u8],
    target_length: usize,
    data: &[u8],
    instructions: &[u8],
    addresses: &[u8],
    checksum: Option<u32>,
) -> Vec<u8> {
    let mut result = vec![0xD6, 0xC3, 0xC4, version, 0];
    result.extend_from_slice(&window(
        indicator,
        source_fields,
        target_length,
        data,
        instructions,
        addresses,
        checksum,
    ));
    result
}

fn window(
    indicator: u8,
    source_fields: &[u8],
    target_length: usize,
    data: &[u8],
    instructions: &[u8],
    addresses: &[u8],
    checksum: Option<u32>,
) -> Vec<u8> {
    let mut delta = Vec::new();
    varint(target_length, &mut delta);
    delta.push(0);
    varint(data.len(), &mut delta);
    varint(instructions.len(), &mut delta);
    varint(addresses.len(), &mut delta);
    if let Some(checksum) = checksum {
        varint(checksum as usize, &mut delta);
    }
    delta.extend_from_slice(data);
    delta.extend_from_slice(instructions);
    delta.extend_from_slice(addresses);

    let mut result = vec![indicator];
    result.extend_from_slice(source_fields);
    varint(delta.len(), &mut result);
    result.extend_from_slice(&delta);
    result
}

fn varint(mut value: usize, output: &mut Vec<u8>) {
    let mut bytes = [0_u8; 10];
    let mut index = bytes.len() - 1;
    bytes[index] = (value & 0x7F) as u8;
    value >>= 7;
    while value != 0 {
        index -= 1;
        bytes[index] = ((value & 0x7F) as u8) | 0x80;
        value >>= 7;
    }
    output.extend_from_slice(&bytes[index..]);
}

fn adler32(data: &[u8]) -> u32 {
    let mut first = 1_u32;
    let mut second = 0_u32;
    for byte in data {
        first = (first + u32::from(*byte)) % 65_521;
        second = (second + first) % 65_521;
    }
    (second << 16) | first
}
