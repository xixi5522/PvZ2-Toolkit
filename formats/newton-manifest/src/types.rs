use crate::{NewtonError, Result};
use serde::{Deserialize, Serialize};

/// A complete `RESOURCES.NEWTON` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceManifest {
    pub slot_count: u32,
    pub groups: Vec<ResourceGroup>,
}

/// A composite group references subgroups; a simple group owns resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResourceGroup {
    #[serde(rename = "composite")]
    Composite(CompositeGroup),
    #[serde(rename = "simple")]
    Simple(SimpleGroup),
}

impl ResourceGroup {
    pub fn id(&self) -> &str {
        match self {
            Self::Composite(group) => &group.id,
            Self::Simple(group) => &group.id,
        }
    }

    pub fn resolution(&self) -> Option<u32> {
        match self {
            Self::Composite(group) => group.resolution,
            Self::Simple(group) => group.resolution,
        }
    }

    pub fn parent(&self) -> Option<&str> {
        match self {
            Self::Composite(group) => group.parent.as_deref(),
            Self::Simple(group) => group.parent.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeGroup {
    pub id: String,
    #[serde(default, rename = "res", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub subgroups: Vec<Subgroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimpleGroup {
    pub id: String,
    #[serde(default, rename = "res", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subgroup {
    pub id: String,
    #[serde(default, rename = "res", skip_serializing_if = "Option::is_none")]
    pub resolution: Option<u32>,
}

/// A resource record owned by a simple group.
///
/// NEWTON stores separate ID and path presence bits. The semantic model keeps
/// both strings required: decoding rejects records that omit either field, and
/// encoding always writes both presence bits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub slot: u32,
    pub id: String,
    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ax: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ay: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aw: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ah: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub atlas: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl Resource {
    pub fn is_sprite(&self) -> bool {
        self.resource_type == ResourceType::Image && self.parent.is_some()
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Resource type byte stored by NEWTON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Image,
    PopAnim,
    SoundBank,
    File,
    PrimeFont,
    RenderEffect,
    DecodedSoundBank,
}

impl ResourceType {
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::Image => 1,
            Self::PopAnim => 2,
            Self::SoundBank => 3,
            Self::File => 4,
            Self::PrimeFont => 5,
            Self::RenderEffect => 6,
            Self::DecodedSoundBank => 7,
        }
    }
}

impl TryFrom<u8> for ResourceType {
    type Error = NewtonError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Image),
            2 => Ok(Self::PopAnim),
            3 => Ok(Self::SoundBank),
            4 => Ok(Self::File),
            5 => Ok(Self::PrimeFont),
            6 => Ok(Self::RenderEffect),
            7 => Ok(Self::DecodedSoundBank),
            _ => Err(NewtonError::InvalidResourceType(value)),
        }
    }
}
