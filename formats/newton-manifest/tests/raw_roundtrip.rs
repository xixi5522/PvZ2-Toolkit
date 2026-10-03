use newton_manifest::{
    DecodeLimits, NewtonError, RawResourceGroup, RawResourceManifest, raw_from_bytes_borrowed,
    raw_from_bytes_borrowed_with_limits, raw_to_bytes,
};

#[test]
fn raw_layer_round_trips_runtime_style_noncanonical_flags() {
    let manifest = RawResourceManifest {
        slot_count: -1,
        groups: vec![RawResourceGroup {
            group_type: 9,
            resolution: -7,
            has_id: 2,
            has_parent: 0,
            id: None::<Vec<u8>>,
            parent: None,
            subgroups: Vec::new(),
            resources: Vec::new(),
        }],
    };

    let bytes = raw_to_bytes(&manifest).unwrap();
    let decoded = raw_from_bytes_borrowed(&bytes).unwrap();
    assert_eq!(decoded.clone().try_into_owned().unwrap(), manifest);
    assert_eq!(raw_to_bytes(&decoded).unwrap(), bytes);
}

#[test]
fn cumulative_string_budget_is_enforced() {
    let manifest = RawResourceManifest {
        slot_count: 0,
        groups: vec![RawResourceGroup {
            group_type: 1,
            resolution: 0,
            has_id: 1,
            has_parent: 1,
            id: Some(vec![b'A'; 8]),
            parent: Some(vec![b'B'; 8]),
            subgroups: Vec::new(),
            resources: Vec::new(),
        }],
    };
    let bytes = raw_to_bytes(&manifest).unwrap();
    let limits = DecodeLimits {
        max_total_string_bytes: 15,
        ..DecodeLimits::default()
    };
    let error = raw_from_bytes_borrowed_with_limits(&bytes, limits).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::TotalStringLimitExceeded {
            requested: 16,
            limit: 15
        }
    ));
}

#[test]
fn allocation_budget_is_enforced_before_vector_reservation() {
    let bytes = [0_i32.to_le_bytes(), 1_i32.to_le_bytes()].concat();
    let limits = DecodeLimits {
        max_allocation_bytes: 0,
        ..DecodeLimits::default()
    };
    let error = raw_from_bytes_borrowed_with_limits(&bytes, limits).unwrap_err();
    assert!(matches!(
        error.root_cause(),
        NewtonError::AllocationLimitExceeded { limit: 0, .. }
    ));
}
