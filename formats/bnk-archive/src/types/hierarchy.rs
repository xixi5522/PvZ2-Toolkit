//! HIRC object types and version-aware identifiers.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::Identifier;
use crate::error::{BnkError, Result};
use crate::hierarchy::HierarchyFields;
use crate::version::BankVersion;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyObject {
    pub kind: HierarchyKind,
    /// Original numeric type, retained even when `kind` is known.
    pub type_code: u8,
    pub id: Identifier,
    pub body: HierarchyBody,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum HierarchyBody {
    StatefulPropertySetting(StatefulPropertySetting),
    EventAction(EventAction),
    Event(Event),
    DialogueEvent(DialogueEvent),
    Attenuation(HierarchyFields),
    LowFrequencyOscillatorModulator(HierarchyFields),
    EnvelopeModulator(HierarchyFields),
    TimeModulator(HierarchyFields),
    Sound(SoundHierarchyObject),
    Effect(PluginHierarchyObject),
    Source(PluginHierarchyObject),
    AudioDevice(PluginHierarchyObject),
    AudioBus(BusHierarchyObject),
    AuxiliaryAudioBus(BusHierarchyObject),
    SoundPlaylistContainer(HierarchyFields),
    SoundSwitchContainer(HierarchyFields),
    SoundBlendContainer(HierarchyFields),
    ActorMixer(HierarchyFields),
    MusicTrack(HierarchyFields),
    MusicSegment(HierarchyFields),
    MusicPlaylistContainer(HierarchyFields),
    MusicSwitchContainer(HierarchyFields),
    /// Lossless payload for hierarchy types whose internal schema is not
    /// selected by this decoder, and for unknown future type codes.
    Raw(#[cfg_attr(feature = "serde", serde(with = "serde_bytes"))] Vec<u8>),
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct StatefulPropertySetting {
    pub values: Vec<StatefulPropertyValue>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatefulPropertyValue {
    pub property_type: u16,
    pub value: f32,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub actions: Vec<Identifier>,
}

/// Common header shared by every event-action subtype.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct EventAction {
    /// Packed Wwise scope and action-mode bits.
    pub scope_and_mode: u8,
    pub action_type: u8,
    pub target: Identifier,
    /// Twinning's still-unidentified `u1` byte.
    pub u1: u8,
    /// Version-aware common properties and action-specific fields.
    pub payload: HierarchyFields,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EventActionScope {
    Global,
    GameObject,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EventActionMode {
    None,
    One,
    All,
    AllExcept,
}

impl EventAction {
    pub fn scope(&self) -> EventActionScope {
        if self.scope_and_mode & 1 == 0 {
            EventActionScope::Global
        } else {
            EventActionScope::GameObject
        }
    }

    pub fn set_scope(&mut self, scope: EventActionScope) {
        self.scope_and_mode =
            (self.scope_and_mode & !1) | u8::from(scope == EventActionScope::GameObject);
    }

    pub fn mode(&self, version: BankVersion) -> EventActionMode {
        let mask = if version.before(125) { 0x07 } else { 0x03 };
        match (self.scope_and_mode >> 1) & mask {
            0 => EventActionMode::None,
            1 => EventActionMode::One,
            2 => EventActionMode::All,
            4 => EventActionMode::AllExcept,
            _ => EventActionMode::None,
        }
    }

    /// Set the packed mode. `AllExcept` is not canonical since Wwise 125.
    pub fn set_mode(&mut self, mode: EventActionMode, version: BankVersion) -> Result<()> {
        if version.at_least(125) && mode == EventActionMode::AllExcept {
            return Err(BnkError::invalid(
                "event action mode",
                0,
                "all_except is only valid before Wwise 125",
            ));
        }
        let raw = match mode {
            EventActionMode::None => 0,
            EventActionMode::One => 1,
            EventActionMode::All => 2,
            EventActionMode::AllExcept => 4,
        };
        let mask = if version.before(125) { 0x0e } else { 0x06 };
        self.scope_and_mode = (self.scope_and_mode & !mask) | (raw << 1);
        Ok(())
    }

    pub fn action_type_name(&self, version: BankVersion) -> Option<&'static str> {
        crate::semantics::EnumerationKind::EventActionType
            .variant(version.number(), self.action_type.into())
            .map(|variant| variant.name)
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct DialogueEvent {
    pub probability: Option<u8>,
    pub association: HierarchyFields,
}

/// Media source prefix of a sound object followed by its common property data.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct SoundHierarchyObject {
    pub source: AudioSourceSetting,
    /// Effect, metadata, mixer, bus, parent, property, RTPC, state, and
    /// positioning records following the source descriptor.
    pub settings: HierarchyFields,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSourceSetting {
    pub plugin: Identifier,
    pub source_type: AudioSourceType,
    pub resource: Identifier,
    /// Present through Wwise 112.
    pub source: Option<Identifier>,
    /// Present through Wwise 112 for non-streamed sources.
    pub resource_offset: Option<u32>,
    /// Present for every source since Wwise 112, and for non-streamed sources
    /// in older banks.
    pub resource_size: Option<u32>,
    /// Packed `is_voice` and `non_cacheable` flags.
    pub flags: u8,
    /// Reserved zero stored for plug-ins whose low 16-bit identifier is at
    /// least two. It is retained rather than silently discarded.
    pub plugin_reserved: Option<u32>,
}

impl AudioSourceSetting {
    pub fn is_voice(&self) -> bool {
        self.flags & 1 != 0
    }

    pub fn set_is_voice(&mut self, value: bool) {
        self.flags = (self.flags & !1) | u8::from(value);
    }

    pub fn non_cachable_stream(&self) -> bool {
        self.flags & 0x08 != 0
    }

    pub fn set_non_cachable_stream(&mut self, value: bool) {
        self.flags = (self.flags & !0x08) | (u8::from(value) << 3);
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AudioSourceType {
    Embedded,
    Streamed,
    Prefetched,
    Unknown(u32),
}

impl AudioSourceType {
    pub fn from_raw(version: BankVersion, value: u32) -> Self {
        match (version.before(112), value) {
            (_, 0) => Self::Embedded,
            (true, 1) | (false, 2) => Self::Streamed,
            (true, 2) | (false, 1) => Self::Prefetched,
            (_, value) => Self::Unknown(value),
        }
    }

    pub fn raw(self, version: BankVersion) -> u32 {
        match (version.before(112), self) {
            (_, Self::Embedded) => 0,
            (true, Self::Streamed) | (false, Self::Prefetched) => 1,
            (true, Self::Prefetched) | (false, Self::Streamed) => 2,
            (_, Self::Unknown(value)) => value,
        }
    }
}

/// Prefix common to Effect, Source, and AudioDevice hierarchy objects.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PluginHierarchyObject {
    pub plugin: Identifier,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    pub expand: Vec<u8>,
    /// RTPC, state, effect-slot, and version-specific records following the
    /// common plug-in prefix.
    pub settings: HierarchyFields,
}

/// Prefix common to regular and auxiliary audio buses.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct BusHierarchyObject {
    pub parent: Identifier,
    /// Root buses have an explicit audio-device identifier since Wwise 128.
    pub audio_device: Option<Identifier>,
    pub settings: HierarchyFields,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HierarchyKind {
    Unknown,
    StatefulPropertySetting,
    EventAction,
    Event,
    DialogueEvent,
    Attenuation,
    LowFrequencyOscillatorModulator,
    EnvelopeModulator,
    TimeModulator,
    Effect,
    Source,
    AudioDevice,
    AudioBus,
    AuxiliaryAudioBus,
    Sound,
    SoundPlaylistContainer,
    SoundSwitchContainer,
    SoundBlendContainer,
    ActorMixer,
    MusicTrack,
    MusicSegment,
    MusicPlaylistContainer,
    MusicSwitchContainer,
}

impl HierarchyKind {
    pub fn from_code(version: BankVersion, code: u8) -> Self {
        match code {
            1 => Self::StatefulPropertySetting,
            3 => Self::EventAction,
            4 => Self::Event,
            15 => Self::DialogueEvent,
            14 => Self::Attenuation,
            8 => Self::AudioBus,
            2 => Self::Sound,
            5 => Self::SoundPlaylistContainer,
            6 => Self::SoundSwitchContainer,
            9 => Self::SoundBlendContainer,
            7 => Self::ActorMixer,
            11 => Self::MusicTrack,
            10 => Self::MusicSegment,
            13 => Self::MusicPlaylistContainer,
            12 => Self::MusicSwitchContainer,
            18 if version.before(128) => Self::Effect,
            19 if version.before(128) => Self::Source,
            20 if version.before(128) => Self::AuxiliaryAudioBus,
            21 if version.at_least(112) && version.before(128) => {
                Self::LowFrequencyOscillatorModulator
            }
            22 if version.at_least(112) && version.before(128) => Self::EnvelopeModulator,
            16 if version.at_least(128) => Self::Effect,
            17 if version.at_least(128) => Self::Source,
            18 if version.at_least(128) => Self::AuxiliaryAudioBus,
            19 if version.at_least(128) => Self::LowFrequencyOscillatorModulator,
            20 if version.at_least(128) => Self::EnvelopeModulator,
            21 if version.at_least(128) => Self::AudioDevice,
            22 if version.at_least(132) => Self::TimeModulator,
            _ => Self::Unknown,
        }
    }

    pub fn code(self, version: BankVersion) -> Option<u8> {
        if self == Self::Unknown {
            None
        } else {
            (0..=u8::MAX).find(|&code| Self::from_code(version, code) == self)
        }
    }
}
