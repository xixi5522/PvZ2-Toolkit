//! Versioned enumeration catalogs.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Named enumeration catalogs exposed by Twinning.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EnumerationKind {
    FadeCurve,
    TimePoint,
    ParameterCategory,
    PropertyCategory,
    CoordinateMode,
    StateChangeOccursAt,
    AssociationMode,
    PlaylistTransitionMode,
    PlaylistRandomMode,
    PlaylistContainerMode,
    TrackType,
    VirtualVoiceBehavior,
    VirtualVoiceOnReturn,
    ValueApplyMode,
    SeekType,
    MusicJumpMode,
    MusicSynchronizeMode,
    EventActionType,
    EventActionMode,
    EventActionScope,
    AudioSourceType,
    GameParameterBuiltIn,
    GameParameterInterpolation,
    VoiceFilterBehavior,
    PlaybackLimitScope,
    PlaybackLimitPriority,
    PlaybackLimitReached,
    BusHdrReleaseMode,
    BusDuckingTarget,
    AudioPlayType,
    AudioPlayMode,
    AudioPlayRandomType,
    AudioPlaySequenceEnd,
    AudioPlayTransitionType,
    SoundPlaylistScope,
    PositioningType,
    PositionSourceMode,
    PositioningSpatialization,
    SpeakerPanningMode,
    SoundMidiEventPlayOn,
    MusicMidiClipTempoSource,
    ModulatorScope,
    ModulatorTriggerOn,
    ModulatorWaveform,
    MusicTrackClipCurveType,
}

/// One zero-based Wwise enumeration value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumerationVariant {
    pub value: u32,
    pub name: &'static str,
}

impl EnumerationKind {
    /// Twinning names for the enumeration. Sparse wire enumerations such as
    /// event-action type retain their actual numeric values.
    pub fn variants(self, version: u32) -> &'static [EnumerationVariant] {
        match self {
            Self::FadeCurve => CURVE_VARIANTS,
            Self::TimePoint if version >= 140 => TIME_POINT_140_VARIANTS,
            Self::TimePoint => TIME_POINT_VARIANTS,
            Self::ParameterCategory if version >= 145 => PARAMETER_CATEGORY_145_VARIANTS,
            Self::ParameterCategory => PARAMETER_CATEGORY_VARIANTS,
            Self::PropertyCategory if version >= 145 => PROPERTY_CATEGORY_145_VARIANTS,
            Self::PropertyCategory if version >= 128 => PROPERTY_CATEGORY_128_VARIANTS,
            Self::PropertyCategory => PROPERTY_CATEGORY_VARIANTS,
            Self::CoordinateMode => COORDINATE_MODE_VARIANTS,
            Self::StateChangeOccursAt if version >= 140 => TIME_POINT_140_VARIANTS,
            Self::StateChangeOccursAt => TIME_POINT_VARIANTS,
            Self::AssociationMode => ASSOCIATION_MODE_VARIANTS,
            Self::PlaylistTransitionMode => PLAYLIST_TRANSITION_VARIANTS,
            Self::PlaylistRandomMode => PLAYLIST_RANDOM_VARIANTS,
            Self::PlaylistContainerMode => PLAYLIST_CONTAINER_VARIANTS,
            Self::TrackType if version >= 112 => TRACK_TYPE_112_VARIANTS,
            Self::TrackType => TRACK_TYPE_VARIANTS,
            Self::EventActionType if version >= 113 => EVENT_ACTION_TYPE_113_VARIANTS,
            Self::EventActionType if version >= 112 => EVENT_ACTION_TYPE_112_VARIANTS,
            Self::EventActionType => EVENT_ACTION_TYPE_VARIANTS,
            Self::EventActionMode if version >= 125 => EVENT_ACTION_MODE_125_VARIANTS,
            Self::EventActionMode => EVENT_ACTION_MODE_VARIANTS,
            Self::EventActionScope => EVENT_ACTION_SCOPE_VARIANTS,
            Self::ValueApplyMode => VALUE_APPLY_MODE_VARIANTS,
            Self::SeekType => SEEK_TYPE_VARIANTS,
            Self::MusicJumpMode => MUSIC_JUMP_MODE_VARIANTS,
            Self::MusicSynchronizeMode => MUSIC_SYNCHRONIZE_MODE_VARIANTS,
            Self::VirtualVoiceBehavior if version >= 140 => VIRTUAL_VOICE_BEHAVIOR_140_VARIANTS,
            Self::VirtualVoiceBehavior => VIRTUAL_VOICE_BEHAVIOR_VARIANTS,
            Self::VirtualVoiceOnReturn => VIRTUAL_VOICE_ON_RETURN_VARIANTS,
            Self::AudioSourceType if version < 112 => AUDIO_SOURCE_TYPE_LEGACY_VARIANTS,
            Self::AudioSourceType => AUDIO_SOURCE_TYPE_VARIANTS,
            Self::GameParameterBuiltIn if version >= 128 => GAME_PARAMETER_BUILT_IN_128_VARIANTS,
            Self::GameParameterBuiltIn => GAME_PARAMETER_BUILT_IN_VARIANTS,
            Self::GameParameterInterpolation => GAME_PARAMETER_INTERPOLATION_VARIANTS,
            Self::VoiceFilterBehavior => VOICE_FILTER_BEHAVIOR_VARIANTS,
            Self::PlaybackLimitScope => PLAYBACK_LIMIT_SCOPE_VARIANTS,
            Self::PlaybackLimitPriority => PLAYBACK_LIMIT_PRIORITY_VARIANTS,
            Self::PlaybackLimitReached => PLAYBACK_LIMIT_REACHED_VARIANTS,
            Self::BusHdrReleaseMode => BUS_HDR_RELEASE_MODE_VARIANTS,
            Self::BusDuckingTarget => BUS_DUCKING_TARGET_VARIANTS,
            Self::AudioPlayType => AUDIO_PLAY_TYPE_VARIANTS,
            Self::AudioPlayMode => AUDIO_PLAY_MODE_VARIANTS,
            Self::AudioPlayRandomType => AUDIO_PLAY_RANDOM_TYPE_VARIANTS,
            Self::AudioPlaySequenceEnd => AUDIO_PLAY_SEQUENCE_END_VARIANTS,
            Self::AudioPlayTransitionType => AUDIO_PLAY_TRANSITION_TYPE_VARIANTS,
            Self::SoundPlaylistScope => SOUND_PLAYLIST_SCOPE_VARIANTS,
            Self::PositioningType => POSITIONING_TYPE_VARIANTS,
            Self::PositionSourceMode if version >= 132 => POSITION_SOURCE_MODE_132_VARIANTS,
            Self::PositionSourceMode => POSITION_SOURCE_MODE_VARIANTS,
            Self::PositioningSpatialization => POSITIONING_SPATIALIZATION_VARIANTS,
            Self::SpeakerPanningMode if version >= 140 => SPEAKER_PANNING_MODE_140_VARIANTS,
            Self::SpeakerPanningMode => SPEAKER_PANNING_MODE_VARIANTS,
            Self::SoundMidiEventPlayOn => SOUND_MIDI_EVENT_PLAY_ON_VARIANTS,
            Self::MusicMidiClipTempoSource => MUSIC_MIDI_CLIP_TEMPO_SOURCE_VARIANTS,
            Self::ModulatorScope => MODULATOR_SCOPE_VARIANTS,
            Self::ModulatorTriggerOn => MODULATOR_TRIGGER_ON_VARIANTS,
            Self::ModulatorWaveform if version >= 125 => MODULATOR_WAVEFORM_125_VARIANTS,
            Self::ModulatorWaveform => MODULATOR_WAVEFORM_VARIANTS,
            Self::MusicTrackClipCurveType if version >= 112 => {
                MUSIC_TRACK_CLIP_CURVE_TYPE_112_VARIANTS
            }
            Self::MusicTrackClipCurveType => MUSIC_TRACK_CLIP_CURVE_TYPE_VARIANTS,
        }
    }

    pub fn variant(self, version: u32, value: u32) -> Option<EnumerationVariant> {
        self.variants(version)
            .iter()
            .copied()
            .find(|variant| variant.value == value)
    }
}

const fn variant(value: u32, name: &'static str) -> EnumerationVariant {
    EnumerationVariant { value, name }
}

const CURVE_VARIANTS: &[EnumerationVariant] = &[
    variant(9, "constant"),
    variant(4, "linear"),
    variant(5, "s"),
    variant(3, "s_inverted"),
    variant(1, "sine"),
    variant(7, "sine_reciprocal"),
    variant(2, "logarithmic_1dot41"),
    variant(0, "logarithmic_3dot0"),
    variant(6, "exponential_1dot41"),
    variant(8, "exponential_3dot0"),
];
const TIME_POINT_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "immediate"),
    variant(1, "next_grid"),
    variant(2, "next_bar"),
    variant(3, "next_beat"),
    variant(4, "next_cue"),
    variant(5, "custom_cue"),
    variant(6, "entry_cue"),
    variant(7, "exit_cue"),
];
const TIME_POINT_140_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "immediate"),
    variant(1, "next_grid"),
    variant(2, "next_bar"),
    variant(3, "next_beat"),
    variant(4, "next_cue"),
    variant(5, "custom_cue"),
    variant(6, "entry_cue"),
    variant(7, "exit_cue"),
    variant(9, "last_exit_position"),
];
const PARAMETER_CATEGORY_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "game_parameter"),
    variant(1, "midi_parameter"),
    variant(2, "modulator"),
];
const PARAMETER_CATEGORY_145_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "game_parameter"),
    variant(1, "midi_parameter"),
    variant(4, "modulator"),
];
const PROPERTY_CATEGORY_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "unidirectional"),
    variant(1, "bidirectional"),
    variant(2, "bidirectional_ranged"),
    variant(3, "boolean"),
];
const PROPERTY_CATEGORY_128_VARIANTS: &[EnumerationVariant] = &[
    variant(1, "unidirectional"),
    variant(2, "bidirectional"),
    variant(3, "bidirectional_ranged"),
    variant(4, "boolean"),
];
const PROPERTY_CATEGORY_145_VARIANTS: &[EnumerationVariant] = &[
    variant(1, "unidirectional"),
    variant(2, "bidirectional"),
    variant(3, "bidirectional_ranged"),
    variant(6, "boolean"),
    variant(4, "unknown_6"),
];
const COORDINATE_MODE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "linear"),
    variant(2, "scaled"),
    variant(3, "scaled_3"),
];
const ASSOCIATION_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "best_match"), variant(1, "weighted")];
const TRACK_TYPE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "normal"),
    variant(1, "random_step"),
    variant(2, "sequence_step"),
];
const TRACK_TYPE_112_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "normal"),
    variant(1, "random_step"),
    variant(2, "sequence_step"),
    variant(3, "switcher"),
];
const PLAYLIST_TRANSITION_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "xfade_amp"),
    variant(2, "xfade_power"),
    variant(3, "delay"),
    variant(4, "sample_accurate"),
    variant(5, "trigger_rate"),
];
const PLAYLIST_RANDOM_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "standard"), variant(1, "shuffle")];
const PLAYLIST_CONTAINER_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "sequence"), variant(1, "random")];
const VALUE_APPLY_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(1, "absolute"), variant(2, "relative")];
const SEEK_TYPE_VARIANTS: &[EnumerationVariant] = &[variant(0, "time"), variant(1, "percent")];
const MUSIC_JUMP_MODE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "start"),
    variant(1, "specific"),
    variant(3, "next"),
    variant(2, "last_played"),
];
const MUSIC_SYNCHRONIZE_MODE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "entry_cue"),
    variant(2, "random_cue"),
    variant(3, "custom_cue"),
    variant(1, "same_time_as_playing_segment"),
];
const VIRTUAL_VOICE_BEHAVIOR_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "continue_to_play"),
    variant(1, "kill_voice"),
    variant(2, "send_to_virtual_voice"),
];
const VIRTUAL_VOICE_BEHAVIOR_140_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "continue_to_play"),
    variant(1, "kill_voice"),
    variant(2, "send_to_virtual_voice"),
    variant(3, "kill_if_finite_else_virtual"),
];
const VIRTUAL_VOICE_ON_RETURN_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "play_from_beginning"),
    variant(1, "play_from_elapsed_time"),
    variant(2, "resume"),
];
const EVENT_ACTION_TYPE_VARIANTS: &[EnumerationVariant] = &[
    variant(1, "stop_audio"),
    variant(2, "pause_audio"),
    variant(3, "resume_audio"),
    variant(4, "play_audio"),
    variant(6, "mute"),
    variant(7, "unmute"),
    variant(8, "set_voice_pitch"),
    variant(9, "reset_voice_pitch"),
    variant(10, "set_voice_volume"),
    variant(11, "reset_voice_volume"),
    variant(12, "set_bus_volume"),
    variant(13, "reset_bus_volume"),
    variant(14, "set_voice_low_pass_filter"),
    variant(15, "reset_voice_low_pass_filter"),
    variant(16, "enable_state_availability"),
    variant(17, "disable_state_availability"),
    variant(18, "activate_state"),
    variant(19, "set_game_parameter"),
    variant(20, "reset_game_parameter"),
    variant(25, "activate_switch"),
    variant(26, "set_bypass_effect"),
    variant(27, "reset_bypass_effect"),
    variant(28, "break_audio"),
    variant(29, "activate_trigger"),
    variant(30, "seek_audio"),
];
const EVENT_ACTION_TYPE_112_VARIANTS: &[EnumerationVariant] = &[
    variant(1, "stop_audio"),
    variant(2, "pause_audio"),
    variant(3, "resume_audio"),
    variant(4, "play_audio"),
    variant(6, "mute"),
    variant(7, "unmute"),
    variant(8, "set_voice_pitch"),
    variant(9, "reset_voice_pitch"),
    variant(10, "set_voice_volume"),
    variant(11, "reset_voice_volume"),
    variant(12, "set_bus_volume"),
    variant(13, "reset_bus_volume"),
    variant(14, "set_voice_low_pass_filter"),
    variant(15, "reset_voice_low_pass_filter"),
    variant(16, "enable_state_availability"),
    variant(17, "disable_state_availability"),
    variant(18, "activate_state"),
    variant(19, "set_game_parameter"),
    variant(20, "reset_game_parameter"),
    variant(25, "activate_switch"),
    variant(26, "set_bypass_effect"),
    variant(27, "reset_bypass_effect"),
    variant(28, "break_audio"),
    variant(29, "activate_trigger"),
    variant(30, "seek_audio"),
    variant(31, "release_envelope"),
    variant(32, "set_voice_high_pass_filter"),
    variant(48, "reset_voice_high_pass_filter"),
];
const EVENT_ACTION_TYPE_113_VARIANTS: &[EnumerationVariant] = &[
    variant(1, "stop_audio"),
    variant(2, "pause_audio"),
    variant(3, "resume_audio"),
    variant(4, "play_audio"),
    variant(6, "mute"),
    variant(7, "unmute"),
    variant(8, "set_voice_pitch"),
    variant(9, "reset_voice_pitch"),
    variant(10, "set_voice_volume"),
    variant(11, "reset_voice_volume"),
    variant(12, "set_bus_volume"),
    variant(13, "reset_bus_volume"),
    variant(14, "set_voice_low_pass_filter"),
    variant(15, "reset_voice_low_pass_filter"),
    variant(16, "enable_state_availability"),
    variant(17, "disable_state_availability"),
    variant(18, "activate_state"),
    variant(19, "set_game_parameter"),
    variant(20, "reset_game_parameter"),
    variant(25, "activate_switch"),
    variant(26, "set_bypass_effect"),
    variant(27, "reset_bypass_effect"),
    variant(28, "break_audio"),
    variant(29, "activate_trigger"),
    variant(30, "seek_audio"),
    variant(31, "release_envelope"),
    variant(32, "set_voice_high_pass_filter"),
    variant(33, "post_event"),
    variant(34, "reset_playlist"),
    variant(48, "reset_voice_high_pass_filter"),
];
const EVENT_ACTION_MODE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "one"),
    variant(2, "all"),
    variant(4, "all_except"),
];
const EVENT_ACTION_MODE_125_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "none"), variant(1, "one"), variant(2, "all")];
const EVENT_ACTION_SCOPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "global"), variant(1, "game_object")];
const AUDIO_SOURCE_TYPE_LEGACY_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "embedded"),
    variant(1, "streamed"),
    variant(2, "streamed_prefetched"),
];
const AUDIO_SOURCE_TYPE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "embedded"),
    variant(1, "streamed_prefetched"),
    variant(2, "streamed"),
];
const GAME_PARAMETER_BUILT_IN_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "distance"),
    variant(2, "azimuth"),
    variant(3, "elevation"),
    variant(4, "object_to_listener_angle"),
    variant(5, "obstruction"),
    variant(6, "occlusion"),
];
const GAME_PARAMETER_BUILT_IN_128_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "distance"),
    variant(2, "azimuth"),
    variant(3, "elevation"),
    variant(4, "emitter_cone"),
    variant(5, "obstruction"),
    variant(6, "occlusion"),
    variant(7, "listener_cone"),
    variant(8, "diffraction"),
];
const GAME_PARAMETER_INTERPOLATION_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "slew_rate"),
    variant(2, "filtering_over_time"),
];
const VOICE_FILTER_BEHAVIOR_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "sum_all_value"), variant(1, "use_highest_value")];
const PLAYBACK_LIMIT_SCOPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "per_game_object"), variant(1, "globally")];
const PLAYBACK_LIMIT_PRIORITY_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "discard_oldest_instance"),
    variant(1, "discard_newest_instance"),
];
const PLAYBACK_LIMIT_REACHED_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "kill_voice"),
    variant(1, "use_virtual_voice_setting"),
];
const BUS_HDR_RELEASE_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "linear"), variant(1, "exponential")];
const BUS_DUCKING_TARGET_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "voice_volume"), variant(5, "bus_volume")];
const AUDIO_PLAY_TYPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "sequence"), variant(1, "random")];
const AUDIO_PLAY_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "step"), variant(1, "continuous")];
const AUDIO_PLAY_RANDOM_TYPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "standard"), variant(1, "shuffle")];
const AUDIO_PLAY_SEQUENCE_END_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "restart"), variant(1, "play_in_reserve_order")];
const AUDIO_PLAY_TRANSITION_TYPE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "xfade_amp"),
    variant(2, "xfade_power"),
    variant(3, "delay"),
    variant(4, "sample_accurate"),
    variant(5, "trigger_rate"),
];
const SOUND_PLAYLIST_SCOPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "game_object"), variant(1, "global")];
const POSITIONING_TYPE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "two_dimension"), variant(1, "three_dimension")];
const POSITION_SOURCE_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "user_defined"), variant(1, "game_defined")];
const POSITION_SOURCE_MODE_132_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "emitter"),
    variant(1, "emitter_with_automation"),
    variant(2, "listener_with_automation"),
];
const POSITIONING_SPATIALIZATION_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "none"),
    variant(1, "position"),
    variant(2, "position_and_orientation"),
];
const SPEAKER_PANNING_MODE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "direct_assignment"), variant(1, "balance_fade")];
const SPEAKER_PANNING_MODE_140_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "direct_assignment"),
    variant(1, "balance_fade"),
    variant(2, "steering"),
];
const SOUND_MIDI_EVENT_PLAY_ON_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "note_on"), variant(2, "note_off")];
const MUSIC_MIDI_CLIP_TEMPO_SOURCE_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "hierarchy"), variant(1, "file")];
const MODULATOR_SCOPE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "voice"),
    variant(1, "note_or_event"),
    variant(2, "game_object"),
    variant(3, "global"),
];
const MODULATOR_TRIGGER_ON_VARIANTS: &[EnumerationVariant] =
    &[variant(0, "play"), variant(2, "note_off")];
const MODULATOR_WAVEFORM_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "sine"),
    variant(1, "triangle"),
    variant(2, "square"),
    variant(3, "saw_up"),
    variant(4, "saw_down"),
];
const MODULATOR_WAVEFORM_125_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "sine"),
    variant(1, "triangle"),
    variant(2, "square"),
    variant(3, "saw_up"),
    variant(4, "saw_down"),
    variant(5, "random"),
];
const MUSIC_TRACK_CLIP_CURVE_TYPE_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "voice_volume"),
    variant(1, "voice_low_pass_filter"),
    variant(2, "clip_fade_in"),
    variant(3, "clip_fade_out"),
];
const MUSIC_TRACK_CLIP_CURVE_TYPE_112_VARIANTS: &[EnumerationVariant] = &[
    variant(0, "voice_volume"),
    variant(1, "voice_low_pass_filter"),
    variant(2, "voice_high_pass_filter"),
    variant(3, "clip_fade_in"),
    variant(4, "clip_fade_out"),
];
