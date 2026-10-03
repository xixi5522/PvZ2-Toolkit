use newton_manifest::{
    CompositeGroup, DecodeLimits, NewtonError, RawResource, RawResourceGroup, RawResourceManifest,
    Resource, ResourceGroup, ResourceManifest, ResourceType, SimpleGroup, Subgroup, encode_newton,
    encoded_len, from_bytes, from_bytes_with_limits, raw_to_bytes, to_bytes,
};
use std::io::{self, Write};

fn sample_manifest() -> ResourceManifest {
    ResourceManifest {
        slot_count: 2,
        groups: vec![
            ResourceGroup::Composite(CompositeGroup {
                id: "Main".into(),
                resolution: None,
                parent: None,
                subgroups: vec![Subgroup {
                    id: "Main_Common".into(),
                    resolution: Some(1536),
                }],
            }),
            ResourceGroup::Simple(SimpleGroup {
                id: "Main_Common".into(),
                resolution: Some(1536),
                parent: Some("Main".into()),
                resources: vec![Resource {
                    resource_type: ResourceType::Image,
                    slot: 1,
                    id: "IMAGE_EXAMPLE".into(),
                    path: "images\\example".into(),
                    width: None,
                    height: None,
                    x: Some(0),
                    y: Some(0),
                    ax: Some(4),
                    ay: Some(8),
                    aw: Some(32),
                    ah: Some(16),
                    cols: None,
                    rows: None,
                    atlas: false,
                    parent: Some("ATLAS_EXAMPLE".into()),
                }],
            }),
        ],
    }
}

#[test]
fn binary_round_trip_preserves_manifest() {
    let manifest = sample_manifest();
    let encoded = to_bytes(&manifest).expect("sample manifest should encode");
    assert_eq!(encoded_len(&manifest).unwrap(), encoded.len());
    let decoded = from_bytes(&encoded).expect("sample manifest should decode");
    assert_eq!(decoded, manifest);
}

#[test]
fn encoding_is_preflighted_before_the_first_write() {
    #[derive(Default)]
    struct Counter(usize);

    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let manifest = ResourceManifest {
        slot_count: u32::MAX,
        groups: Vec::new(),
    };
    let mut writer = Counter::default();
    assert!(matches!(
        encode_newton(&manifest, &mut writer),
        Err(NewtonError::IntegerOutOfRange {
            field: "slot count",
            ..
        })
    ));
    assert_eq!(writer.0, 0);
}

#[test]
fn serde_uses_official_group_and_resource_names() {
    let json = serde_json::to_value(sample_manifest()).expect("manifest should serialize");
    assert_eq!(json["groups"][0]["type"], "composite");
    assert_eq!(json["groups"][1]["type"], "simple");
    assert_eq!(json["groups"][1]["resources"][0]["type"], "Image");
    assert_eq!(json["groups"][1]["res"], 1536);
}

#[test]
fn rejects_trailing_bytes() {
    let mut encoded = to_bytes(&sample_manifest()).unwrap();
    encoded.push(0);
    let error = from_bytes(&encoded).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::TrailingData { remaining: 1 }
    ));
}

#[test]
fn rejects_non_boolean_flags() {
    let mut encoded = to_bytes(&ResourceManifest {
        slot_count: 0,
        groups: vec![ResourceGroup::Composite(CompositeGroup {
            id: String::new(),
            resolution: None,
            parent: None,
            subgroups: Vec::new(),
        })],
    })
    .unwrap();
    encoded[22] = 2;
    let error = from_bytes(&encoded).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::InvalidBoolean {
            field: "group parent flag",
            value: 2
        }
    ));
}

#[test]
fn rejects_a_missing_required_group_id() {
    let encoded = raw_to_bytes(&RawResourceManifest {
        slot_count: 0,
        groups: vec![RawResourceGroup {
            group_type: 1,
            resolution: 0,
            has_id: 0,
            has_parent: 0,
            id: None::<Vec<u8>>,
            parent: None,
            subgroups: Vec::new(),
            resources: Vec::new(),
        }],
    })
    .unwrap();
    let error = from_bytes(&encoded).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::MissingRequiredString { field: "group id" }
    ));
}

#[test]
fn writes_resource_id_and_path_presence_flags() {
    let manifest = ResourceManifest {
        slot_count: 1,
        groups: vec![ResourceGroup::Simple(SimpleGroup {
            id: String::new(),
            resolution: None,
            parent: None,
            resources: vec![Resource {
                resource_type: ResourceType::File,
                slot: 0,
                id: "ID".into(),
                path: "路径".into(),
                width: None,
                height: None,
                x: None,
                y: None,
                ax: None,
                ay: None,
                aw: None,
                ah: None,
                cols: None,
                rows: None,
                atlas: false,
                parent: None,
            }],
        })],
    };

    let encoded = to_bytes(&manifest).unwrap();
    let resource_offset = 27;
    assert_eq!(encoded[resource_offset + 0x2e], 1);
    assert_eq!(encoded[resource_offset + 0x2f], 1);
    assert_eq!(encoded[resource_offset + 0x30], 0);
    assert_eq!(from_bytes(&encoded).unwrap(), manifest);
}

#[test]
fn rejects_missing_required_resource_strings() {
    let raw_resource = RawResource {
        resource_type: 4,
        slot: 0,
        width: 0,
        height: 0,
        x: i32::MAX,
        y: i32::MAX,
        ax: 0,
        ay: 0,
        aw: 0,
        ah: 0,
        cols: 1,
        rows: 1,
        atlas: 0,
        has_id: 0,
        has_path: 1,
        has_parent: 0,
        id: None,
        path: Some(b"PATH".to_vec()),
        parent: None,
    };
    let raw_manifest = |resource| RawResourceManifest {
        slot_count: 1,
        groups: vec![RawResourceGroup {
            group_type: 2,
            resolution: 0,
            has_id: 1,
            has_parent: 0,
            id: Some(Vec::new()),
            parent: None,
            subgroups: Vec::new(),
            resources: vec![resource],
        }],
    };
    let missing_id = raw_to_bytes(&raw_manifest(raw_resource.clone())).unwrap();
    let error = from_bytes(&missing_id).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::MissingRequiredString {
            field: "resource id"
        }
    ));

    let missing_path_resource = RawResource {
        has_id: 1,
        has_path: 0,
        id: Some(b"ID".to_vec()),
        path: None,
        ..raw_resource
    };
    let missing_path = raw_to_bytes(&raw_manifest(missing_path_resource)).unwrap();
    let error = from_bytes(&missing_path).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::MissingRequiredString {
            field: "resource path"
        }
    ));
}

#[test]
fn string_lengths_count_utf8_bytes() {
    let manifest = ResourceManifest {
        slot_count: 0,
        groups: vec![ResourceGroup::Composite(CompositeGroup {
            id: "主组".into(),
            resolution: None,
            parent: None,
            subgroups: Vec::new(),
        })],
    };
    let encoded = to_bytes(&manifest).unwrap();

    assert_eq!(&encoded[23..27], &6_i32.to_le_bytes());
    assert_eq!(from_bytes(&encoded).unwrap(), manifest);
}

#[test]
fn rejects_negative_string_lengths() {
    let mut encoded = to_bytes(&ResourceManifest {
        slot_count: 0,
        groups: vec![ResourceGroup::Composite(CompositeGroup {
            id: "Main".into(),
            resolution: None,
            parent: None,
            subgroups: Vec::new(),
        })],
    })
    .unwrap();
    encoded[23..27].copy_from_slice(&(-1_i32).to_le_bytes());

    let error = from_bytes(&encoded).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::NegativeValue {
            field: "group id",
            value: -1
        }
    ));
}

#[test]
fn enforces_group_allocation_limit_before_allocating() {
    let encoded = to_bytes(&sample_manifest()).unwrap();
    let limits = DecodeLimits {
        max_groups: 1,
        ..DecodeLimits::default()
    };
    let error = from_bytes_with_limits(&encoded, limits).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::CountLimitExceeded {
            field: "group count",
            count: 2,
            limit: 1
        }
    ));
}
