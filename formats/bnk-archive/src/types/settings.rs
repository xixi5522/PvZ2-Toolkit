//! INIT, STMG, ENVS, STID, and PLAT setting types.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::Identifier;
use crate::version::BankVersion;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginReference {
    pub id: Identifier,
    pub library: String,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct GameSynchronization {
    /// Present before all other STMG values since Wwise 145.
    pub voice_filter_behavior: Option<VoiceFilterBehavior>,
    pub volume_threshold: f32,
    pub maximum_voice_instances: u16,
    /// Canonical value is 50 and this field is present since Wwise 128.
    pub compatibility_value: Option<u16>,
    pub state_groups: Vec<StateGroup>,
    pub switch_groups: Vec<SwitchGroup>,
    pub game_parameters: Vec<GameParameter>,
    /// Twinning's still-unidentified Wwise 140+ `u1` records.
    pub u1: Vec<GameSynchronizationU1>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum VoiceFilterBehavior {
    SumAllValues,
    UseHighestValue,
    Unknown(u16),
}

impl VoiceFilterBehavior {
    pub fn from_raw(value: u16) -> Self {
        match value {
            0 => Self::SumAllValues,
            1 => Self::UseHighestValue,
            value => Self::Unknown(value),
        }
    }

    pub fn raw(self) -> u16 {
        match self {
            Self::SumAllValues => 0,
            Self::UseHighestValue => 1,
            Self::Unknown(value) => value,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateGroup {
    pub id: Identifier,
    pub default_transition_time: u32,
    pub custom_transitions: Vec<StateTransition>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateTransition {
    pub from: Identifier,
    pub to: Identifier,
    pub time: u32,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchGroup {
    pub id: Identifier,
    pub parameter: ParameterReference,
    pub points: Vec<IdentifierGraphPoint>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterReference {
    pub id: Identifier,
    pub category: Option<ParameterCategory>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ParameterCategory {
    GameParameter,
    MidiParameter,
    Modulator,
    Unknown(u8),
}

impl ParameterCategory {
    pub fn from_raw(version: BankVersion, value: u8) -> Self {
        match value {
            0 => Self::GameParameter,
            1 => Self::MidiParameter,
            2 if version.before(145) => Self::Modulator,
            4 if version.at_least(145) => Self::Modulator,
            value => Self::Unknown(value),
        }
    }

    pub fn raw(self, version: BankVersion) -> u8 {
        match self {
            Self::GameParameter => 0,
            Self::MidiParameter => 1,
            Self::Modulator if version.before(145) => 2,
            Self::Modulator => 4,
            Self::Unknown(value) => value,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IdentifierGraphPoint {
    pub x: f32,
    pub y: Identifier,
    pub curve: Curve,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Curve(pub u32);

impl Curve {
    pub fn interpolation(self) -> CurveInterpolation {
        CurveInterpolation::from_raw(self.0)
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CurveInterpolation {
    Exponential3,
    Sine,
    Logarithmic1_41,
    InvertedS,
    Linear,
    S,
    Exponential1_41,
    ReciprocalSine,
    Logarithmic3,
    Constant,
    Unknown(u32),
}

impl CurveInterpolation {
    pub const fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::Logarithmic3,
            1 => Self::Sine,
            2 => Self::Logarithmic1_41,
            3 => Self::InvertedS,
            4 => Self::Linear,
            5 => Self::S,
            6 => Self::Exponential1_41,
            7 => Self::ReciprocalSine,
            8 => Self::Exponential3,
            9 => Self::Constant,
            _ => Self::Unknown(value),
        }
    }

    pub const fn raw(self) -> u32 {
        match self {
            Self::Logarithmic3 => 0,
            Self::Sine => 1,
            Self::Logarithmic1_41 => 2,
            Self::InvertedS => 3,
            Self::Linear => 4,
            Self::S => 5,
            Self::Exponential1_41 => 6,
            Self::ReciprocalSine => 7,
            Self::Exponential3 => 8,
            Self::Constant => 9,
            Self::Unknown(value) => value,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct GameParameter {
    pub id: Identifier,
    pub default_value: f32,
    pub interpolation: Option<GameParameterInterpolation>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameParameterInterpolation {
    pub mode: InterpolationMode,
    pub attack: f32,
    pub release: f32,
    pub built_in_parameter: BuiltInParameter,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum InterpolationMode {
    None,
    SlewRate,
    FilteringOverTime,
    Unknown(u32),
}

impl InterpolationMode {
    pub fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::SlewRate,
            2 => Self::FilteringOverTime,
            value => Self::Unknown(value),
        }
    }

    pub fn raw(self) -> u32 {
        match self {
            Self::None => 0,
            Self::SlewRate => 1,
            Self::FilteringOverTime => 2,
            Self::Unknown(value) => value,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BuiltInParameter {
    None,
    Distance,
    Azimuth,
    Elevation,
    ObjectToListenerAngle,
    EmitterCone,
    Obstruction,
    Occlusion,
    ListenerCone,
    Diffraction,
    Unknown(u8),
}

impl BuiltInParameter {
    pub fn from_raw(version: BankVersion, value: u8) -> Self {
        if version.before(128) {
            match value {
                0 => Self::None,
                1 => Self::Distance,
                2 => Self::Azimuth,
                3 => Self::Elevation,
                4 => Self::ObjectToListenerAngle,
                5 => Self::Obstruction,
                6 => Self::Occlusion,
                value => Self::Unknown(value),
            }
        } else {
            match value {
                0 => Self::None,
                1 => Self::Distance,
                2 => Self::Azimuth,
                3 => Self::Elevation,
                4 => Self::EmitterCone,
                5 => Self::Obstruction,
                6 => Self::Occlusion,
                7 => Self::ListenerCone,
                8 => Self::Diffraction,
                value => Self::Unknown(value),
            }
        }
    }

    /// Return the wire value for this version, or `None` when the named
    /// binding did not exist in that version.
    pub fn raw(self, version: BankVersion) -> Option<u8> {
        match self {
            Self::None => Some(0),
            Self::Distance => Some(1),
            Self::Azimuth => Some(2),
            Self::Elevation => Some(3),
            Self::ObjectToListenerAngle if version.before(128) => Some(4),
            Self::EmitterCone if version.at_least(128) => Some(4),
            Self::Obstruction => Some(5),
            Self::Occlusion => Some(6),
            Self::ListenerCone if version.at_least(128) => Some(7),
            Self::Diffraction if version.at_least(128) => Some(8),
            Self::Unknown(value) => Some(value),
            _ => None,
        }
    }
}

/// Opaque semantic record used by Twinning for the Wwise 140+ STMG tail.
///
/// The neutral field names are intentional: Twinning does not identify the
/// meaning of these six floating-point values.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameSynchronizationU1 {
    pub identifier: Identifier,
    pub u1: f32,
    pub u2: f32,
    pub u3: f32,
    pub u4: f32,
    pub u5: f32,
    pub u6: f32,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentSettings {
    pub obstruction: EnvironmentBundle,
    pub occlusion: EnvironmentBundle,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentBundle {
    pub volume: EnvironmentCurve,
    pub low_pass_filter: EnvironmentCurve,
    pub high_pass_filter: Option<EnvironmentCurve>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentCurve {
    pub enabled: bool,
    pub mode: CoordinateMode,
    pub points: Vec<GraphPoint>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CoordinateMode {
    Linear,
    Scaled,
    Scaled3,
    Unknown(u8),
}

impl CoordinateMode {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Linear,
            2 => Self::Scaled,
            3 => Self::Scaled3,
            value => Self::Unknown(value),
        }
    }

    pub fn raw(self) -> u8 {
        match self {
            Self::Linear => 0,
            Self::Scaled => 2,
            Self::Scaled3 => 3,
            Self::Unknown(value) => value,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphPoint {
    pub x: f32,
    pub y: f32,
    pub curve: Curve,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankReferences {
    /// Canonical STID marker is 1.
    pub marker: u32,
    pub entries: Vec<BankReference>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankReference {
    pub id: Identifier,
    pub name: String,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSetting {
    pub name: String,
}
