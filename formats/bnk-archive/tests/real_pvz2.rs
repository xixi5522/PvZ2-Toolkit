use std::fs;
use std::path::{Path, PathBuf};

use bnk_archive::{
    BankChunk, BankVersion, HierarchyBody, HierarchyFields, HierarchyKind, SoundBank, from_bytes,
    to_bytes,
};

fn assert_structured(fields: &HierarchyFields, path: &Path, kind: HierarchyKind) {
    assert!(
        fields
            .fields
            .iter()
            .all(|field| field.path != "unparsed_version_payload"
                && field.path != "action_specific.unclassified"),
        "{} contains an unparsed {kind:?} payload",
        path.display()
    );
}

fn collect_banks(directory: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_banks(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bnk"))
        {
            output.push(path);
        }
    }
}

fn assert_bank_is_structured(bank: &SoundBank, path: &Path) -> usize {
    let mut hierarchy_objects = 0;
    for objects in bank.chunks.iter().filter_map(|chunk| match chunk {
        BankChunk::Hierarchy(objects) => Some(objects),
        _ => None,
    }) {
        hierarchy_objects += objects.len();
        for object in objects {
            match &object.body {
                HierarchyBody::EventAction(value) => {
                    assert_structured(&value.payload, path, object.kind)
                }
                HierarchyBody::DialogueEvent(value) => {
                    assert_structured(&value.association, path, object.kind)
                }
                HierarchyBody::Sound(value) => {
                    assert_structured(&value.settings, path, object.kind)
                }
                HierarchyBody::Effect(value)
                | HierarchyBody::Source(value)
                | HierarchyBody::AudioDevice(value) => {
                    assert_structured(&value.settings, path, object.kind)
                }
                HierarchyBody::AudioBus(value) | HierarchyBody::AuxiliaryAudioBus(value) => {
                    assert_structured(&value.settings, path, object.kind)
                }
                HierarchyBody::Attenuation(value)
                | HierarchyBody::LowFrequencyOscillatorModulator(value)
                | HierarchyBody::EnvelopeModulator(value)
                | HierarchyBody::TimeModulator(value)
                | HierarchyBody::SoundPlaylistContainer(value)
                | HierarchyBody::SoundSwitchContainer(value)
                | HierarchyBody::SoundBlendContainer(value)
                | HierarchyBody::ActorMixer(value)
                | HierarchyBody::MusicTrack(value)
                | HierarchyBody::MusicSegment(value)
                | HierarchyBody::MusicPlaylistContainer(value)
                | HierarchyBody::MusicSwitchContainer(value) => {
                    assert_structured(value, path, object.kind)
                }
                HierarchyBody::Raw(_) => assert_eq!(
                    object.kind,
                    HierarchyKind::Unknown,
                    "{} retains raw bytes for known {:?}",
                    path.display(),
                    object.kind
                ),
                HierarchyBody::StatefulPropertySetting(_) | HierarchyBody::Event(_) => {}
            }
        }
    }
    hierarchy_objects
}

fn validate_corpus(root: &Path, version: BankVersion) -> Option<(usize, usize)> {
    if !root.exists() {
        eprintln!("skipping real BNK test: {} is unavailable", root.display());
        return None;
    }
    let mut paths = Vec::new();
    collect_banks(root, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no BNK fixtures found");

    let mut hierarchy_objects = 0usize;
    for path in &paths {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", path.display());
        });
        let bank = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode {}: {error}", path.display());
        });
        assert_eq!(
            bank.version(),
            version,
            "unexpected version in {}",
            path.display()
        );
        hierarchy_objects += assert_bank_is_structured(&bank, path);
        let encoded = to_bytes(&bank).unwrap_or_else(|error| {
            panic!("failed to encode {}: {error}", path.display());
        });
        assert_eq!(
            encoded,
            bytes,
            "byte-exact round trip changed {}",
            path.display()
        );
    }
    Some((paths.len(), hierarchy_objects))
}

#[test]
fn pvz2_12_7_1_banks_round_trip_byte_exactly() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../pvz2/test_data/12.7.1_unpacked");
    let Some((banks, hierarchy_objects)) = validate_corpus(&root, BankVersion::V140) else {
        return;
    };

    assert_eq!(banks, 399);
    assert_eq!(hierarchy_objects, 19_944);
}

#[test]
fn pvz2_wwise_2014_banks_round_trip_byte_exactly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../pvz2-toolkit/test_data");
    let Some((banks, hierarchy_objects)) = validate_corpus(&root, BankVersion::V112) else {
        return;
    };

    assert_eq!(banks, 87);
    assert_eq!(hierarchy_objects, 6_462);
}
