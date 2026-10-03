//! Unified Twinning-aligned versioned HIRC layouts.

mod audio;
mod containers;
mod music;

use audio::{
    parse_attenuation, parse_bus, parse_modulator, parse_parameter_node, parse_plugin_settings,
};
use containers::{parse_blend_container, parse_playlist_container, parse_switch_container};
use music::{parse_music_playlist, parse_music_segment, parse_music_switch, parse_music_track};

use crate::error::{BnkError, Result};
use crate::hierarchy::{FieldReader, HierarchyFields, parse_children};
use crate::limits::{DecodeLimits, ValidationMode};
use crate::types::HierarchyKind;
use crate::version::BankVersion;

pub(crate) fn decode_fields(
    kind: HierarchyKind,
    bytes: &[u8],
    offset: usize,
    version: BankVersion,
    validation: ValidationMode,
    limits: DecodeLimits,
) -> Result<HierarchyFields> {
    let mut reader = FieldReader::new(bytes, offset, validation, limits);
    match kind {
        HierarchyKind::Sound => parse_parameter_node(&mut reader, "node", version)?,
        HierarchyKind::AudioBus | HierarchyKind::AuxiliaryAudioBus => {
            parse_bus(&mut reader, version)?
        }
        HierarchyKind::Attenuation => parse_attenuation(&mut reader, version)?,
        HierarchyKind::LowFrequencyOscillatorModulator
        | HierarchyKind::EnvelopeModulator
        | HierarchyKind::TimeModulator => parse_modulator(&mut reader, version)?,
        HierarchyKind::Effect | HierarchyKind::Source => {
            parse_plugin_settings(&mut reader, version, false)?
        }
        HierarchyKind::AudioDevice => parse_plugin_settings(&mut reader, version, true)?,
        HierarchyKind::SoundPlaylistContainer => parse_playlist_container(&mut reader, version)?,
        HierarchyKind::SoundSwitchContainer => parse_switch_container(&mut reader, version)?,
        HierarchyKind::SoundBlendContainer => parse_blend_container(&mut reader, version)?,
        HierarchyKind::ActorMixer => {
            parse_parameter_node(&mut reader, "node", version)?;
            parse_children(&mut reader, "children")?;
        }
        HierarchyKind::MusicTrack => parse_music_track(&mut reader, version)?,
        HierarchyKind::MusicSegment => parse_music_segment(&mut reader, version)?,
        HierarchyKind::MusicPlaylistContainer => parse_music_playlist(&mut reader, version)?,
        HierarchyKind::MusicSwitchContainer => parse_music_switch(&mut reader, version)?,
        _ => return Ok(HierarchyFields::from_opaque_bytes(bytes)),
    }
    reader.finish().map_err(|error| {
        BnkError::invalid(
            "HIRC structured payload",
            offset,
            format!("Wwise {} {kind:?}: {error}", version.number()),
        )
    })
}
