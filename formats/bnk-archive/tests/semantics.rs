use bnk_archive::{
    BankVersion, CommonPropertyDefault, CommonPropertyDomain, CommonPropertyValue, Curve,
    CurveInterpolation, EnumerationKind, HierarchyField, HierarchyFieldSemantic,
    HierarchyFieldValue, HierarchyFields, PackedLayout, common_property_descriptor,
};

#[test]
fn common_property_catalog_tracks_every_identifier_boundary() {
    let cases = [
        (72, CommonPropertyDomain::Audio, 4, "bus_volume"),
        (
            88,
            CommonPropertyDomain::Audio,
            33,
            "voice_volume_make_up_gain",
        ),
        (
            112,
            CommonPropertyDomain::Audio,
            59,
            "playback_initial_delay",
        ),
        (118, CommonPropertyDomain::Audio, 24, "output_bus_volume"),
        (
            128,
            CommonPropertyDomain::Audio,
            68,
            "game_defined_auxiliary_send_low_pass_filter",
        ),
        (
            132,
            CommonPropertyDomain::Audio,
            70,
            "positioning_listener_routing_attenuation_identifier",
        ),
        (
            135,
            CommonPropertyDomain::Audio,
            72,
            "early_reflection_auxiliary_send_volume",
        ),
        (
            140,
            CommonPropertyDomain::Audio,
            73,
            "positioning_speaker_panning_z",
        ),
        (
            150,
            CommonPropertyDomain::Audio,
            85,
            "positioning_listener_routing_attenuation_identifier",
        ),
        (112, CommonPropertyDomain::Modulator, 9, "attack_time"),
        (150, CommonPropertyDomain::Modulator, 10, "attack_time"),
        (117, CommonPropertyDomain::EventAction, 14, "delay"),
        (118, CommonPropertyDomain::EventAction, 15, "delay"),
        (150, CommonPropertyDomain::EventAction, 57, "delay"),
    ];
    for (version, domain, id, name) in cases {
        assert_eq!(
            common_property_descriptor(version, domain, id)
                .expect("known Twinning property")
                .name,
            name
        );
    }

    assert_eq!(
        common_property_descriptor(112, CommonPropertyDomain::Audio, 45)
            .unwrap()
            .default,
        CommonPropertyDefault::Integer(60)
    );
    assert!(common_property_descriptor(117, CommonPropertyDomain::EventAction, 17).is_none());
    assert!(common_property_descriptor(149, CommonPropertyDomain::Audio, 85).is_none());
}

#[test]
fn semantic_common_property_is_typed_and_editable_without_wire_changes() {
    let mut field = HierarchyField {
        path: "properties.regular[0].value_for_45".into(),
        value: HierarchyFieldValue::PropertyValue(60),
        semantic: Some(HierarchyFieldSemantic::CommonProperty {
            domain: CommonPropertyDomain::Audio,
            version: 112,
            property_id: 45,
        }),
    };

    assert_eq!(
        field.common_property_value(),
        Some(CommonPropertyValue::Integer(60))
    );
    assert!(field.set_common_property_value(CommonPropertyValue::Integer(64)));
    assert!(!field.set_common_property_value(CommonPropertyValue::Floater(64.0)));
    assert_eq!(field.value.property_as_i32(), Some(64));
}

#[test]
fn packed_members_preserve_neighbors_and_validate_reserved_bits() {
    let mut field = HierarchyField {
        path: "node.priority_and_midi_flags".into(),
        value: HierarchyFieldValue::U8(0b0010_0011),
        semantic: Some(HierarchyFieldSemantic::Packed {
            layout: PackedLayout::PriorityAndMidi,
        }),
    };
    let before = match field.value {
        HierarchyFieldValue::U8(value) => value,
        _ => unreachable!(),
    };
    assert!(field.set_packed_member("override_midi_event", 1));
    let after = match field.value {
        HierarchyFieldValue::U8(value) => value,
        _ => unreachable!(),
    };
    assert_eq!(after & !0b100, before & !0b100);

    let invalid = HierarchyFields {
        fields: vec![HierarchyField {
            path: "source.flags".into(),
            value: HierarchyFieldValue::U8(0b10),
            semantic: Some(HierarchyFieldSemantic::Packed {
                layout: PackedLayout::Source,
            }),
        }],
    };
    assert!(invalid.validate_semantics().is_err());
    // Lossless writing remains available for permissively decoded banks.
    assert_eq!(invalid.to_bytes().unwrap(), [0b10]);
}

#[test]
fn twinning_enumerations_follow_their_exact_version_boundaries() {
    assert_eq!(Curve(0).interpolation(), CurveInterpolation::Logarithmic3);
    assert_eq!(Curve(8).interpolation(), CurveInterpolation::Exponential3);

    assert_eq!(
        EnumerationKind::EventActionType
            .variant(112, 31)
            .unwrap()
            .name,
        "release_envelope"
    );
    assert!(EnumerationKind::EventActionType.variant(112, 33).is_none());
    assert_eq!(
        EnumerationKind::EventActionType
            .variant(113, 33)
            .unwrap()
            .name,
        "post_event"
    );
    assert!(EnumerationKind::EventActionType.variant(140, 21).is_none());

    let mode = HierarchyField {
        path: "scope_and_mode".into(),
        value: HierarchyFieldValue::U8(0b1000),
        semantic: Some(HierarchyFieldSemantic::Packed {
            layout: PackedLayout::EventActionScopeAndMode72,
        }),
    };
    assert_eq!(
        mode.packed_member_enumeration_variant("mode", BankVersion::V112)
            .unwrap()
            .name,
        "all_except"
    );
    assert!(
        mode.packed_member_enumeration_variant("mode", BankVersion::V125)
            .is_none()
    );

    let playback = HierarchyField {
        path: "playback.flags".into(),
        value: HierarchyFieldValue::U8(0b101),
        semantic: Some(HierarchyFieldSemantic::Packed {
            layout: PackedLayout::PlaybackObject,
        }),
    };
    assert_eq!(
        playback
            .packed_member_enumeration_variant("when_priority_is_equal", BankVersion::V140)
            .unwrap()
            .name,
        "discard_newest_instance"
    );
    assert_eq!(
        playback
            .packed_member_enumeration_variant("scope", BankVersion::V140)
            .unwrap()
            .name,
        "globally"
    );
    assert!(PackedLayout::EventActionEffectBypassSet.is_canonical(0x1f));
    assert!(!PackedLayout::EventActionEffectBypassSet.is_canonical(0x20));
    assert!(PackedLayout::EventActionEffectBypassReset.is_canonical(0xff));
    assert!(!PackedLayout::EventActionEffectBypassReset.is_canonical(0x1f));
    assert!(PackedLayout::BusPositioning112.is_canonical(0b11));
    assert!(!PackedLayout::BusPositioning112.is_canonical(0b100));
    assert!(PackedLayout::BusHdr.is_canonical(0b11));
    assert!(!PackedLayout::BusHdr.is_canonical(0b100));
    assert!(PackedLayout::BooleanU8IgnoredHigh.is_canonical(0xfe));
    assert!(!PackedLayout::Boolean.is_canonical(0xfe));

    let mut midi = HierarchyField {
        path: "properties.regular[0].value_for_46".into(),
        value: HierarchyFieldValue::PropertyValue(2),
        semantic: Some(HierarchyFieldSemantic::CommonProperty {
            domain: CommonPropertyDomain::Audio,
            version: 112,
            property_id: 46,
        }),
    };
    assert_eq!(
        midi.common_property_enumeration_variant().unwrap().name,
        "note_off"
    );
    assert!(!midi.set_common_property_value(CommonPropertyValue::Enumerated(1)));
    assert!(midi.validate_semantic().is_ok());
    midi.value = HierarchyFieldValue::PropertyValue(1);
    assert!(midi.validate_semantic().is_err());
}
