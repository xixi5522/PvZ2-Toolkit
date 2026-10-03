use newton_manifest::{
    NewtonError, ResourceGroup, ResourceType, from_bytes, raw_from_bytes_borrowed, raw_to_bytes,
    to_bytes,
};

// One simple group "G" containing one File resource "R" at path "P".
// This fixture is handwritten from the Hopper-confirmed packed layout and does
// not use the crate writer to construct its expected bytes.
const GOLDEN: &[u8] = &[
    0x01, 0x00, 0x00, 0x00, // slot_count
    0x01, 0x00, 0x00, 0x00, // group_count
    0x02, // simple
    0x00, 0x00, 0x00, 0x00, // resolution
    0x00, 0x00, 0x00, 0x00, // subgroup_count
    0x01, 0x00, 0x00, 0x00, // resource_count
    0x01, 0x00, // has_id, has_parent
    0x01, 0x00, 0x00, 0x00, b'G', // group id
    0x04, // File
    0x00, 0x00, 0x00, 0x00, // slot
    0x00, 0x00, 0x00, 0x00, // width
    0x00, 0x00, 0x00, 0x00, // height
    0xff, 0xff, 0xff, 0x7f, // x sentinel
    0xff, 0xff, 0xff, 0x7f, // y sentinel
    0x00, 0x00, 0x00, 0x00, // ax
    0x00, 0x00, 0x00, 0x00, // ay
    0x00, 0x00, 0x00, 0x00, // aw
    0x00, 0x00, 0x00, 0x00, // ah
    0x01, 0x00, 0x00, 0x00, // cols
    0x01, 0x00, 0x00, 0x00, // rows
    0x00, 0x01, 0x01, 0x00, // atlas, has_id, has_path, has_parent
    0x01, 0x00, 0x00, 0x00, b'R', // resource id
    0x01, 0x00, 0x00, 0x00, b'P', // path
];

#[test]
fn decodes_and_reencodes_handwritten_hopper_layout() {
    let manifest = from_bytes(GOLDEN).unwrap();
    assert_eq!(manifest.slot_count, 1);
    let ResourceGroup::Simple(group) = &manifest.groups[0] else {
        panic!("golden group should be simple");
    };
    assert_eq!(group.id, "G");
    assert_eq!(group.resources[0].resource_type, ResourceType::File);
    assert_eq!(group.resources[0].id, "R");
    assert_eq!(group.resources[0].path, "P");
    assert_eq!(to_bytes(&manifest).unwrap(), GOLDEN);
}

#[test]
fn all_hopper_resource_type_indices_are_stable() {
    let expected = [
        ResourceType::Image,
        ResourceType::PopAnim,
        ResourceType::SoundBank,
        ResourceType::File,
        ResourceType::PrimeFont,
        ResourceType::RenderEffect,
        ResourceType::DecodedSoundBank,
    ];
    for (index, expected) in (1_u8..=7).zip(expected) {
        let mut bytes = GOLDEN.to_vec();
        bytes[28] = index;
        let manifest = from_bytes(&bytes).unwrap();
        let ResourceGroup::Simple(group) = &manifest.groups[0] else {
            unreachable!();
        };
        assert_eq!(group.resources[0].resource_type, expected);
        assert_eq!(expected.to_u8(), index);
    }
}

#[test]
fn borrowed_raw_layer_preserves_invalid_utf8_without_copying() {
    let mut bytes = GOLDEN.to_vec();
    bytes[27] = 0xff;

    let raw = raw_from_bytes_borrowed(&bytes).unwrap();
    let id = raw.groups[0].id.unwrap();
    assert_eq!(id, &[0xff]);
    assert_eq!(id.as_ptr(), bytes[27..].as_ptr());
    assert_eq!(raw_to_bytes(&raw).unwrap(), bytes);

    let error = from_bytes(&bytes).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::InvalidUtf8 { field: "group id" }
    ));
}

#[test]
fn truncated_input_reports_offset_and_record_context() {
    let error = from_bytes(&GOLDEN[..GOLDEN.len() - 1]).unwrap_err();
    let NewtonError::DecodeContext {
        offset,
        context,
        field,
        ..
    } = error
    else {
        panic!("decode error should carry context");
    };
    assert!(offset > 0);
    assert_eq!(context, "group[0].resource[0]");
    assert_eq!(field, "resource path");
}
