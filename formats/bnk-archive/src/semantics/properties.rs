//! Versioned AkPropValue catalogs.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::EnumerationKind;

/// The three independent common-property identifier catalogs used by Wwise.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CommonPropertyDomain {
    Audio,
    Modulator,
    EventAction,
}

/// How the four-byte `AkPropValue` union is interpreted.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CommonPropertyValueKind {
    Boolean,
    Integer,
    Floater,
    Enumerated,
    Identifier,
}

/// A Twinning-defined default value for a common property.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum CommonPropertyDefault {
    Boolean(bool),
    Integer(i32),
    Floater(f32),
    Enumerated(u32),
    Identifier(u32),
}

/// A decoded value from an `AkPropValue` union.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "snake_case")
)]
pub enum CommonPropertyValue {
    Boolean(bool),
    Integer(i32),
    Floater(f32),
    Enumerated(u32),
    Identifier(u32),
}

/// Version-resolved description of a common property identifier.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommonPropertyDescriptor {
    pub id: u8,
    pub name: &'static str,
    pub kind: CommonPropertyValueKind,
    pub default: CommonPropertyDefault,
    /// Exact Twinning enumeration when `kind` is `Enumerated`.
    pub enumeration: Option<EnumerationKind>,
}

impl CommonPropertyDescriptor {
    const fn float(id: u8, name: &'static str, default: f32) -> Self {
        Self {
            id,
            name,
            kind: CommonPropertyValueKind::Floater,
            default: CommonPropertyDefault::Floater(default),
            enumeration: None,
        }
    }

    const fn integer(id: u8, name: &'static str, default: i32) -> Self {
        Self {
            id,
            name,
            kind: CommonPropertyValueKind::Integer,
            default: CommonPropertyDefault::Integer(default),
            enumeration: None,
        }
    }

    const fn enumerated(
        id: u8,
        name: &'static str,
        default: u32,
        enumeration: EnumerationKind,
    ) -> Self {
        Self {
            id,
            name,
            kind: CommonPropertyValueKind::Enumerated,
            default: CommonPropertyDefault::Enumerated(default),
            enumeration: Some(enumeration),
        }
    }

    const fn boolean(id: u8, name: &'static str, default: bool) -> Self {
        Self {
            id,
            name,
            kind: CommonPropertyValueKind::Boolean,
            default: CommonPropertyDefault::Boolean(default),
            enumeration: None,
        }
    }

    const fn identifier(id: u8, name: &'static str, default: u32) -> Self {
        Self {
            id,
            name,
            kind: CommonPropertyValueKind::Identifier,
            default: CommonPropertyDefault::Identifier(default),
            enumeration: None,
        }
    }
}

const fn f(id: u8, name: &'static str) -> CommonPropertyDescriptor {
    CommonPropertyDescriptor::float(id, name, 0.0)
}

const fn i(id: u8, name: &'static str, default: i32) -> CommonPropertyDescriptor {
    CommonPropertyDescriptor::integer(id, name, default)
}

const fn e(id: u8, name: &'static str, enumeration: EnumerationKind) -> CommonPropertyDescriptor {
    CommonPropertyDescriptor::enumerated(id, name, 0, enumeration)
}

const fn id(id: u8, name: &'static str) -> CommonPropertyDescriptor {
    CommonPropertyDescriptor::identifier(id, name, 0)
}

/// Resolve a common-property identifier using the exact version ranges used by
/// Twinning. Unknown identifiers are intentionally returned as `None` so a
/// newer/custom bank can still preserve the raw union value.
pub fn common_property_descriptor(
    version: u32,
    domain: CommonPropertyDomain,
    property_id: u8,
) -> Option<CommonPropertyDescriptor> {
    match domain {
        CommonPropertyDomain::EventAction => event_action_property(version, property_id),
        CommonPropertyDomain::Modulator => modulator_property(version, property_id),
        CommonPropertyDomain::Audio => audio_property(version, property_id),
    }
}

fn event_action_property(version: u32, property_id: u8) -> Option<CommonPropertyDescriptor> {
    let base = if version < 118 {
        14
    } else if version < 150 {
        15
    } else {
        57
    };
    match property_id {
        value if value == base => Some(i(value, "delay", 0)),
        value if value == base + 1 => Some(i(value, "fade_time", 0)),
        value if value == base + 2 => {
            Some(CommonPropertyDescriptor::float(value, "probability", 100.0))
        }
        _ => None,
    }
}

fn modulator_property(version: u32, property_id: u8) -> Option<CommonPropertyDescriptor> {
    if version < 112 {
        return None;
    }
    if version < 150 {
        return match property_id {
            0 => Some(e(0, "scope", EnumerationKind::ModulatorScope)),
            15 => Some(e(15, "trigger_on", EnumerationKind::ModulatorTriggerOn)),
            2 => Some(CommonPropertyDescriptor::float(2, "depth", 100.0)),
            4 => Some(CommonPropertyDescriptor::float(4, "frequency", 1.0)),
            5 => Some(e(5, "waveform", EnumerationKind::ModulatorWaveform)),
            6 => Some(f(6, "smoothing")),
            7 => Some(CommonPropertyDescriptor::float(
                7,
                "pulse_width_modulation",
                50.0,
            )),
            3 => Some(f(3, "attack")),
            8 => Some(f(8, "initial_phase_offset")),
            9 => Some(CommonPropertyDescriptor::float(9, "attack_time", 0.2)),
            10 => Some(CommonPropertyDescriptor::float(10, "attack_curve", 50.0)),
            11 => Some(CommonPropertyDescriptor::float(11, "decay_time", 0.2)),
            12 => Some(CommonPropertyDescriptor::float(12, "sustain_level", 100.0)),
            14 => Some(CommonPropertyDescriptor::float(14, "release_time", 0.5)),
            13 => Some(f(13, "sustain_time")),
            19 => Some(f(19, "initial_delay")),
            16 => Some(CommonPropertyDescriptor::float(16, "duration", 1.0)),
            17 => Some(i(17, "loop", 1)),
            18 => Some(CommonPropertyDescriptor::float(18, "playback_rate", 1.0)),
            1 => Some(CommonPropertyDescriptor::boolean(1, "stop_playback", true)),
            _ => None,
        };
    }
    match property_id {
        0 => Some(e(0, "scope", EnumerationKind::ModulatorScope)),
        16 => Some(e(16, "trigger_on", EnumerationKind::ModulatorTriggerOn)),
        2 => Some(CommonPropertyDescriptor::float(2, "depth", 100.0)),
        4 => Some(CommonPropertyDescriptor::float(4, "frequency", 1.0)),
        5 => Some(e(5, "waveform", EnumerationKind::ModulatorWaveform)),
        6 => Some(f(6, "smoothing")),
        7 => Some(CommonPropertyDescriptor::float(
            7,
            "pulse_width_modulation",
            50.0,
        )),
        3 => Some(f(3, "attack")),
        8 => Some(f(8, "initial_phase_offset")),
        10 => Some(CommonPropertyDescriptor::float(10, "attack_time", 0.2)),
        11 => Some(CommonPropertyDescriptor::float(11, "attack_curve", 50.0)),
        12 => Some(CommonPropertyDescriptor::float(12, "decay_time", 0.2)),
        13 => Some(CommonPropertyDescriptor::float(13, "sustain_level", 100.0)),
        15 => Some(CommonPropertyDescriptor::float(15, "release_time", 0.5)),
        14 => Some(f(14, "sustain_time")),
        20 => Some(f(20, "initial_delay")),
        17 => Some(CommonPropertyDescriptor::float(17, "duration", 1.0)),
        18 => Some(i(18, "loop", 1)),
        19 => Some(CommonPropertyDescriptor::float(19, "playback_rate", 1.0)),
        1 => Some(CommonPropertyDescriptor::boolean(1, "stop_playback", true)),
        _ => None,
    }
}

fn audio_property(version: u32, property_id: u8) -> Option<CommonPropertyDescriptor> {
    if version < 88 {
        return audio_72(property_id);
    }
    if version < 112 {
        return audio_88(property_id);
    }
    if version < 118 {
        return audio_112(property_id);
    }
    if version < 128 {
        return audio_118(property_id);
    }
    if version < 150 {
        return audio_128_149(version, property_id);
    }
    audio_150(property_id)
}

fn audio_72(property_id: u8) -> Option<CommonPropertyDescriptor> {
    match property_id {
        4 => Some(f(4, "bus_volume")),
        23 => Some(f(23, "output_bus_volume")),
        24 => Some(f(24, "output_bus_low_pass_filter")),
        0 => Some(f(0, "voice_volume")),
        2 => Some(f(2, "voice_pitch")),
        3 => Some(f(3, "voice_low_pass_filter")),
        22 => Some(f(22, "game_defined_auxiliary_send_volume")),
        18..=21 => Some(f(property_id, USER_AUX_VOLUME[(property_id - 18) as usize])),
        13 => Some(f(13, "positioning_center_percent")),
        11 => Some(f(11, "positioning_speaker_panning_x")),
        12 => Some(f(12, "positioning_speaker_panning_y")),
        5 => Some(f(5, "playback_priority_value")),
        6 => Some(f(6, "playback_priority_offset_at_maximum_distance")),
        7 => Some(i(7, "playback_loop", 0)),
        8 => Some(f(8, "motion_volume_offset")),
        9 => Some(f(9, "motion_low_pass_filter")),
        _ => None,
    }
}

fn audio_88(property_id: u8) -> Option<CommonPropertyDescriptor> {
    audio_72(property_id).or_else(|| match property_id {
        33 => Some(f(33, "voice_volume_make_up_gain")),
        26 => Some(f(26, "hdr_threshold")),
        27 => Some(f(27, "hdr_ratio")),
        28 => Some(f(28, "hdr_release_time")),
        29 => Some(id(29, "hdr_window_tap_output_game_parameter_identifier")),
        30 => Some(f(30, "hdr_window_tap_output_game_parameter_minimum")),
        31 => Some(f(31, "hdr_window_tap_output_game_parameter_maximum")),
        32 => Some(CommonPropertyDescriptor::float(
            32,
            "hdr_envelope_tracking_active_range",
            12.0,
        )),
        25 => Some(f(25, "playback_initial_delay")),
        _ => None,
    })
}

fn midi_property(property_id: u8) -> Option<CommonPropertyDescriptor> {
    match property_id {
        45 => Some(i(45, "midi_note_tracking_root_note", 60)),
        46 => Some(e(
            46,
            "midi_event_play_on",
            EnumerationKind::SoundMidiEventPlayOn,
        )),
        47 => Some(i(47, "midi_transformation_transposition", 0)),
        48 => Some(i(48, "midi_transformation_velocity_offset", 0)),
        49 => Some(i(49, "midi_filter_key_range_minimum", 0)),
        50 => Some(i(50, "midi_filter_key_range_maximum", 127)),
        51 => Some(i(51, "midi_filter_velocity_range_minimum", 0)),
        52 => Some(i(52, "midi_filter_velocity_range_maximum", 127)),
        53 => Some(i(53, "midi_filter_channel", 65_535)),
        55 => Some(e(
            55,
            "midi_clip_tempo_source",
            EnumerationKind::MusicMidiClipTempoSource,
        )),
        56 => Some(id(56, "midi_target_identifier")),
        _ => None,
    }
}

fn audio_112(property_id: u8) -> Option<CommonPropertyDescriptor> {
    match property_id {
        5 => Some(f(5, "bus_volume")),
        23 => Some(f(23, "output_bus_volume")),
        25 => Some(f(25, "output_bus_low_pass_filter")),
        24 => Some(f(24, "output_bus_high_pass_filter")),
        0 => Some(f(0, "voice_volume")),
        2 => Some(f(2, "voice_pitch")),
        3 => Some(f(3, "voice_low_pass_filter")),
        4 => Some(f(4, "voice_high_pass_filter")),
        33 => Some(f(33, "voice_volume_make_up_gain")),
        22 => Some(f(22, "game_defined_auxiliary_send_volume")),
        18..=21 => Some(f(property_id, USER_AUX_VOLUME[(property_id - 18) as usize])),
        13 => Some(f(13, "positioning_center_percent")),
        11 => Some(f(11, "positioning_speaker_panning_x")),
        12 => Some(f(12, "positioning_speaker_panning_y")),
        26 => Some(f(26, "hdr_threshold")),
        27 => Some(f(27, "hdr_ratio")),
        28 => Some(f(28, "hdr_release_time")),
        29 => Some(id(29, "hdr_window_tap_output_game_parameter_identifier")),
        30 => Some(f(30, "hdr_window_tap_output_game_parameter_minimum")),
        31 => Some(f(31, "hdr_window_tap_output_game_parameter_maximum")),
        32 => Some(CommonPropertyDescriptor::float(
            32,
            "hdr_envelope_tracking_active_range",
            12.0,
        )),
        6 => Some(f(6, "playback_priority_value")),
        7 => Some(f(7, "playback_priority_offset_at_maximum_distance")),
        59 => Some(f(59, "playback_initial_delay")),
        58 => Some(i(58, "playback_loop", 0)),
        54 => Some(CommonPropertyDescriptor::float(54, "playback_speed", 1.0)),
        8 => Some(f(8, "motion_volume_offset")),
        9 => Some(f(9, "motion_low_pass_filter")),
        57 => Some(id(57, "mixer_identifier")),
        _ => midi_property(property_id),
    }
}

fn audio_118(property_id: u8) -> Option<CommonPropertyDescriptor> {
    match property_id {
        5 => Some(f(5, "bus_volume")),
        24 => Some(f(24, "output_bus_volume")),
        26 => Some(f(26, "output_bus_low_pass_filter")),
        25 => Some(f(25, "output_bus_high_pass_filter")),
        0 => Some(f(0, "voice_volume")),
        2 => Some(f(2, "voice_pitch")),
        3 => Some(f(3, "voice_low_pass_filter")),
        4 => Some(f(4, "voice_high_pass_filter")),
        6 => Some(f(6, "voice_volume_make_up_gain")),
        23 => Some(f(23, "game_defined_auxiliary_send_volume")),
        19..=22 => Some(f(property_id, USER_AUX_VOLUME[(property_id - 19) as usize])),
        14 => Some(f(14, "positioning_center_percent")),
        12 => Some(f(12, "positioning_speaker_panning_x")),
        13 => Some(f(13, "positioning_speaker_panning_y")),
        27 => Some(f(27, "hdr_threshold")),
        28 => Some(f(28, "hdr_ratio")),
        29 => Some(f(29, "hdr_release_time")),
        30 => Some(id(30, "hdr_window_tap_output_game_parameter_identifier")),
        31 => Some(f(31, "hdr_window_tap_output_game_parameter_minimum")),
        32 => Some(f(32, "hdr_window_tap_output_game_parameter_maximum")),
        33 => Some(CommonPropertyDescriptor::float(
            33,
            "hdr_envelope_tracking_active_range",
            12.0,
        )),
        7 => Some(f(7, "playback_priority_value")),
        8 => Some(f(8, "playback_priority_offset_at_maximum_distance")),
        59 => Some(f(59, "playback_initial_delay")),
        58 => Some(i(58, "playback_loop", 0)),
        54 => Some(CommonPropertyDescriptor::float(54, "playback_speed", 1.0)),
        9 => Some(f(9, "motion_volume_offset")),
        10 => Some(f(10, "motion_low_pass_filter")),
        57 => Some(id(57, "mixer_identifier")),
        _ => midi_property(property_id),
    }
}

fn audio_128_149(version: u32, property_id: u8) -> Option<CommonPropertyDescriptor> {
    audio_118(property_id)
        .filter(|descriptor| {
            !matches!(
                descriptor.name,
                "motion_volume_offset" | "motion_low_pass_filter"
            )
        })
        .or_else(|| match property_id {
            68 => Some(f(68, "game_defined_auxiliary_send_low_pass_filter")),
            69 => Some(f(69, "game_defined_auxiliary_send_high_pass_filter")),
            60..=63 => Some(f(property_id, USER_AUX_LPF[(property_id - 60) as usize])),
            64..=67 => Some(f(property_id, USER_AUX_HPF[(property_id - 64) as usize])),
            71 if version >= 132 => Some(CommonPropertyDescriptor::float(
                71,
                "positioning_listener_routing_speaker_panning_division_spatialization_mix",
                100.0,
            )),
            70 if version >= 132 => Some(id(
                70,
                "positioning_listener_routing_attenuation_identifier",
            )),
            72 if version >= 135 => Some(f(72, "early_reflection_auxiliary_send_volume")),
            73 if version >= 140 => Some(f(73, "positioning_speaker_panning_z")),
            _ => None,
        })
}

fn audio_150(property_id: u8) -> Option<CommonPropertyDescriptor> {
    match property_id {
        4 => Some(f(4, "bus_volume")),
        13 => Some(f(13, "output_bus_volume")),
        15 => Some(f(15, "output_bus_low_pass_filter")),
        14 => Some(f(14, "output_bus_high_pass_filter")),
        0 => Some(f(0, "voice_volume")),
        1 => Some(f(1, "voice_pitch")),
        2 => Some(f(2, "voice_low_pass_filter")),
        3 => Some(f(3, "voice_high_pass_filter")),
        5 => Some(f(5, "voice_volume_make_up_gain")),
        12 => Some(f(12, "game_defined_auxiliary_send_volume")),
        24 => Some(f(24, "game_defined_auxiliary_send_low_pass_filter")),
        25 => Some(f(25, "game_defined_auxiliary_send_high_pass_filter")),
        8..=11 => Some(f(property_id, USER_AUX_VOLUME[(property_id - 8) as usize])),
        16..=19 => Some(f(property_id, USER_AUX_LPF[(property_id - 16) as usize])),
        20..=23 => Some(f(property_id, USER_AUX_HPF[(property_id - 20) as usize])),
        26 => Some(f(26, "early_reflection_auxiliary_send_volume")),
        41 => Some(f(41, "positioning_center_percent")),
        35 => Some(f(35, "positioning_speaker_panning_x")),
        36 => Some(f(36, "positioning_speaker_panning_y")),
        37 => Some(f(37, "positioning_speaker_panning_z")),
        42 => Some(CommonPropertyDescriptor::float(
            42,
            "positioning_listener_routing_speaker_panning_division_spatialization_mix",
            100.0,
        )),
        85 => Some(id(
            85,
            "positioning_listener_routing_attenuation_identifier",
        )),
        27 => Some(f(27, "hdr_threshold")),
        28 => Some(f(28, "hdr_ratio")),
        29 => Some(f(29, "hdr_release_time")),
        61 => Some(id(61, "hdr_window_tap_output_game_parameter_identifier")),
        62 => Some(f(62, "hdr_window_tap_output_game_parameter_minimum")),
        63 => Some(f(63, "hdr_window_tap_output_game_parameter_maximum")),
        30 => Some(CommonPropertyDescriptor::float(
            30,
            "hdr_envelope_tracking_active_range",
            12.0,
        )),
        75 => Some(i(75, "midi_note_tracking_root_note", 60)),
        76 => Some(e(
            76,
            "midi_event_play_on",
            EnumerationKind::SoundMidiEventPlayOn,
        )),
        31 => Some(i(31, "midi_transformation_transposition", 0)),
        32 => Some(i(32, "midi_transformation_velocity_offset", 0)),
        77 => Some(i(77, "midi_filter_key_range_minimum", 0)),
        78 => Some(i(78, "midi_filter_key_range_maximum", 127)),
        79 => Some(i(79, "midi_filter_velocity_range_minimum", 0)),
        80 => Some(i(80, "midi_filter_velocity_range_maximum", 127)),
        81 => Some(i(81, "midi_filter_channel", 65_535)),
        82 => Some(e(
            82,
            "midi_clip_tempo_source",
            EnumerationKind::MusicMidiClipTempoSource,
        )),
        83 => Some(id(83, "midi_target_identifier")),
        6 => Some(f(6, "playback_priority_value")),
        56 => Some(f(56, "playback_priority_offset_at_maximum_distance")),
        34 => Some(f(34, "playback_initial_delay")),
        84 => Some(i(84, "playback_loop", 0)),
        33 => Some(CommonPropertyDescriptor::float(33, "playback_speed", 1.0)),
        _ => None,
    }
}

const USER_AUX_VOLUME: [&str; 4] = [
    "user_defined_auxiliary_send_volume_0",
    "user_defined_auxiliary_send_volume_1",
    "user_defined_auxiliary_send_volume_2",
    "user_defined_auxiliary_send_volume_3",
];
const USER_AUX_LPF: [&str; 4] = [
    "user_defined_auxiliary_send_low_pass_filter_0",
    "user_defined_auxiliary_send_low_pass_filter_1",
    "user_defined_auxiliary_send_low_pass_filter_2",
    "user_defined_auxiliary_send_low_pass_filter_3",
];
const USER_AUX_HPF: [&str; 4] = [
    "user_defined_auxiliary_send_high_pass_filter_0",
    "user_defined_auxiliary_send_high_pass_filter_1",
    "user_defined_auxiliary_send_high_pass_filter_2",
    "user_defined_auxiliary_send_high_pass_filter_3",
];
