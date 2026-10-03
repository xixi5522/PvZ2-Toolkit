//! Packed-bit layouts and named members.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::EnumerationKind;

/// Named packed layouts that Twinning expands into individual values.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PackedLayout {
    Boolean,
    BooleanU8IgnoredHigh,
    BooleanU8UniformHigh,
    PositioningMode72,
    PositioningMode88,
    BusPositioning112,
    PriorityAndMidi,
    Positioning112,
    Positioning125,
    Positioning132,
    Positioning140,
    ListenerRouting112,
    ListenerRouting125,
    ListenerRouting128,
    ListenerRouting132,
    ListenerRouting134,
    ListenerRouting140,
    PositionAutomation,
    AuxiliarySends112,
    AuxiliarySends135,
    PlaybackObject,
    PlaybackBus,
    Hdr,
    BusHdr,
    Source,
    EffectSlot150,
    EffectBypass,
    Playlist,
    SwitchPlayback,
    MusicMidi,
    EventActionScopeAndMode72,
    EventActionScopeAndMode125,
    EventActionStop,
    EventActionPauseResumeLegacy,
    EventActionPauseResume,
    EventActionEffectBypassSet,
    EventActionEffectBypassReset,
    MusicPlaylistPlayType,
}

/// One member within a packed integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedMember {
    pub name: &'static str,
    pub bit_offset: u8,
    pub bit_width: u8,
}

impl PackedMember {
    pub const fn mask(self) -> u64 {
        ((1_u64 << self.bit_width) - 1) << self.bit_offset
    }

    pub const fn get(self, raw: u64) -> u64 {
        (raw & self.mask()) >> self.bit_offset
    }

    pub const fn set(self, raw: u64, value: u64) -> u64 {
        (raw & !self.mask()) | ((value << self.bit_offset) & self.mask())
    }
}

impl PackedLayout {
    /// Ordered low-to-high bit members. Names follow Twinning's model.
    pub const fn members(self) -> &'static [PackedMember] {
        match self {
            Self::Boolean => BOOLEAN,
            Self::BooleanU8IgnoredHigh => BOOLEAN_U8_IGNORED_HIGH,
            Self::BooleanU8UniformHigh => BOOLEAN_U8_UNIFORM_HIGH,
            Self::PositioningMode72 => POSITIONING_MODE_72,
            Self::PositioningMode88 => POSITIONING_MODE_88,
            Self::BusPositioning112 => BUS_POSITIONING_112,
            Self::PriorityAndMidi => PRIORITY_AND_MIDI,
            Self::Positioning112 => POSITIONING_112,
            Self::Positioning125 => POSITIONING_125,
            Self::Positioning132 => POSITIONING_132,
            Self::Positioning140 => POSITIONING_140,
            Self::ListenerRouting112 => LISTENER_ROUTING_112,
            Self::ListenerRouting125 => LISTENER_ROUTING_125,
            Self::ListenerRouting128 => LISTENER_ROUTING_128,
            Self::ListenerRouting132 => LISTENER_ROUTING_132,
            Self::ListenerRouting134 => LISTENER_ROUTING_134,
            Self::ListenerRouting140 => LISTENER_ROUTING_140,
            Self::PositionAutomation => POSITION_AUTOMATION,
            Self::AuxiliarySends112 => AUXILIARY_SENDS_112,
            Self::AuxiliarySends135 => AUXILIARY_SENDS_135,
            Self::PlaybackObject => PLAYBACK_OBJECT,
            Self::PlaybackBus => PLAYBACK_BUS,
            Self::Hdr => HDR,
            Self::BusHdr => BUS_HDR,
            Self::Source => SOURCE,
            Self::EffectSlot150 => EFFECT_SLOT_150,
            Self::EffectBypass => EFFECT_BYPASS,
            Self::Playlist => PLAYLIST,
            Self::SwitchPlayback => SWITCH_PLAYBACK,
            Self::MusicMidi => MUSIC_MIDI,
            Self::EventActionScopeAndMode72 => EVENT_ACTION_SCOPE_MODE_72,
            Self::EventActionScopeAndMode125 => EVENT_ACTION_SCOPE_MODE_125,
            Self::EventActionStop => EVENT_ACTION_STOP,
            Self::EventActionPauseResumeLegacy => EVENT_ACTION_PAUSE_RESUME_LEGACY,
            Self::EventActionPauseResume => EVENT_ACTION_PAUSE_RESUME,
            Self::EventActionEffectBypassSet => EVENT_ACTION_EFFECT_BYPASS_SET,
            Self::EventActionEffectBypassReset => EVENT_ACTION_EFFECT_BYPASS_RESET,
            Self::MusicPlaylistPlayType => MUSIC_PLAYLIST_PLAY_TYPE,
        }
    }

    pub fn member(self, name: &str) -> Option<PackedMember> {
        self.members()
            .iter()
            .copied()
            .find(|member| member.name == name)
    }

    /// Twinning enumeration carried by a named packed member, when one is
    /// defined. The version is needed for layouts whose value set changes.
    pub fn member_enumeration(self, name: &str, version: u32) -> Option<EnumerationKind> {
        match (self, name) {
            (Self::Positioning112 | Self::Positioning125, "is_3d") => {
                Some(EnumerationKind::PositioningType)
            }
            (Self::Positioning132 | Self::Positioning140, "position_source_mode")
            | (
                Self::ListenerRouting112 | Self::ListenerRouting125 | Self::ListenerRouting128,
                "position_source_mode",
            ) => Some(EnumerationKind::PositionSourceMode),
            (Self::Positioning132 | Self::Positioning140, "speaker_panning_mode") => {
                Some(EnumerationKind::SpeakerPanningMode)
            }
            (
                Self::ListenerRouting128
                | Self::ListenerRouting132
                | Self::ListenerRouting134
                | Self::ListenerRouting140,
                "spatialization",
            ) => Some(EnumerationKind::PositioningSpatialization),
            (Self::PositionAutomation, "play_type") => Some(EnumerationKind::AudioPlayType),
            (Self::PositionAutomation, "play_mode") => Some(EnumerationKind::AudioPlayMode),
            (Self::Playlist, "at_end_of_playlist") => Some(EnumerationKind::AudioPlaySequenceEnd),
            (Self::Playlist, "play_mode") => Some(EnumerationKind::AudioPlayMode),
            (Self::Playlist, "scope") => Some(EnumerationKind::SoundPlaylistScope),
            (Self::PlaybackObject, "when_priority_is_equal")
            | (Self::PlaybackBus, "when_priority_is_equal") => {
                Some(EnumerationKind::PlaybackLimitPriority)
            }
            (Self::PlaybackObject, "when_limit_is_reached")
            | (Self::PlaybackBus, "when_limit_is_reached") => {
                Some(EnumerationKind::PlaybackLimitReached)
            }
            (Self::PlaybackObject, "scope") => Some(EnumerationKind::PlaybackLimitScope),
            (Self::BusHdr, "release_mode") => Some(EnumerationKind::BusHdrReleaseMode),
            (Self::MusicPlaylistPlayType, "play_mode") => Some(EnumerationKind::AudioPlayMode),
            (Self::MusicPlaylistPlayType, "play_type") => Some(EnumerationKind::AudioPlayType),
            (Self::EventActionScopeAndMode72 | Self::EventActionScopeAndMode125, "scope") => {
                Some(EnumerationKind::EventActionScope)
            }
            (Self::EventActionScopeAndMode72 | Self::EventActionScopeAndMode125, "mode") => {
                Some(EnumerationKind::EventActionMode)
            }
            _ => {
                let _ = version;
                None
            }
        }
    }

    pub fn used_mask(self) -> u64 {
        self.members()
            .iter()
            .fold(0, |mask, member| mask | member.mask())
    }

    pub fn reserved_mask(self) -> u64 {
        self.members()
            .iter()
            .filter(|member| member.name.starts_with("reserved"))
            .fold(0, |mask, member| mask | member.mask())
    }

    /// Whether reserved members and all unused high bits are zero.
    pub fn is_canonical(self, raw: u64) -> bool {
        if self == Self::BooleanU8UniformHigh {
            return raw <= u8::MAX.into() && matches!(raw & 0xfe, 0 | 0xfe);
        }
        if self == Self::EventActionEffectBypassReset {
            return raw <= u8::MAX.into() && raw & 0xe0 == 0xe0;
        }
        raw & (!self.used_mask() | self.reserved_mask()) == 0
    }
}

const fn bit(name: &'static str, bit_offset: u8) -> PackedMember {
    bits(name, bit_offset, 1)
}

const fn bits(name: &'static str, bit_offset: u8, bit_width: u8) -> PackedMember {
    PackedMember {
        name,
        bit_offset,
        bit_width,
    }
}

const BOOLEAN: &[PackedMember] = &[bit("value", 0)];
const BOOLEAN_U8_IGNORED_HIGH: &[PackedMember] =
    &[bit("value", 0), bits("ignored_high_bits", 1, 7)];
const BOOLEAN_U8_UNIFORM_HIGH: &[PackedMember] =
    &[bit("value", 0), bits("uniform_high_bits", 1, 7)];
const POSITIONING_MODE_72: &[PackedMember] = &[bit("u1", 0), bit("u2", 1)];
const POSITIONING_MODE_88: &[PackedMember] = &[bit("u1", 0)];
const BUS_POSITIONING_112: &[PackedMember] =
    &[bit("override_parent", 0), bit("speaker_panning_enabled", 1)];
const PRIORITY_AND_MIDI: &[PackedMember] = &[
    bit("override_priority", 0),
    bit("use_distance_factor", 1),
    bit("override_midi_event", 2),
    bit("override_midi_note_tracking", 3),
    bit("midi_note_tracking_enabled", 4),
    bit("midi_break_on_note_off", 5),
];
const POSITIONING_112: &[PackedMember] = &[
    bit("override_parent", 0),
    bit("unknown", 1),
    bit("speaker_panning_enabled", 2),
    bit("is_3d", 3),
    bit("spatialization", 4),
    bit("automation_loop", 5),
    bit("update_at_each_frame", 6),
    bit("hold_listener_orientation", 7),
];
const POSITIONING_125: &[PackedMember] = &[
    bit("override_parent", 0),
    bit("enabled", 1),
    bit("unknown", 2),
    bit("speaker_panning_enabled", 3),
    bit("is_3d", 4),
];
const POSITIONING_132: &[PackedMember] = &[
    bit("override_parent", 0),
    bit("listener_routing_enabled", 1),
    bit("speaker_panning_mode", 2),
    bit("reserved_0", 3),
    bits("position_source_mode", 4, 2),
    bit("reserved_1", 6),
];
const POSITIONING_140: &[PackedMember] = &[
    bit("override_parent", 0),
    bit("listener_routing_enabled", 1),
    bits("speaker_panning_mode", 2, 2),
    bit("reserved_0", 4),
    bits("position_source_mode", 5, 2),
    bit("reserved_1", 7),
];
const LISTENER_ROUTING_112: &[PackedMember] = &[bits("position_source_mode", 0, 2)];
const LISTENER_ROUTING_125: &[PackedMember] = &[
    bit("spatialization", 0),
    bit("automation_loop", 1),
    bit("update_at_each_frame", 2),
    bit("hold_listener_orientation", 3),
    bits("position_source_mode", 4, 2),
];
const LISTENER_ROUTING_128: &[PackedMember] = &[
    bits("spatialization", 0, 2),
    bit("automation_loop", 2),
    bit("update_at_each_frame", 3),
    bit("hold_listener_orientation", 4),
    bits("position_source_mode", 5, 2),
];
const LISTENER_ROUTING_132: &[PackedMember] = &[
    bits("spatialization", 0, 2),
    bit("hold_emitter_position_and_orientation", 2),
    bit("hold_listener_orientation", 3),
    bit("automation_loop", 4),
];
const LISTENER_ROUTING_134: &[PackedMember] = &[
    bits("spatialization", 0, 2),
    bit("attenuation_enabled", 2),
    bit("hold_emitter_position_and_orientation", 3),
    bit("hold_listener_orientation", 4),
    bit("automation_loop", 5),
];
const LISTENER_ROUTING_140: &[PackedMember] = &[
    bits("spatialization", 0, 2),
    bit("attenuation_enabled", 2),
    bit("hold_emitter_position_and_orientation", 3),
    bit("hold_listener_orientation", 4),
    bit("automation_loop", 5),
    bit("diffraction_and_transmission", 6),
];
const POSITION_AUTOMATION: &[PackedMember] = &[
    bit("play_type", 0),
    bit("play_mode", 1),
    bit("pick_new_path_when_sound_starts", 2),
];
const AUXILIARY_SENDS_112: &[PackedMember] = &[
    bit("override_game_defined", 0),
    bit("game_defined_enabled", 1),
    bit("override_user_defined", 2),
    bit("user_defined_enabled", 3),
];
const AUXILIARY_SENDS_135: &[PackedMember] = &[
    bit("override_game_defined", 0),
    bit("game_defined_enabled", 1),
    bit("override_user_defined", 2),
    bit("user_defined_enabled", 3),
    bit("override_early_reflection", 4),
];
const PLAYBACK_OBJECT: &[PackedMember] = &[
    bit("when_priority_is_equal", 0),
    bit("when_limit_is_reached", 1),
    bit("scope", 2),
    bit("override_limit", 3),
    bit("override_virtual_voice", 4),
];
const PLAYBACK_BUS: &[PackedMember] = &[
    bit("when_priority_is_equal", 0),
    bit("when_limit_is_reached", 1),
    bit("override_limit", 2),
    bit("mute_for_background_music", 3),
];
const HDR: &[PackedMember] = &[
    bit("override_envelope_tracking", 0),
    bit("override_loudness_normalization", 1),
    bit("loudness_normalization_enabled", 2),
    bit("envelope_tracking_enabled", 3),
];
const BUS_HDR: &[PackedMember] = &[bit("enabled", 0), bit("release_mode", 1)];
const SOURCE: &[PackedMember] = &[
    bit("is_voice", 0),
    bit("reserved_0", 1),
    bit("reserved_1", 2),
    bit("non_cachable_stream", 3),
];
const EFFECT_SLOT_150: &[PackedMember] = &[bit("bypass", 0), bit("use_share_set", 1)];
const EFFECT_BYPASS: &[PackedMember] = &[
    bit("slot_1", 0),
    bit("slot_2", 1),
    bit("slot_3", 2),
    bit("slot_4", 3),
    bit("slot_5", 4),
];
const PLAYLIST: &[PackedMember] = &[
    bit("reserved", 0),
    bit("always_reset_playlist", 1),
    bit("at_end_of_playlist", 2),
    bit("play_mode", 3),
    bit("scope", 4),
];
const SWITCH_PLAYBACK: &[PackedMember] =
    &[bit("play_first_only", 0), bit("continue_across_switch", 1)];
const MUSIC_MIDI: &[PackedMember] = &[
    bit("reserved", 0),
    bit("override_clip_tempo", 1),
    bit("override_target", 2),
];
const EVENT_ACTION_SCOPE_MODE_72: &[PackedMember] = &[bit("scope", 0), bits("mode", 1, 3)];
const EVENT_ACTION_SCOPE_MODE_125: &[PackedMember] = &[bit("scope", 0), bits("mode", 1, 2)];
const EVENT_ACTION_STOP: &[PackedMember] = &[
    bit("reserved", 0),
    bit("resume_state_transition", 1),
    bit("apply_to_dynamic_sequence", 2),
];
const EVENT_ACTION_PAUSE_RESUME_LEGACY: &[PackedMember] =
    &[bit("include_delayed_or_master_resume", 0)];
const EVENT_ACTION_PAUSE_RESUME: &[PackedMember] = &[
    bit("include_delayed_or_master_resume", 0),
    bit("resume_state_transition", 1),
    bit("apply_to_dynamic_sequence", 2),
];
const EVENT_ACTION_EFFECT_BYPASS_SET: &[PackedMember] = &[
    bit("slot_1", 0),
    bit("slot_2", 1),
    bit("slot_3", 2),
    bit("slot_4", 3),
    bit("slot_5", 4),
    bit("reserved_0", 5),
    bit("reserved_1", 6),
    bit("reserved_2", 7),
];
const EVENT_ACTION_EFFECT_BYPASS_RESET: &[PackedMember] = &[
    bit("slot_1", 0),
    bit("slot_2", 1),
    bit("slot_3", 2),
    bit("slot_4", 3),
    bit("slot_5", 4),
    bit("reset_constant_0", 5),
    bit("reset_constant_1", 6),
    bit("reset_constant_2", 7),
];
const MUSIC_PLAYLIST_PLAY_TYPE: &[PackedMember] = &[
    bit("play_mode", 0),
    bit("play_type", 1),
    // Twinning intentionally ignores the remaining bits (`k_true`).
    bits("ignored_high_bits", 2, 30),
];
