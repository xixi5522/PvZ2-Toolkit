//! Canonical and lossless chunk encoding.

use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};

use super::reader::{
    check_count_fit, ensure_enumeration, validate_media_pairs, validate_twinning_chunks,
};
use crate::error::{BnkError, Result};
use crate::hierarchy::{
    HierarchyFields, decode_dialogue_fields, decode_event_action_fields, decode_fields,
};
use crate::limits::{DecodeLimits, ValidationMode};
use crate::semantics::{EnumerationKind, PackedLayout};
use crate::types::*;
use crate::version::BankVersion;

enum CountWidth {
    U8,
    U16,
    U32,
}

pub(crate) fn validate_sound_bank(bank: &SoundBank, twinning_compatible: bool) -> Result<()> {
    bank.version().ensure_supported()?;
    validate_media_pairs(&bank.chunks)?;
    if twinning_compatible {
        validate_twinning_chunks(bank.version(), &bank.chunks)?;
    }
    encode_header(&bank.header)?;
    for chunk in &bank.chunks {
        encode_chunk(chunk, bank.version())?;
    }
    Ok(())
}

pub fn to_writer(bank: &SoundBank, mut writer: impl Write) -> Result<()> {
    validate_media_pairs(&bank.chunks)?;
    let header = encode_header(&bank.header)?;
    write_chunk_to(&mut writer, ChunkId::BKHD, &header)?;
    for chunk in &bank.chunks {
        match chunk {
            BankChunk::MediaData(data) => write_chunk_to(&mut writer, ChunkId::DATA, data)?,
            BankChunk::Unknown(raw) => write_chunk_to(&mut writer, raw.id, &raw.data)?,
            _ => {
                let data = encode_chunk(chunk, bank.header.version)?;
                write_chunk_to(&mut writer, chunk.id(), &data)?;
            }
        }
    }
    Ok(())
}

pub fn to_bytes(bank: &SoundBank) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    to_writer(bank, &mut output)?;
    Ok(output)
}

fn write_chunk_to(output: &mut impl Write, id: ChunkId, data: &[u8]) -> Result<()> {
    let length = u32::try_from(data.len()).map_err(|_| BnkError::LimitExceeded {
        resource: "chunk bytes",
        requested: data.len() as u64,
        limit: u32::MAX as u64,
    })?;
    output
        .write_all(&id.0)
        .map_err(|source| BnkError::io(0, source))?;
    output
        .write_u32::<LittleEndian>(length)
        .map_err(|source| BnkError::io(4, source))?;
    output
        .write_all(data)
        .map_err(|source| BnkError::io(8, source))?;
    Ok(())
}

fn encode_header(header: &BankHeader) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(12 + header.header_expand.len());
    output
        .write_u32::<LittleEndian>(header.version.number())
        .expect("Vec write");
    output
        .write_u32::<LittleEndian>(header.id)
        .expect("Vec write");
    output
        .write_u32::<LittleEndian>(header.language)
        .expect("Vec write");
    output.extend_from_slice(&header.header_expand);
    Ok(output)
}

fn encode_chunk(chunk: &BankChunk, version: BankVersion) -> Result<Vec<u8>> {
    match chunk {
        BankChunk::MediaIndex(entries) => encode_media_index(entries),
        BankChunk::MediaData(data) => Ok(data.clone()),
        BankChunk::Plugins(plugins) => encode_plugins(plugins, version),
        BankChunk::GameSynchronization(value) => encode_game_synchronization(value, version),
        BankChunk::Hierarchy(objects) => encode_hierarchy(objects, version),
        BankChunk::References(references) => encode_references(references),
        BankChunk::Environments(settings) => encode_environments(settings, version),
        BankChunk::Platform(platform) => encode_platform(platform, version),
        BankChunk::Unknown(chunk) => Ok(chunk.data.clone()),
    }
}

fn encode_media_index(entries: &[MediaIndexEntry]) -> Result<Vec<u8>> {
    let length = entries
        .len()
        .checked_mul(12)
        .ok_or(BnkError::IntegerOverflow {
            context: "DIDX length",
        })?;
    let mut output = Vec::with_capacity(length);
    for entry in entries {
        output
            .write_u32::<LittleEndian>(entry.id)
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(entry.offset)
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(entry.size)
            .expect("Vec write");
    }
    Ok(output)
}

fn encode_plugins(plugins: &[PluginReference], version: BankVersion) -> Result<Vec<u8>> {
    if version.before(118) {
        return Err(BnkError::invalid(
            "INIT",
            0,
            format!("INIT is not defined for version {}", version.number()),
        ));
    }
    let mut output = Vec::new();
    write_count(&mut output, plugins.len(), CountWidth::U32, "INIT plugins")?;
    for plugin in plugins {
        output
            .write_u32::<LittleEndian>(plugin.id)
            .expect("Vec write");
        if version.before(140) {
            write_length_string(
                &mut output,
                &plugin.library,
                CountWidth::U32,
                true,
                "INIT plugin library",
            )?;
        } else {
            write_c_string(&mut output, &plugin.library, "INIT plugin library")?;
        }
    }
    Ok(output)
}

fn encode_game_synchronization(
    value: &GameSynchronization,
    version: BankVersion,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    if version.at_least(145) {
        let behavior = value.voice_filter_behavior.ok_or_else(|| {
            BnkError::invalid(
                "STMG voice filter behavior",
                0,
                "field is required since version 145",
            )
        })?;
        ensure_enumeration(
            EnumerationKind::VoiceFilterBehavior,
            version,
            behavior.raw().into(),
            "STMG voice filter behavior",
            0,
        )?;
        output
            .write_u16::<LittleEndian>(behavior.raw())
            .expect("Vec write");
    } else if value.voice_filter_behavior.is_some() {
        return Err(BnkError::invalid(
            "STMG voice filter behavior",
            0,
            format!(
                "field is not present before Wwise 145, found it for {}",
                version.number()
            ),
        ));
    }
    output
        .write_u32::<LittleEndian>(value.volume_threshold.to_bits())
        .expect("Vec write");
    output
        .write_u16::<LittleEndian>(value.maximum_voice_instances)
        .expect("Vec write");
    if version.at_least(128) {
        let compatibility = value.compatibility_value.ok_or_else(|| {
            BnkError::invalid(
                "STMG compatibility value",
                0,
                "field is required since version 128",
            )
        })?;
        if compatibility != 50 {
            return Err(BnkError::invalid(
                "STMG compatibility value",
                0,
                format!("expected 50, found {compatibility}"),
            ));
        }
        output
            .write_u16::<LittleEndian>(compatibility)
            .expect("Vec write");
    } else if value.compatibility_value.is_some() {
        return Err(BnkError::invalid(
            "STMG compatibility value",
            0,
            format!(
                "field is not present before Wwise 128, found it for {}",
                version.number()
            ),
        ));
    }

    write_count(
        &mut output,
        value.state_groups.len(),
        CountWidth::U32,
        "STMG state groups",
    )?;
    for group in &value.state_groups {
        output
            .write_u32::<LittleEndian>(group.id)
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(group.default_transition_time)
            .expect("Vec write");
        write_count(
            &mut output,
            group.custom_transitions.len(),
            CountWidth::U32,
            "STMG transitions",
        )?;
        for transition in &group.custom_transitions {
            output
                .write_u32::<LittleEndian>(transition.from)
                .expect("Vec write");
            output
                .write_u32::<LittleEndian>(transition.to)
                .expect("Vec write");
            output
                .write_u32::<LittleEndian>(transition.time)
                .expect("Vec write");
        }
    }

    write_count(
        &mut output,
        value.switch_groups.len(),
        CountWidth::U32,
        "STMG switch groups",
    )?;
    for group in &value.switch_groups {
        output
            .write_u32::<LittleEndian>(group.id)
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(group.parameter.id)
            .expect("Vec write");
        if version.at_least(112) {
            let category = group.parameter.category.ok_or_else(|| {
                BnkError::invalid(
                    "STMG switch parameter category",
                    0,
                    "field is required since version 112",
                )
            })?;
            let category_raw = category.raw(version);
            ensure_enumeration(
                EnumerationKind::ParameterCategory,
                version,
                category_raw.into(),
                "STMG switch parameter category",
                0,
            )?;
            output.push(category_raw);
        } else if group.parameter.category.is_some() {
            return Err(BnkError::invalid(
                "STMG switch parameter category",
                0,
                format!(
                    "field is not present before Wwise 112, found it for {}",
                    version.number()
                ),
            ));
        }
        write_count(
            &mut output,
            group.points.len(),
            CountWidth::U32,
            "STMG switch points",
        )?;
        for point in &group.points {
            output
                .write_u32::<LittleEndian>(point.x.to_bits())
                .expect("Vec write");
            output
                .write_u32::<LittleEndian>(point.y)
                .expect("Vec write");
            ensure_enumeration(
                EnumerationKind::FadeCurve,
                version,
                point.curve.0,
                "STMG switch point curve",
                0,
            )?;
            output
                .write_u32::<LittleEndian>(point.curve.0)
                .expect("Vec write");
        }
    }

    write_count(
        &mut output,
        value.game_parameters.len(),
        CountWidth::U32,
        "STMG game parameters",
    )?;
    for parameter in &value.game_parameters {
        output
            .write_u32::<LittleEndian>(parameter.id)
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(parameter.default_value.to_bits())
            .expect("Vec write");
        if version.at_least(112) {
            let interpolation = parameter.interpolation.ok_or_else(|| {
                BnkError::invalid(
                    "STMG game parameter interpolation",
                    0,
                    "field is required since version 112",
                )
            })?;
            let mode = interpolation.mode.raw();
            ensure_enumeration(
                EnumerationKind::GameParameterInterpolation,
                version,
                mode,
                "STMG game parameter interpolation mode",
                0,
            )?;
            output.write_u32::<LittleEndian>(mode).expect("Vec write");
            output
                .write_u32::<LittleEndian>(interpolation.attack.to_bits())
                .expect("Vec write");
            output
                .write_u32::<LittleEndian>(interpolation.release.to_bits())
                .expect("Vec write");
            let binding = interpolation
                .built_in_parameter
                .raw(version)
                .ok_or_else(|| {
                    BnkError::invalid(
                        "STMG game parameter built-in binding",
                        0,
                        format!(
                            "{:?} is not available in version {}",
                            interpolation.built_in_parameter,
                            version.number()
                        ),
                    )
                })?;
            ensure_enumeration(
                EnumerationKind::GameParameterBuiltIn,
                version,
                binding.into(),
                "STMG game parameter built-in binding",
                0,
            )?;
            output.push(binding);
        } else if parameter.interpolation.is_some() {
            return Err(BnkError::invalid(
                "STMG game parameter interpolation",
                0,
                format!(
                    "field is not present before Wwise 112, found it for {}",
                    version.number()
                ),
            ));
        }
    }

    if version.before(140) && !value.u1.is_empty() {
        return Err(BnkError::invalid(
            "STMG u1",
            0,
            format!(
                "u1 records are only present since Wwise 140, found {} for {}",
                value.u1.len(),
                version.number()
            ),
        ));
    }
    if version.at_least(120) && version.before(125) {
        output.write_u32::<LittleEndian>(0).expect("Vec write");
        output.write_u32::<LittleEndian>(0).expect("Vec write");
    } else if version.at_least(125) && version.before(140) {
        output.write_u32::<LittleEndian>(0).expect("Vec write");
    } else if version.at_least(140) {
        write_count(&mut output, value.u1.len(), CountWidth::U32, "STMG u1")?;
        for item in &value.u1 {
            output
                .write_u32::<LittleEndian>(item.identifier)
                .expect("Vec write");
            for field in [item.u1, item.u2, item.u3, item.u4, item.u5, item.u6] {
                output
                    .write_u32::<LittleEndian>(field.to_bits())
                    .expect("Vec write");
            }
        }
    }
    Ok(output)
}

fn encode_environments(value: &EnvironmentSettings, version: BankVersion) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    encode_environment_bundle(&mut output, &value.obstruction, version)?;
    encode_environment_bundle(&mut output, &value.occlusion, version)?;
    Ok(output)
}

fn encode_environment_bundle(
    output: &mut Vec<u8>,
    bundle: &EnvironmentBundle,
    version: BankVersion,
) -> Result<()> {
    encode_environment_curve(output, &bundle.volume, version)?;
    encode_environment_curve(output, &bundle.low_pass_filter, version)?;
    if version.at_least(112) {
        let high_pass = bundle.high_pass_filter.as_ref().ok_or_else(|| {
            BnkError::invalid(
                "ENVS high-pass filter",
                0,
                "field is required since version 112",
            )
        })?;
        encode_environment_curve(output, high_pass, version)?;
    } else if bundle.high_pass_filter.is_some() {
        return Err(BnkError::invalid(
            "ENVS high-pass filter",
            0,
            format!(
                "field is not present before Wwise 112, found it for {}",
                version.number()
            ),
        ));
    }
    Ok(())
}

fn encode_environment_curve(
    output: &mut Vec<u8>,
    value: &EnvironmentCurve,
    version: BankVersion,
) -> Result<()> {
    output.push(u8::from(value.enabled));
    let mode = value.mode.raw();
    ensure_enumeration(
        EnumerationKind::CoordinateMode,
        version,
        mode.into(),
        "ENVS coordinate mode",
        0,
    )?;
    output.push(mode);
    write_count(output, value.points.len(), CountWidth::U16, "ENVS points")?;
    for point in &value.points {
        output
            .write_u32::<LittleEndian>(point.x.to_bits())
            .expect("Vec write");
        output
            .write_u32::<LittleEndian>(point.y.to_bits())
            .expect("Vec write");
        ensure_enumeration(
            EnumerationKind::FadeCurve,
            version,
            point.curve.0,
            "ENVS point curve",
            0,
        )?;
        output
            .write_u32::<LittleEndian>(point.curve.0)
            .expect("Vec write");
    }
    Ok(())
}

fn encode_references(value: &BankReferences) -> Result<Vec<u8>> {
    if value.marker != 1 {
        return Err(BnkError::invalid(
            "STID marker",
            0,
            format!("expected 1, found {}", value.marker),
        ));
    }
    let mut output = Vec::new();
    output
        .write_u32::<LittleEndian>(value.marker)
        .expect("Vec write");
    write_count(
        &mut output,
        value.entries.len(),
        CountWidth::U32,
        "STID references",
    )?;
    for entry in &value.entries {
        output
            .write_u32::<LittleEndian>(entry.id)
            .expect("Vec write");
        write_length_string(
            &mut output,
            &entry.name,
            CountWidth::U8,
            false,
            "STID reference name",
        )?;
    }
    Ok(output)
}

fn encode_platform(value: &PlatformSetting, version: BankVersion) -> Result<Vec<u8>> {
    if version.before(113) {
        return Err(BnkError::invalid(
            "PLAT",
            0,
            format!("PLAT is not defined for version {}", version.number()),
        ));
    }
    let mut output = Vec::new();
    if version.before(118) {
        write_length_string(
            &mut output,
            &value.name,
            CountWidth::U32,
            false,
            "PLAT platform",
        )?;
    } else if version.before(140) {
        write_length_string(
            &mut output,
            &value.name,
            CountWidth::U32,
            true,
            "PLAT platform",
        )?;
    } else {
        write_c_string(&mut output, &value.name, "PLAT platform")?;
    }
    Ok(output)
}

fn encode_hierarchy(objects: &[HierarchyObject], version: BankVersion) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    write_count(&mut output, objects.len(), CountWidth::U32, "HIRC objects")?;
    for object in objects {
        let mapped_kind = HierarchyKind::from_code(version, object.type_code);
        if mapped_kind != object.kind {
            return Err(BnkError::invalid(
                "HIRC object type",
                0,
                format!(
                    "type code {} maps to {:?} in version {}, but object is {:?}",
                    object.type_code,
                    mapped_kind,
                    version.number(),
                    object.kind
                ),
            ));
        }
        match (&object.kind, &object.body) {
            (HierarchyKind::StatefulPropertySetting, HierarchyBody::StatefulPropertySetting(_))
            | (HierarchyKind::EventAction, HierarchyBody::EventAction(_))
            | (HierarchyKind::Event, HierarchyBody::Event(_))
            | (HierarchyKind::DialogueEvent, HierarchyBody::DialogueEvent(_))
            | (HierarchyKind::Sound, HierarchyBody::Sound(_))
            | (HierarchyKind::Effect, HierarchyBody::Effect(_))
            | (HierarchyKind::Source, HierarchyBody::Source(_))
            | (HierarchyKind::AudioDevice, HierarchyBody::AudioDevice(_))
            | (HierarchyKind::AudioBus, HierarchyBody::AudioBus(_))
            | (HierarchyKind::AuxiliaryAudioBus, HierarchyBody::AuxiliaryAudioBus(_))
            | (HierarchyKind::Attenuation, HierarchyBody::Attenuation(_))
            | (
                HierarchyKind::LowFrequencyOscillatorModulator,
                HierarchyBody::LowFrequencyOscillatorModulator(_),
            )
            | (HierarchyKind::EnvelopeModulator, HierarchyBody::EnvelopeModulator(_))
            | (HierarchyKind::TimeModulator, HierarchyBody::TimeModulator(_))
            | (HierarchyKind::SoundPlaylistContainer, HierarchyBody::SoundPlaylistContainer(_))
            | (HierarchyKind::SoundSwitchContainer, HierarchyBody::SoundSwitchContainer(_))
            | (HierarchyKind::SoundBlendContainer, HierarchyBody::SoundBlendContainer(_))
            | (HierarchyKind::ActorMixer, HierarchyBody::ActorMixer(_))
            | (HierarchyKind::MusicTrack, HierarchyBody::MusicTrack(_))
            | (HierarchyKind::MusicSegment, HierarchyBody::MusicSegment(_))
            | (HierarchyKind::MusicPlaylistContainer, HierarchyBody::MusicPlaylistContainer(_))
            | (HierarchyKind::MusicSwitchContainer, HierarchyBody::MusicSwitchContainer(_))
            | (_, HierarchyBody::Raw(_)) => {}
            (kind, body) => {
                return Err(BnkError::invalid(
                    "HIRC object body",
                    0,
                    format!("body {body:?} is incompatible with {kind:?}"),
                ));
            }
        }
        validate_hierarchy_body_structure(object.kind, &object.body, version)?;
        let body = encode_hierarchy_body(&object.body, version)?;
        let length = body.len().checked_add(4).ok_or(BnkError::IntegerOverflow {
            context: "HIRC object length",
        })?;
        let length = u32::try_from(length).map_err(|_| BnkError::LimitExceeded {
            resource: "HIRC object bytes",
            requested: length as u64,
            limit: u32::MAX as u64,
        })?;
        output.push(object.type_code);
        output.write_u32::<LittleEndian>(length).expect("Vec write");
        output
            .write_u32::<LittleEndian>(object.id)
            .expect("Vec write");
        output.extend_from_slice(&body);
    }
    Ok(output)
}

fn validate_hierarchy_fields(
    kind: HierarchyKind,
    fields: &HierarchyFields,
    version: BankVersion,
) -> Result<()> {
    fields.validate_semantics()?;
    let bytes = fields.to_bytes()?;
    decode_fields(
        kind,
        &bytes,
        0,
        version,
        ValidationMode::Strict,
        DecodeLimits::default(),
    )?;
    Ok(())
}

fn validate_hierarchy_body_structure(
    kind: HierarchyKind,
    body: &HierarchyBody,
    version: BankVersion,
) -> Result<()> {
    match body {
        HierarchyBody::EventAction(value) => {
            value.payload.validate_semantics()?;
            let bytes = value.payload.to_bytes()?;
            decode_event_action_fields(
                &bytes,
                0,
                version,
                value.action_type,
                ValidationMode::Strict,
                DecodeLimits::default(),
            )?;
        }
        HierarchyBody::DialogueEvent(value) => {
            value.association.validate_semantics()?;
            let bytes = value.association.to_bytes()?;
            decode_dialogue_fields(
                &bytes,
                0,
                version,
                ValidationMode::Strict,
                DecodeLimits::default(),
            )?;
        }
        HierarchyBody::Sound(value) => {
            validate_hierarchy_fields(HierarchyKind::Sound, &value.settings, version)?;
        }
        HierarchyBody::Effect(value)
        | HierarchyBody::Source(value)
        | HierarchyBody::AudioDevice(value) => {
            validate_hierarchy_fields(kind, &value.settings, version)?;
        }
        HierarchyBody::AudioBus(value) | HierarchyBody::AuxiliaryAudioBus(value) => {
            validate_hierarchy_fields(kind, &value.settings, version)?;
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
            validate_hierarchy_fields(kind, value, version)?;
        }
        HierarchyBody::StatefulPropertySetting(_)
        | HierarchyBody::Event(_)
        | HierarchyBody::Raw(_) => {}
    }
    Ok(())
}

fn encode_hierarchy_body(body: &HierarchyBody, version: BankVersion) -> Result<Vec<u8>> {
    match body {
        HierarchyBody::StatefulPropertySetting(value) => {
            let mut output = Vec::new();
            let width = if version.before(128) {
                CountWidth::U8
            } else {
                CountWidth::U16
            };
            write_count(
                &mut output,
                value.values.len(),
                width,
                "stateful property values",
            )?;
            for item in &value.values {
                if version.before(128) {
                    output.push(u8::try_from(item.property_type).map_err(|_| {
                        BnkError::LimitExceeded {
                            resource: "stateful property type",
                            requested: item.property_type as u64,
                            limit: u8::MAX as u64,
                        }
                    })?);
                } else {
                    output
                        .write_u16::<LittleEndian>(item.property_type)
                        .expect("Vec write");
                }
            }
            for item in &value.values {
                output
                    .write_u32::<LittleEndian>(item.value.to_bits())
                    .expect("Vec write");
            }
            Ok(output)
        }
        HierarchyBody::Event(value) => {
            let mut output = Vec::new();
            let width = if version.before(125) {
                CountWidth::U32
            } else {
                CountWidth::U8
            };
            write_count(&mut output, value.actions.len(), width, "event actions")?;
            for action in &value.actions {
                output
                    .write_u32::<LittleEndian>(*action)
                    .expect("Vec write");
            }
            Ok(output)
        }
        HierarchyBody::EventAction(value) => {
            if EnumerationKind::EventActionType
                .variant(version.number(), value.action_type.into())
                .is_none()
            {
                return Err(BnkError::invalid(
                    "event action type",
                    0,
                    format!(
                        "unknown value {} for Wwise {}",
                        value.action_type,
                        version.number()
                    ),
                ));
            }
            let scope_layout = if version.before(125) {
                PackedLayout::EventActionScopeAndMode72
            } else {
                PackedLayout::EventActionScopeAndMode125
            };
            if !scope_layout.is_canonical(value.scope_and_mode.into()) {
                return Err(BnkError::invalid(
                    "event action scope and mode",
                    0,
                    format!("non-zero reserved bits in 0x{:02x}", value.scope_and_mode),
                ));
            }
            let mode = if version.before(125) {
                (value.scope_and_mode >> 1) & 0x07
            } else {
                (value.scope_and_mode >> 1) & 0x03
            };
            let valid_mode = if version.before(125) {
                matches!(mode, 0 | 1 | 2 | 4)
            } else {
                matches!(mode, 0..=2)
            };
            if !valid_mode {
                return Err(BnkError::invalid(
                    "event action mode",
                    0,
                    format!("unknown packed mode {mode}"),
                ));
            }
            let payload = value.payload.to_bytes()?;
            let mut output = Vec::with_capacity(7 + payload.len());
            output.push(value.scope_and_mode);
            output.push(value.action_type);
            output
                .write_u32::<LittleEndian>(value.target)
                .expect("Vec write");
            output.push(value.u1);
            output.extend_from_slice(&payload);
            Ok(output)
        }
        HierarchyBody::DialogueEvent(value) => {
            let mut output = Vec::with_capacity(
                usize::from(value.probability.is_some()) + value.association.fields.len(),
            );
            if version.at_least(88) {
                output.push(value.probability.ok_or_else(|| {
                    BnkError::invalid(
                        "dialogue event probability",
                        0,
                        "field is required since version 88",
                    )
                })?);
            } else if value.probability.is_some() {
                return Err(BnkError::invalid(
                    "dialogue event probability",
                    0,
                    "the probability is stored inside the association before Wwise 88",
                ));
            }
            output.extend_from_slice(&value.association.to_bytes()?);
            Ok(output)
        }
        HierarchyBody::Sound(value) => {
            let mut output = Vec::new();
            encode_audio_source(&mut output, &value.source, version)?;
            output.extend_from_slice(&value.settings.to_bytes()?);
            Ok(output)
        }
        HierarchyBody::Effect(value)
        | HierarchyBody::Source(value)
        | HierarchyBody::AudioDevice(value) => {
            let mut output = Vec::new();
            output
                .write_u32::<LittleEndian>(value.plugin)
                .expect("Vec write");
            write_count(
                &mut output,
                value.expand.len(),
                CountWidth::U32,
                "HIRC plug-in expand",
            )?;
            output.extend_from_slice(&value.expand);
            output.extend_from_slice(&value.settings.to_bytes()?);
            Ok(output)
        }
        HierarchyBody::AudioBus(value) | HierarchyBody::AuxiliaryAudioBus(value) => {
            let mut output = Vec::new();
            output
                .write_u32::<LittleEndian>(value.parent)
                .expect("Vec write");
            if version.at_least(128) && value.parent == 0 {
                output
                    .write_u32::<LittleEndian>(value.audio_device.ok_or_else(|| {
                        BnkError::invalid(
                            "audio bus audio device",
                            0,
                            "root bus requires an audio-device identifier since version 128",
                        )
                    })?)
                    .expect("Vec write");
            } else if value.audio_device.is_some() {
                return Err(BnkError::invalid(
                    "audio bus audio device",
                    0,
                    format!(
                        "field is only present on root buses since Wwise 128, found it for parent {} in {}",
                        value.parent,
                        version.number()
                    ),
                ));
            }
            output.extend_from_slice(&value.settings.to_bytes()?);
            Ok(output)
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
        | HierarchyBody::MusicSwitchContainer(value) => value.to_bytes(),
        HierarchyBody::Raw(bytes) => Ok(bytes.clone()),
    }
}

fn encode_audio_source(
    output: &mut Vec<u8>,
    value: &AudioSourceSetting,
    version: BankVersion,
) -> Result<()> {
    let flags_layout = if version.before(112) {
        PackedLayout::Boolean
    } else {
        PackedLayout::Source
    };
    if !flags_layout.is_canonical(value.flags.into()) {
        return Err(BnkError::invalid(
            "audio source flags",
            0,
            format!("non-zero reserved bits in 0x{:02x}", value.flags),
        ));
    }
    if value.plugin_reserved.is_some_and(|reserved| reserved != 0) {
        return Err(BnkError::invalid(
            "audio source plug-in reserved value",
            0,
            "expected zero",
        ));
    }
    output
        .write_u32::<LittleEndian>(value.plugin)
        .expect("Vec write");
    let source_type = value.source_type.raw(version);
    ensure_enumeration(
        EnumerationKind::AudioSourceType,
        version,
        source_type,
        "audio source type",
        0,
    )?;
    if version.before(112) {
        output
            .write_u32::<LittleEndian>(source_type)
            .expect("Vec write");
    } else {
        output.push(
            u8::try_from(source_type).map_err(|_| BnkError::LimitExceeded {
                resource: "audio source type",
                requested: source_type as u64,
                limit: u8::MAX as u64,
            })?,
        );
    }
    output
        .write_u32::<LittleEndian>(value.resource)
        .expect("Vec write");
    if version.before(113) {
        output
            .write_u32::<LittleEndian>(value.source.ok_or_else(|| {
                BnkError::invalid(
                    "audio source identifier",
                    0,
                    "field is required through version 112",
                )
            })?)
            .expect("Vec write");
        if value.source_type != AudioSourceType::Streamed {
            output
                .write_u32::<LittleEndian>(value.resource_offset.ok_or_else(|| {
                    BnkError::invalid(
                        "audio source resource offset",
                        0,
                        "field is required for non-streamed sources through version 112",
                    )
                })?)
                .expect("Vec write");
        } else if value.resource_offset.is_some() {
            return Err(BnkError::invalid(
                "audio source resource offset",
                0,
                "streamed sources do not store an offset through version 112",
            ));
        }
    } else if value.source.is_some() || value.resource_offset.is_some() {
        return Err(BnkError::invalid(
            "audio source legacy fields",
            0,
            format!(
                "source identifier and resource offset are not present since Wwise 113, found one for {}",
                version.number()
            ),
        ));
    }
    if version.at_least(112) || value.source_type != AudioSourceType::Streamed {
        output
            .write_u32::<LittleEndian>(value.resource_size.ok_or_else(|| {
                BnkError::invalid(
                    "audio source resource size",
                    0,
                    "field is required for this source type and bank version",
                )
            })?)
            .expect("Vec write");
    } else if value.resource_size.is_some() {
        return Err(BnkError::invalid(
            "audio source resource size",
            0,
            "legacy streamed sources do not store a resource size",
        ));
    }
    output.push(value.flags);
    if (value.plugin & 0xffff) >= 2 {
        output
            .write_u32::<LittleEndian>(value.plugin_reserved.ok_or_else(|| {
                BnkError::invalid(
                    "audio source plug-in reserved value",
                    0,
                    "field is required when the low plug-in identifier is at least two",
                )
            })?)
            .expect("Vec write");
    } else if value.plugin_reserved.is_some() {
        return Err(BnkError::invalid(
            "audio source plug-in reserved value",
            0,
            "field is not present when the low plug-in identifier is below two",
        ));
    }
    Ok(())
}

fn write_count(
    output: &mut Vec<u8>,
    count: usize,
    width: CountWidth,
    context: &'static str,
) -> Result<()> {
    match width {
        CountWidth::U8 => {
            check_count_fit(count, u8::MAX as usize, context)?;
            output.push(count as u8);
        }
        CountWidth::U16 => {
            check_count_fit(count, u16::MAX as usize, context)?;
            output
                .write_u16::<LittleEndian>(count as u16)
                .expect("Vec write");
        }
        CountWidth::U32 => {
            check_count_fit(count, u32::MAX as usize, context)?;
            output
                .write_u32::<LittleEndian>(count as u32)
                .expect("Vec write");
        }
    }
    Ok(())
}

fn write_length_string(
    output: &mut Vec<u8>,
    value: &str,
    width: CountWidth,
    nul_terminated: bool,
    context: &'static str,
) -> Result<()> {
    if value.as_bytes().contains(&0) {
        return Err(BnkError::invalid(
            context,
            0,
            "embedded NUL byte is not representable",
        ));
    }
    let length = value
        .len()
        .checked_add(usize::from(nul_terminated))
        .ok_or(BnkError::IntegerOverflow { context })?;
    write_count(output, length, width, context)?;
    output.extend_from_slice(value.as_bytes());
    if nul_terminated {
        output.push(0);
    }
    Ok(())
}

fn write_c_string(output: &mut Vec<u8>, value: &str, context: &'static str) -> Result<()> {
    if value.as_bytes().contains(&0) {
        return Err(BnkError::invalid(
            context,
            0,
            "embedded NUL byte is not representable",
        ));
    }
    output.extend_from_slice(value.as_bytes());
    output.push(0);
    Ok(())
}
