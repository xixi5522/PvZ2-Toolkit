use bnk_archive::{HierarchyField, HierarchyFieldValue, HierarchyFields};

#[test]
fn hierarchy_fields_are_addressable_editable_and_wire_stable() {
    let mut fields = HierarchyFields {
        fields: vec![
            HierarchyField {
                path: "properties.regular_count".into(),
                value: HierarchyFieldValue::U8(1),
                semantic: None,
            },
            HierarchyField {
                path: "properties.regular[0].property_id".into(),
                value: HierarchyFieldValue::U8(0),
                semantic: None,
            },
            HierarchyField {
                path: "properties.regular[0].value_for_0".into(),
                value: HierarchyFieldValue::PropertyValue(1.5_f32.to_bits()),
                semantic: None,
            },
        ],
    };

    assert_eq!(
        fields
            .get("properties.regular[0].value_for_0")
            .and_then(HierarchyFieldValue::property_as_f32),
        Some(1.5)
    );
    assert!(
        fields
            .get_mut("properties.regular[0].value_for_0")
            .unwrap()
            .set_property_f32(-3.0)
    );

    assert_eq!(fields.to_bytes().unwrap(), [1, 0, 0, 0, 64, 192],);

    let previous = fields
        .replace_value(
            "properties.regular[0].value_for_0",
            HierarchyFieldValue::PropertyValue(2.0_f32.to_bits()),
        )
        .unwrap();
    assert_eq!(previous.property_as_f32(), Some(-3.0));
    assert!(
        fields
            .replace_value(
                "properties.regular[0].value_for_0",
                HierarchyFieldValue::U32(0),
            )
            .is_err()
    );

    let index = fields.build_index();
    assert_eq!(
        index
            .first("properties.regular[0].value_for_0")
            .unwrap()
            .value
            .property_as_f32(),
        Some(2.0)
    );
}
