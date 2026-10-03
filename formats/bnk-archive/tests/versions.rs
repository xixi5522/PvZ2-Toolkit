use bnk_archive::{
    AudioSourceSetting, AudioSourceType, BankChunk, BankHeader, BankVersion, BusHierarchyObject,
    DialogueEvent, Event, EventAction, GameSynchronization, GameSynchronizationU1, HierarchyBody,
    HierarchyFieldValue, HierarchyFields, HierarchyKind, HierarchyObject, PlatformSetting,
    PluginHierarchyObject, PluginReference, SUPPORTED_VERSIONS, SoundBank, SoundHierarchyObject,
    StatefulPropertySetting, VoiceFilterBehavior, from_bytes, to_bytes,
};

fn empty_game_sync(version: BankVersion) -> GameSynchronization {
    GameSynchronization {
        voice_filter_behavior: version
            .at_least(145)
            .then_some(VoiceFilterBehavior::SumAllValues),
        volume_threshold: -96.3,
        maximum_voice_instances: 256,
        compatibility_value: version.at_least(128).then_some(50),
        state_groups: Vec::new(),
        switch_groups: Vec::new(),
        game_parameters: Vec::new(),
        u1: Vec::new(),
    }
}

fn zeros(bytes: &mut Vec<u8>, count: usize) {
    bytes.resize(bytes.len() + count, 0);
}

fn minimal_parameter_node(version: BankVersion) -> Vec<u8> {
    parameter_node_with_positioning(version, &[0])
}

fn parameter_node_with_positioning(version: BankVersion, positioning: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0, 0]; // effect override and slot count
    if version.at_least(140) {
        bytes.extend_from_slice(&[0, 0]); // metadata override and slot count
    }
    if version.at_least(112) && version.before(150) {
        bytes.push(0); // mixer override
    }
    zeros(&mut bytes, 8); // output bus and parent
    zeros(&mut bytes, if version.before(112) { 2 } else { 1 });
    zeros(&mut bytes, 2); // regular and randomized property counts
    bytes.extend_from_slice(positioning);
    zeros(
        &mut bytes,
        if version.before(112) {
            4
        } else if version.before(135) {
            1
        } else {
            5
        },
    );
    zeros(&mut bytes, if version.before(112) { 9 } else { 5 });
    zeros(
        &mut bytes,
        if version.before(88) {
            0
        } else if version.before(112) {
            4
        } else {
            1
        },
    );
    zeros(&mut bytes, if version.before(125) { 4 } else { 2 });
    zeros(&mut bytes, 2); // RTPC count
    bytes
}

fn automated_positioning(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    if version.before(112) {
        bytes.push(1); // override
        if version.at_least(88) {
            bytes.push(0); // 3D discriminator
        }
        bytes.push(1); // 3D positioning
        bytes.push(if version.before(88) { 0b10 } else { 0 }); // user-defined source
        zeros(&mut bytes, 3);
        zeros(&mut bytes, 4); // attenuation identifier
        bytes.push(0); // spatialization
        bytes.push(0); // path mode
        zeros(&mut bytes, 3);
        bytes.push(0); // loop
        zeros(&mut bytes, 4); // transition time
        bytes.push(0); // hold listener orientation
    } else if version.before(132) {
        bytes.push(if version.before(125) { 0x08 } else { 0x10 });
        bytes.push(0); // user-defined source mode in every packed layout
        zeros(&mut bytes, 4); // attenuation identifier
        bytes.push(0); // automation path mode
        zeros(&mut bytes, 4); // transition time
    } else {
        bytes.push(0x22); // listener routing enabled, emitter automation source
        bytes.push(0); // listener routing details
        bytes.push(0); // automation path mode
        zeros(&mut bytes, 4); // transition time
    }
    zeros(&mut bytes, 8); // vertex and path counts
    bytes
}

fn minimal_bus(version: BankVersion) -> Vec<u8> {
    let mut bytes = vec![0]; // regular property count
    if version.at_least(88) && version.before(112) {
        zeros(&mut bytes, 2);
    } else if version.at_least(112) {
        bytes.push(u8::from(version.at_least(125))); // bus positioning must override
    }
    if version.at_least(125) {
        bytes.push(if version.before(135) { 0x05 } else { 0x15 });
        if version.at_least(135) {
            zeros(&mut bytes, 4); // early-reflection bus
        }
    }
    zeros(&mut bytes, if version.before(112) { 5 } else { 3 });
    if version.before(88) {
        bytes.extend_from_slice(&63_u32.to_le_bytes());
    } else {
        zeros(&mut bytes, 4); // bus configuration
    }
    if version.at_least(88) {
        zeros(&mut bytes, if version.before(112) { 2 } else { 1 });
    }
    zeros(&mut bytes, 12); // automatic ducking values and count
    bytes.push(0); // effect count
    if version.at_least(112) && version.before(150) {
        zeros(&mut bytes, 6); // mixer identifier and reserved u16
    }
    if version.at_least(140) {
        bytes.push(0); // metadata count
    }
    zeros(&mut bytes, 2); // RTPC count
    zeros(&mut bytes, if version.before(125) { 4 } else { 2 });
    bytes
}

fn minimal_attenuation(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    if version.at_least(140) {
        bytes.push(0);
    }
    bytes.push(0); // cone disabled
    zeros(
        &mut bytes,
        if version.before(88) {
            4
        } else if version.before(112) {
            5
        } else if version.before(145) {
            7
        } else {
            19
        },
    );
    bytes.push(0); // curve count
    zeros(&mut bytes, 2); // RTPC count
    bytes
}

fn minimal_plugin(version: BankVersion, audio_device: bool) -> Vec<u8> {
    let mut bytes = vec![0]; // reserved
    zeros(&mut bytes, 2); // RTPC count
    if version.at_least(125) && version.before(128) {
        zeros(&mut bytes, 2);
    }
    if version.at_least(128) {
        zeros(&mut bytes, 2); // state attribute and group counts
    }
    if version.at_least(112) {
        zeros(&mut bytes, 2); // unknown/effect-value list count
    }
    if audio_device && version.at_least(140) {
        bytes.push(0); // effect count
    }
    bytes
}

fn minimal_playlist_container(version: BankVersion) -> Vec<u8> {
    let mut bytes = minimal_parameter_node(version);
    zeros(&mut bytes, 2);
    if version.at_least(88) {
        zeros(&mut bytes, 4);
    }
    zeros(&mut bytes, 12);
    zeros(&mut bytes, 5);
    zeros(&mut bytes, if version.before(112) { 5 } else { 1 });
    zeros(&mut bytes, 4); // child count
    zeros(&mut bytes, 2); // playlist count
    bytes
}

fn minimal_switch_container(version: BankVersion) -> Vec<u8> {
    let mut bytes = minimal_parameter_node(version);
    zeros(&mut bytes, if version.before(112) { 4 } else { 1 });
    zeros(&mut bytes, 9); // group/default identifiers and mode
    zeros(&mut bytes, 12); // child, assignment, and attribute counts
    bytes
}

fn minimal_blend_container(version: BankVersion) -> Vec<u8> {
    let mut bytes = minimal_parameter_node(version);
    zeros(&mut bytes, 8); // child and layer counts
    if version.at_least(120) {
        bytes.push(0);
    }
    bytes
}

fn append_minimal_music_common(bytes: &mut Vec<u8>, version: BankVersion) {
    if version.at_least(112) {
        bytes.push(0); // MIDI flags
    }
    bytes.extend_from_slice(&minimal_parameter_node(version));
    zeros(bytes, 4); // child count
    zeros(bytes, 23); // meter/time setting
    zeros(bytes, 4); // stinger count
}

fn minimal_music_track(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    if version.at_least(112) {
        bytes.push(0);
    }
    zeros(&mut bytes, 12); // source, clip, and automation counts
    bytes.extend_from_slice(&minimal_parameter_node(version));
    zeros(&mut bytes, if version.before(112) { 4 } else { 1 });
    zeros(&mut bytes, 4); // look-ahead and reserved
    bytes
}

fn switch_music_track(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(0); // MIDI flags
    zeros(&mut bytes, 12); // source, clip, and automation counts
    bytes.extend_from_slice(&minimal_parameter_node(version));
    bytes.push(3); // switch track
    bytes.extend_from_slice(&1u32.to_le_bytes());
    zeros(&mut bytes, 36); // switcher, source transition, and destination transition
    zeros(&mut bytes, 4); // look-ahead and reserved
    bytes
}

fn minimal_music_segment(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_minimal_music_common(&mut bytes, version);
    zeros(&mut bytes, 12); // duration and cue count
    bytes
}

fn minimal_music_playlist(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_minimal_music_common(&mut bytes, version);
    zeros(&mut bytes, 8); // transition and playlist counts
    bytes
}

fn minimal_music_switch(version: BankVersion) -> Vec<u8> {
    let mut bytes = Vec::new();
    append_minimal_music_common(&mut bytes, version);
    zeros(&mut bytes, 4); // transition count
    if version.before(88) {
        zeros(&mut bytes, 12); // switcher category/group/default
        bytes.push(0); // continue playback
        zeros(&mut bytes, 4); // legacy association count
    } else {
        bytes.push(0); // continue playback
        zeros(&mut bytes, 4); // argument count
        zeros(&mut bytes, 4); // decision tree byte size
        bytes.push(0); // decision mode
    }
    bytes
}

fn hierarchy_object(
    version: BankVersion,
    id: u32,
    kind: HierarchyKind,
    body: HierarchyBody,
) -> HierarchyObject {
    HierarchyObject {
        kind,
        type_code: kind.code(version).expect("kind must exist in this version"),
        id,
        body,
    }
}

fn opaque(bytes: Vec<u8>) -> HierarchyFields {
    HierarchyFields::from_opaque_bytes(bytes)
}

fn append_empty_exceptions(bytes: &mut Vec<u8>, version: BankVersion) {
    zeros(bytes, if version.before(125) { 4 } else { 1 });
}

fn event_action_payload(version: BankVersion, action_type: u8) -> Vec<u8> {
    let mut bytes = vec![0, 0]; // regular and randomized common-property counts
    match action_type {
        1 => {
            bytes.push(0);
            if version.at_least(125) {
                bytes.push(0);
            }
            append_empty_exceptions(&mut bytes, version);
        }
        2 | 3 => {
            zeros(&mut bytes, 2);
            append_empty_exceptions(&mut bytes, version);
        }
        4 => {
            zeros(&mut bytes, 5);
            if version.at_least(145) {
                zeros(&mut bytes, 4);
            }
        }
        6 | 7 => {
            bytes.push(0);
            append_empty_exceptions(&mut bytes, version);
        }
        8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 32 | 48 => {
            bytes.extend_from_slice(&[0, 1]); // valid fade, absolute apply mode
            zeros(&mut bytes, 12);
            append_empty_exceptions(&mut bytes, version);
        }
        16 | 17 | 28 | 29 | 31 | 33 => {}
        18 | 25 => zeros(&mut bytes, 8),
        19 | 20 => {
            bytes.push(0);
            if version.at_least(112) {
                bytes.push(0);
            }
            bytes.push(1); // absolute apply mode
            zeros(&mut bytes, 12);
            append_empty_exceptions(&mut bytes, version);
        }
        26 | 27 => {
            bytes.push(0);
            bytes.push(if action_type == 27 { 0xe0 } else { 0 });
            append_empty_exceptions(&mut bytes, version);
        }
        30 => {
            zeros(&mut bytes, 14);
            append_empty_exceptions(&mut bytes, version);
        }
        34 => {
            bytes.push(4);
            zeros(&mut bytes, if version.before(115) { 4 } else { 1 });
        }
        _ => unreachable!("test does not define action type {action_type}"),
    }
    bytes
}

#[test]
fn hierarchy_wire_codes_follow_wwise_version_boundaries() {
    assert_eq!(
        HierarchyKind::from_code(BankVersion::V72, 18),
        HierarchyKind::Effect
    );
    assert_eq!(
        HierarchyKind::from_code(BankVersion::V112, 21),
        HierarchyKind::LowFrequencyOscillatorModulator
    );
    assert_eq!(
        HierarchyKind::from_code(BankVersion::V128, 21),
        HierarchyKind::AudioDevice
    );
    assert_eq!(
        HierarchyKind::from_code(BankVersion::V132, 22),
        HierarchyKind::TimeModulator
    );
    assert_eq!(
        HierarchyKind::from_code(BankVersion::V140, 4),
        HierarchyKind::Event
    );
}

#[test]
fn every_supported_version_uses_its_exact_chunk_layout() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let mut chunks = vec![BankChunk::GameSynchronization(empty_game_sync(version))];
        if version.at_least(113) {
            chunks.push(BankChunk::Platform(PlatformSetting {
                name: "Apple".into(),
            }));
        }
        if version.at_least(118) {
            chunks.push(BankChunk::Plugins(vec![PluginReference {
                id: 7,
                library: "AkVorbis".into(),
            }]));
        }
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: vec![0xaa, 0x55],
            },
            chunks,
        };

        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap();
        assert_eq!(decoded, bank, "semantic round trip failed for {number}");
        assert_eq!(
            to_bytes(&decoded).unwrap(),
            bytes,
            "byte round trip failed for {number}"
        );
    }
}

#[test]
fn stmg_tail_uses_twinning_constants_and_neutral_u1_records() {
    for version in [BankVersion::V120, BankVersion::V125] {
        let settings = empty_game_sync(version);
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: 1,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::GameSynchronization(settings)],
        };
        let bytes = to_bytes(&bank).unwrap();
        assert_eq!(from_bytes(&bytes).unwrap(), bank);
    }

    let mut settings = empty_game_sync(BankVersion::V140);
    settings.u1.push(GameSynchronizationU1 {
        identifier: 3,
        u1: 0.1,
        u2: 0.2,
        u3: 0.3,
        u4: 0.4,
        u5: 0.5,
        u6: 0.6,
    });
    let bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V140,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::GameSynchronization(settings)],
    };
    let bytes = to_bytes(&bank).unwrap();
    assert_eq!(from_bytes(&bytes).unwrap(), bank);
}

#[test]
fn dialogue_reserved_word_follows_the_wwise_120_boundary() {
    for version in [BankVersion::V112, BankVersion::V140] {
        let mut association = vec![
            0, 0, 0, 0, // tree depth
            0, 0, 0, 0, // tree byte size
            0, // decision mode
        ];
        if version.at_least(120) {
            association.extend_from_slice(&[0, 0]); // fixed reserved u16
        }
        let kind = HierarchyKind::DialogueEvent;
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: 1,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(vec![HierarchyObject {
                kind,
                type_code: kind.code(version).unwrap(),
                id: 2,
                body: HierarchyBody::DialogueEvent(DialogueEvent {
                    probability: Some(100),
                    association: HierarchyFields::from_opaque_bytes(association),
                }),
            }])],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap();
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        let HierarchyBody::DialogueEvent(dialogue) = &objects[0].body else {
            unreachable!()
        };
        assert!(!dialogue.association.is_opaque());
        assert_eq!(to_bytes(&decoded).unwrap(), bytes);
    }
}

#[test]
fn every_wwise_112_hirc_family_has_a_structured_layout() {
    let version = BankVersion::V112;
    let opaque = |bytes: Vec<u8>| HierarchyFields::from_opaque_bytes(bytes);
    let bus_settings = vec![
        0, // properties
        0, 0, 0, 0, 0, 0, 0, 0, 0, // routing/limits/config/HDR
        0, 0, 0, 0, // duck recovery
        0, 0, 0, 0, // maximum duck volume
        0, 0, 0, 0, // duck count
        0, // effects
        0, 0, 0, 0, 0, 0, // mixer identifier and reserved u16
        0, 0, // RTPC
        0, 0, 0, 0, // state
    ];
    let fields = vec![
        (
            HierarchyKind::AudioBus,
            HierarchyBody::AudioBus(BusHierarchyObject {
                parent: 0,
                audio_device: None,
                settings: opaque(bus_settings.clone()),
            }),
        ),
        (
            HierarchyKind::AuxiliaryAudioBus,
            HierarchyBody::AuxiliaryAudioBus(BusHierarchyObject {
                parent: 1,
                audio_device: None,
                settings: opaque(bus_settings),
            }),
        ),
        (
            HierarchyKind::Attenuation,
            HierarchyBody::Attenuation(opaque(vec![
                0, // cone
                0, 0, 0, 0, 0, 0, 0, // curve slots
                0, // curves
                0, 0, // RTPC
            ])),
        ),
        (
            HierarchyKind::LowFrequencyOscillatorModulator,
            HierarchyBody::LowFrequencyOscillatorModulator(opaque(vec![0, 0, 0, 0])),
        ),
        (
            HierarchyKind::EnvelopeModulator,
            HierarchyBody::EnvelopeModulator(opaque(vec![0, 0, 0, 0])),
        ),
        (
            HierarchyKind::Effect,
            HierarchyBody::Effect(PluginHierarchyObject {
                plugin: 1,
                expand: Vec::new(),
                settings: opaque(vec![0, 0, 0, 0, 0]),
            }),
        ),
        (
            HierarchyKind::Source,
            HierarchyBody::Source(PluginHierarchyObject {
                plugin: 1,
                expand: Vec::new(),
                settings: opaque(vec![0, 0, 0, 0, 0]),
            }),
        ),
    ];
    let objects = fields
        .into_iter()
        .enumerate()
        .map(|(index, (kind, body))| HierarchyObject {
            kind,
            type_code: kind.code(version).unwrap(),
            id: index as u32 + 1,
            body,
        })
        .collect();
    let bank = SoundBank {
        header: BankHeader {
            version,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::Hierarchy(objects)],
    };
    let bytes = to_bytes(&bank).unwrap();
    let decoded = from_bytes(&bytes).unwrap();
    let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
        unreachable!()
    };
    for object in objects {
        let fields = match &object.body {
            HierarchyBody::AudioBus(value) | HierarchyBody::AuxiliaryAudioBus(value) => {
                &value.settings
            }
            HierarchyBody::Attenuation(value)
            | HierarchyBody::LowFrequencyOscillatorModulator(value)
            | HierarchyBody::EnvelopeModulator(value) => value,
            HierarchyBody::Effect(value) | HierarchyBody::Source(value) => &value.settings,
            _ => unreachable!(),
        };
        assert!(!fields.is_opaque(), "{:?} remained opaque", object.kind);
    }
    assert_eq!(to_bytes(&decoded).unwrap(), bytes);
}

#[test]
fn every_twinning_version_decodes_all_of_its_hirc_families() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let mut next_id = 1;
        let mut object = |kind, body| {
            let value = hierarchy_object(version, next_id, kind, body);
            next_id += 1;
            value
        };
        let mut association = vec![
            0, 0, 0, 0, // argument depth
            0, 0, 0, 0, // tree size
        ];
        if version.before(88) {
            association.push(100); // probability moved into the association in Wwise 72
        }
        association.push(0); // decision mode
        if version.at_least(120) {
            association.extend_from_slice(&[0, 0]);
        }

        let source = AudioSourceSetting {
            plugin: 1,
            source_type: AudioSourceType::Embedded,
            resource: 100,
            source: version.before(113).then_some(101),
            resource_offset: version.before(113).then_some(0),
            resource_size: Some(16),
            flags: 0,
            plugin_reserved: None,
        };
        let plugin = |audio_device| PluginHierarchyObject {
            plugin: 1,
            expand: Vec::new(),
            settings: opaque(minimal_plugin(version, audio_device)),
        };
        let bus = || BusHierarchyObject {
            parent: 1,
            audio_device: None,
            settings: opaque(minimal_bus(version)),
        };

        let mut objects = vec![
            object(
                HierarchyKind::StatefulPropertySetting,
                HierarchyBody::StatefulPropertySetting(StatefulPropertySetting {
                    values: Vec::new(),
                }),
            ),
            object(
                HierarchyKind::EventAction,
                HierarchyBody::EventAction(EventAction {
                    scope_and_mode: 0,
                    action_type: 16,
                    target: 0,
                    u1: 0,
                    payload: opaque(vec![0, 0]),
                }),
            ),
            object(
                HierarchyKind::Event,
                HierarchyBody::Event(Event {
                    actions: Vec::new(),
                }),
            ),
            object(
                HierarchyKind::DialogueEvent,
                HierarchyBody::DialogueEvent(DialogueEvent {
                    probability: version.at_least(88).then_some(100),
                    association: opaque(association),
                }),
            ),
            object(
                HierarchyKind::Attenuation,
                HierarchyBody::Attenuation(opaque(minimal_attenuation(version))),
            ),
            object(HierarchyKind::Effect, HierarchyBody::Effect(plugin(false))),
            object(HierarchyKind::Source, HierarchyBody::Source(plugin(false))),
            object(HierarchyKind::AudioBus, HierarchyBody::AudioBus(bus())),
            object(
                HierarchyKind::AuxiliaryAudioBus,
                HierarchyBody::AuxiliaryAudioBus(bus()),
            ),
            object(
                HierarchyKind::Sound,
                HierarchyBody::Sound(SoundHierarchyObject {
                    source,
                    settings: opaque(minimal_parameter_node(version)),
                }),
            ),
            object(
                HierarchyKind::SoundPlaylistContainer,
                HierarchyBody::SoundPlaylistContainer(opaque(minimal_playlist_container(version))),
            ),
            object(
                HierarchyKind::SoundSwitchContainer,
                HierarchyBody::SoundSwitchContainer(opaque(minimal_switch_container(version))),
            ),
            object(
                HierarchyKind::SoundBlendContainer,
                HierarchyBody::SoundBlendContainer(opaque(minimal_blend_container(version))),
            ),
            object(
                HierarchyKind::ActorMixer,
                HierarchyBody::ActorMixer(opaque({
                    let mut bytes = minimal_parameter_node(version);
                    zeros(&mut bytes, 4);
                    bytes
                })),
            ),
            object(
                HierarchyKind::MusicTrack,
                HierarchyBody::MusicTrack(opaque(minimal_music_track(version))),
            ),
            object(
                HierarchyKind::MusicSegment,
                HierarchyBody::MusicSegment(opaque(minimal_music_segment(version))),
            ),
            object(
                HierarchyKind::MusicPlaylistContainer,
                HierarchyBody::MusicPlaylistContainer(opaque(minimal_music_playlist(version))),
            ),
            object(
                HierarchyKind::MusicSwitchContainer,
                HierarchyBody::MusicSwitchContainer(opaque(minimal_music_switch(version))),
            ),
        ];
        if version.at_least(112) {
            objects.push(object(
                HierarchyKind::LowFrequencyOscillatorModulator,
                HierarchyBody::LowFrequencyOscillatorModulator(opaque(vec![0, 0, 0, 0])),
            ));
            objects.push(object(
                HierarchyKind::EnvelopeModulator,
                HierarchyBody::EnvelopeModulator(opaque(vec![0, 0, 0, 0])),
            ));
        }
        if version.at_least(128) {
            objects.push(object(
                HierarchyKind::AudioDevice,
                HierarchyBody::AudioDevice(plugin(true)),
            ));
        }
        if version.at_least(132) {
            objects.push(object(
                HierarchyKind::TimeModulator,
                HierarchyBody::TimeModulator(opaque(vec![0, 0, 0, 0])),
            ));
        }

        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(objects)],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode synthetic Wwise {number} HIRC matrix: {error}")
        });
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        for object in objects {
            let fields = match &object.body {
                HierarchyBody::EventAction(value) => Some(&value.payload),
                HierarchyBody::DialogueEvent(value) => Some(&value.association),
                HierarchyBody::Sound(value) => Some(&value.settings),
                HierarchyBody::Effect(value)
                | HierarchyBody::Source(value)
                | HierarchyBody::AudioDevice(value) => Some(&value.settings),
                HierarchyBody::AudioBus(value) | HierarchyBody::AuxiliaryAudioBus(value) => {
                    Some(&value.settings)
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
                | HierarchyBody::MusicSwitchContainer(value) => Some(value),
                HierarchyBody::StatefulPropertySetting(_) | HierarchyBody::Event(_) => None,
                HierarchyBody::Raw(_) => panic!(
                    "Wwise {number} {:?} unexpectedly decoded as raw",
                    object.kind
                ),
            };
            if let Some(fields) = fields {
                assert!(
                    !fields.is_opaque(),
                    "Wwise {number} {:?} remained opaque",
                    object.kind
                );
            }
        }
        assert_eq!(
            to_bytes(&decoded).unwrap(),
            bytes,
            "Wwise {number} HIRC matrix was not byte-exact"
        );
    }
}

#[test]
fn every_twinning_event_action_layout_is_structured_and_lossless() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let mut action_types = vec![
            1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 25, 26, 27, 28, 29,
            30,
        ];
        if version.at_least(112) {
            action_types.extend_from_slice(&[31, 32, 48]);
        }
        if version.at_least(113) {
            action_types.extend_from_slice(&[33, 34]);
        }
        let objects = action_types
            .into_iter()
            .enumerate()
            .map(|(index, action_type)| {
                hierarchy_object(
                    version,
                    index as u32 + 1,
                    HierarchyKind::EventAction,
                    HierarchyBody::EventAction(EventAction {
                        scope_and_mode: 0,
                        action_type,
                        target: 0,
                        u1: 0,
                        payload: opaque(event_action_payload(version, action_type)),
                    }),
                )
            })
            .collect();
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(objects)],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode Wwise {number} event-action matrix: {error}")
        });
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        for object in objects {
            let HierarchyBody::EventAction(action) = &object.body else {
                unreachable!()
            };
            assert!(
                !action.payload.is_opaque(),
                "Wwise {number} action {} remained opaque",
                action.action_type
            );
        }
        assert_eq!(to_bytes(&decoded).unwrap(), bytes);
    }
}

#[test]
fn structured_hirc_edits_are_revalidated_before_writing() {
    let version = BankVersion::V140;
    let bank = SoundBank {
        header: BankHeader {
            version,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::Hierarchy(vec![hierarchy_object(
            version,
            1,
            HierarchyKind::Sound,
            HierarchyBody::Sound(SoundHierarchyObject {
                source: AudioSourceSetting {
                    plugin: 1,
                    source_type: AudioSourceType::Embedded,
                    resource: 2,
                    source: None,
                    resource_offset: None,
                    resource_size: Some(16),
                    flags: 0,
                    plugin_reserved: None,
                },
                settings: opaque(minimal_parameter_node(version)),
            }),
        )])],
    };
    let mut decoded = from_bytes(&to_bytes(&bank).unwrap()).unwrap();
    let BankChunk::Hierarchy(objects) = &mut decoded.chunks[0] else {
        unreachable!()
    };
    let HierarchyBody::Sound(sound) = &mut objects[0].body else {
        unreachable!()
    };
    *sound
        .settings
        .get_mut("node.properties.regular_count")
        .unwrap() = HierarchyFieldValue::U8(1);

    assert!(to_bytes(&decoded).is_err());
}

#[test]
fn every_positioning_generation_decodes_user_automation() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(vec![hierarchy_object(
                version,
                1,
                HierarchyKind::Sound,
                HierarchyBody::Sound(SoundHierarchyObject {
                    source: AudioSourceSetting {
                        plugin: 1,
                        source_type: AudioSourceType::Embedded,
                        resource: 2,
                        source: version.before(113).then_some(3),
                        resource_offset: version.before(113).then_some(0),
                        resource_size: Some(16),
                        flags: 0,
                        plugin_reserved: None,
                    },
                    settings: opaque(parameter_node_with_positioning(
                        version,
                        &automated_positioning(version),
                    )),
                }),
            )])],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode Wwise {number} automated positioning: {error}")
        });
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        let HierarchyBody::Sound(sound) = &objects[0].body else {
            unreachable!()
        };
        assert!(
            sound
                .settings
                .get("node.positioning.automation.transition_time")
                .is_some(),
            "Wwise {number} did not expose positioning automation"
        );
        assert_eq!(to_bytes(&decoded).unwrap(), bytes);
    }
}

#[test]
fn music_switch_tracks_and_versioned_cues_match_twinning() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let mut segment = Vec::new();
        append_minimal_music_common(&mut segment, version);
        zeros(&mut segment, 8); // duration
        segment.extend_from_slice(&1u32.to_le_bytes());
        segment.extend_from_slice(&7u32.to_le_bytes());
        zeros(&mut segment, 8); // cue position
        zeros(&mut segment, if version.before(140) { 4 } else { 1 });

        let mut objects = vec![hierarchy_object(
            version,
            1,
            HierarchyKind::MusicSegment,
            HierarchyBody::MusicSegment(opaque(segment)),
        )];
        if version.at_least(112) {
            objects.push(hierarchy_object(
                version,
                2,
                HierarchyKind::MusicTrack,
                HierarchyBody::MusicTrack(opaque(switch_music_track(version))),
            ));
        }
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(objects)],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode Wwise {number} music special cases: {error}")
        });
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        let HierarchyBody::MusicSegment(segment) = &objects[0].body else {
            unreachable!()
        };
        assert!(segment.get("cues.items[0].id").is_some());
        if version.at_least(112) {
            let HierarchyBody::MusicTrack(track) = &objects[1].body else {
                unreachable!()
            };
            assert!(track.get("switch.transition.destination.offset").is_some());
            assert!(track.get("look_ahead_reserved").is_some());
        }
        assert_eq!(to_bytes(&decoded).unwrap(), bytes);
    }
}

#[test]
fn dialogue_trees_and_rtpc_items_cover_legacy_and_current_widths() {
    for &number in SUPPORTED_VERSIONS {
        let version = BankVersion::new(number).unwrap();
        let mut association = Vec::new();
        association.extend_from_slice(&1u32.to_le_bytes());
        association.extend_from_slice(&10u32.to_le_bytes());
        if version.at_least(88) {
            association.push(0);
        }
        association.extend_from_slice(&12u32.to_le_bytes());
        if version.before(88) {
            association.push(100);
        }
        association.push(0);
        association.extend_from_slice(&1u32.to_le_bytes());
        association.extend_from_slice(&2u32.to_le_bytes());
        association.extend_from_slice(&1u16.to_le_bytes());
        association.extend_from_slice(&100u16.to_le_bytes());
        if version.at_least(120) {
            association.extend_from_slice(&[0, 0]);
        }

        let mut attenuation = minimal_attenuation(version);
        attenuation.truncate(attenuation.len() - 2);
        attenuation.extend_from_slice(&1u16.to_le_bytes());
        attenuation.extend_from_slice(&20u32.to_le_bytes());
        if version.before(112) {
            attenuation.extend_from_slice(&3u32.to_le_bytes());
        } else {
            attenuation.extend_from_slice(&[0, u8::from(version.at_least(128)), 3]);
        }
        attenuation.extend_from_slice(&21u32.to_le_bytes());
        attenuation.push(0);
        attenuation.extend_from_slice(&1u16.to_le_bytes());
        zeros(&mut attenuation, 12);

        let objects = vec![
            hierarchy_object(
                version,
                1,
                HierarchyKind::DialogueEvent,
                HierarchyBody::DialogueEvent(DialogueEvent {
                    probability: version.at_least(88).then_some(100),
                    association: opaque(association),
                }),
            ),
            hierarchy_object(
                version,
                2,
                HierarchyKind::Attenuation,
                HierarchyBody::Attenuation(opaque(attenuation)),
            ),
        ];
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: number,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(objects)],
        };
        let bytes = to_bytes(&bank).unwrap();
        let decoded = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode Wwise {number} tree/RTPC cases: {error}")
        });
        let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
            unreachable!()
        };
        let HierarchyBody::DialogueEvent(dialogue) = &objects[0].body else {
            unreachable!()
        };
        assert!(
            dialogue
                .association
                .get("association.paths[0].object")
                .is_some()
        );
        let HierarchyBody::Attenuation(attenuation) = &objects[1].body else {
            unreachable!()
        };
        assert!(attenuation.get("rtpc.items[0].property_type").is_some());
        assert!(attenuation.get("rtpc.items[0].points[0].curve").is_some());
        assert_eq!(to_bytes(&decoded).unwrap(), bytes);
    }
}

#[test]
fn sound_source_layout_changes_are_version_aware() {
    for version in [
        BankVersion::V72,
        BankVersion::V112,
        BankVersion::V113,
        BankVersion::V140,
    ] {
        for source_type in [
            AudioSourceType::Embedded,
            AudioSourceType::Streamed,
            AudioSourceType::Prefetched,
        ] {
            let bank = SoundBank {
                header: BankHeader {
                    version,
                    id: 1,
                    language: 0,
                    header_expand: Vec::new(),
                },
                chunks: vec![BankChunk::Hierarchy(vec![HierarchyObject {
                    kind: HierarchyKind::Sound,
                    type_code: 2,
                    id: 42,
                    body: HierarchyBody::Sound(SoundHierarchyObject {
                        source: AudioSourceSetting {
                            plugin: 1,
                            source_type,
                            resource: 100,
                            source: version.before(113).then_some(101),
                            resource_offset: (version.before(113)
                                && source_type != AudioSourceType::Streamed)
                                .then_some(12),
                            resource_size: (version.at_least(112)
                                || source_type != AudioSourceType::Streamed)
                                .then_some(256),
                            flags: if version.before(112) { 1 } else { 0b1001 },
                            plugin_reserved: None,
                        },
                        settings: bnk_archive::HierarchyFields::from_opaque_bytes(
                            minimal_parameter_node(version),
                        ),
                    }),
                }])],
            };

            let bytes = to_bytes(&bank).unwrap();
            let decoded = from_bytes(&bytes).unwrap();
            assert_eq!(to_bytes(&decoded).unwrap(), bytes);
        }
    }
}

#[test]
fn legacy_positioning_discriminators_match_twinning_assertions() {
    for (version, positioning) in [
        (BankVersion::V72, vec![1, 1, 0]),
        (BankVersion::V88, vec![1, 0, 0, 0]),
    ] {
        let bank = SoundBank {
            header: BankHeader {
                version,
                id: 1,
                language: 0,
                header_expand: Vec::new(),
            },
            chunks: vec![BankChunk::Hierarchy(vec![hierarchy_object(
                version,
                1,
                HierarchyKind::Sound,
                HierarchyBody::Sound(SoundHierarchyObject {
                    source: AudioSourceSetting {
                        plugin: 1,
                        source_type: AudioSourceType::Embedded,
                        resource: 2,
                        source: Some(3),
                        resource_offset: Some(0),
                        resource_size: Some(16),
                        flags: 0,
                        plugin_reserved: None,
                    },
                    settings: opaque(parameter_node_with_positioning(version, &positioning)),
                }),
            )])],
        };
        assert!(
            to_bytes(&bank).is_err(),
            "Wwise {} accepted a positioning discriminator Twinning rejects",
            version.number()
        );
    }
}

#[test]
fn bus_override_flags_match_twinning_assertions() {
    for version in [BankVersion::V125, BankVersion::V135, BankVersion::V150] {
        for (byte_index, description) in [(1, "positioning"), (2, "auxiliary-send")] {
            let mut settings = minimal_bus(version);
            settings[byte_index] = 0;
            let bank = SoundBank {
                header: BankHeader {
                    version,
                    id: 1,
                    language: 0,
                    header_expand: Vec::new(),
                },
                chunks: vec![BankChunk::Hierarchy(vec![hierarchy_object(
                    version,
                    1,
                    HierarchyKind::AudioBus,
                    HierarchyBody::AudioBus(BusHierarchyObject {
                        parent: 1,
                        audio_device: None,
                        settings: opaque(settings),
                    }),
                )])],
            };
            assert!(
                to_bytes(&bank).is_err(),
                "Wwise {} accepted disabled bus {description} overrides",
                version.number()
            );
        }
    }
}

#[test]
fn music_playlist_items_are_the_flat_list_twinning_models() {
    let version = BankVersion::V140;
    let mut settings = Vec::new();
    append_minimal_music_common(&mut settings, version);
    zeros(&mut settings, 4); // transition count
    settings.extend_from_slice(&2_u32.to_le_bytes());
    for (index, child_count) in [(0_u32, 99_u32), (1, 0)] {
        settings.extend_from_slice(&index.to_le_bytes()); // item
        zeros(&mut settings, 4); // u1
        settings.extend_from_slice(&child_count.to_le_bytes());
        zeros(&mut settings, 4); // play mode/type
        zeros(&mut settings, 2); // loop
        zeros(&mut settings, 4); // reserved
        zeros(&mut settings, 4); // weight
        zeros(&mut settings, 2); // avoid repeat
        settings.push(0); // group
        settings.push(0); // random type
    }
    let bank = SoundBank {
        header: BankHeader {
            version,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::Hierarchy(vec![hierarchy_object(
            version,
            1,
            HierarchyKind::MusicPlaylistContainer,
            HierarchyBody::MusicPlaylistContainer(opaque(settings)),
        )])],
    };

    let bytes = to_bytes(&bank).expect("Twinning does not derive list shape from child_count");
    let decoded = from_bytes(&bytes).unwrap();
    let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
        unreachable!()
    };
    let HierarchyBody::MusicPlaylistContainer(fields) = &objects[0].body else {
        unreachable!()
    };
    assert!(fields.get("playlist.items[1].item").is_some());
    assert_eq!(to_bytes(&decoded).unwrap(), bytes);
}

#[test]
fn hierarchy_integer_signedness_matches_twinning_wire_types() {
    let version = BankVersion::V140;
    let bank = SoundBank {
        header: BankHeader {
            version,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::Hierarchy(vec![
            hierarchy_object(
                version,
                1,
                HierarchyKind::SoundPlaylistContainer,
                HierarchyBody::SoundPlaylistContainer(opaque(minimal_playlist_container(version))),
            ),
            hierarchy_object(
                version,
                2,
                HierarchyKind::Attenuation,
                HierarchyBody::Attenuation(opaque(minimal_attenuation(version))),
            ),
            hierarchy_object(
                version,
                3,
                HierarchyKind::Sound,
                HierarchyBody::Sound(SoundHierarchyObject {
                    source: AudioSourceSetting {
                        plugin: 1,
                        source_type: AudioSourceType::Embedded,
                        resource: 2,
                        source: None,
                        resource_offset: None,
                        resource_size: Some(16),
                        flags: 0,
                        plugin_reserved: None,
                    },
                    settings: opaque(parameter_node_with_positioning(
                        version,
                        &automated_positioning(version),
                    )),
                }),
            ),
        ])],
    };

    let decoded = from_bytes(&to_bytes(&bank).unwrap()).unwrap();
    let BankChunk::Hierarchy(objects) = &decoded.chunks[0] else {
        unreachable!()
    };
    let HierarchyBody::SoundPlaylistContainer(playlist) = &objects[0].body else {
        unreachable!()
    };
    assert!(matches!(
        playlist.get("playback.loop_count"),
        Some(HierarchyFieldValue::I16(0))
    ));
    let HierarchyBody::Attenuation(attenuation) = &objects[1].body else {
        unreachable!()
    };
    assert!(matches!(
        attenuation.get("curve_slots[0]"),
        Some(HierarchyFieldValue::U8(0))
    ));
    let HierarchyBody::Sound(sound) = &objects[2].body else {
        unreachable!()
    };
    assert!(matches!(
        sound
            .settings
            .get("node.positioning.automation.transition_time"),
        Some(HierarchyFieldValue::U32(0))
    ));
}
