use newton_manifest::{
    Resource, ResourceGroup, ResourceManifest, ResourceType, SimpleGroup, from_bytes, to_bytes,
};
use proptest::prelude::*;

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..24)
        .prop_map(|characters| characters.into_iter().collect())
}

proptest! {
    #[test]
    fn semantic_manifests_round_trip(
        records in prop::collection::vec((1_u8..=7, text(), text(), any::<bool>()), 0..32)
    ) {
        let resources = records
            .into_iter()
            .enumerate()
            .map(|(slot, (kind, id, path, atlas_requested))| {
                let resource_type = ResourceType::try_from(kind).unwrap();
                let atlas = resource_type == ResourceType::Image && atlas_requested;
                Resource {
                    resource_type,
                    slot: slot as u32,
                    id,
                    path,
                    width: atlas.then_some(64),
                    height: atlas.then_some(64),
                    x: None,
                    y: None,
                    ax: None,
                    ay: None,
                    aw: None,
                    ah: None,
                    cols: None,
                    rows: None,
                    atlas,
                    parent: None,
                }
            })
            .collect::<Vec<_>>();
        let manifest = ResourceManifest {
            slot_count: resources.len() as u32,
            groups: vec![ResourceGroup::Simple(SimpleGroup {
                id: "Generated".into(),
                resolution: None,
                parent: None,
                resources,
            })],
        };

        let bytes = to_bytes(&manifest).unwrap();
        prop_assert_eq!(from_bytes(&bytes).unwrap(), manifest);
    }
}
