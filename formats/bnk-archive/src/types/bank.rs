//! Top-level bank, chunk, and embedded-media types.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::{
    BankReferences, EnvironmentSettings, GameSynchronization, HierarchyObject, PlatformSetting,
    PluginReference,
};
use crate::error::Result;
use crate::version::BankVersion;

pub type Identifier = u32;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ChunkId(pub [u8; 4]);

impl ChunkId {
    pub const BKHD: Self = Self(*b"BKHD");
    pub const DIDX: Self = Self(*b"DIDX");
    pub const DATA: Self = Self(*b"DATA");
    pub const INIT: Self = Self(*b"INIT");
    pub const STMG: Self = Self(*b"STMG");
    pub const HIRC: Self = Self(*b"HIRC");
    pub const STID: Self = Self(*b"STID");
    pub const ENVS: Self = Self(*b"ENVS");
    pub const PLAT: Self = Self(*b"PLAT");

    pub fn as_bytes(self) -> [u8; 4] {
        self.0
    }

    pub fn as_str(self) -> Option<&'static str> {
        match self {
            Self::BKHD => Some("BKHD"),
            Self::DIDX => Some("DIDX"),
            Self::DATA => Some("DATA"),
            Self::INIT => Some("INIT"),
            Self::STMG => Some("STMG"),
            Self::HIRC => Some("HIRC"),
            Self::STID => Some("STID"),
            Self::ENVS => Some("ENVS"),
            Self::PLAT => Some("PLAT"),
            _ => None,
        }
    }
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(value) = std::str::from_utf8(&self.0) {
            formatter.debug_tuple("ChunkId").field(&value).finish()
        } else {
            formatter.debug_tuple("ChunkId").field(&self.0).finish()
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankHeader {
    pub version: BankVersion,
    pub id: Identifier,
    /// Raw four-byte language field. Since Wwise 125 it is semantically an
    /// object identifier, but the wire representation remains `u32`.
    pub language: u32,
    /// Twinning's lossless `header_expand` bytes.
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    pub header_expand: Vec<u8>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct SoundBank {
    pub header: BankHeader,
    /// Chunks following BKHD in their original order.
    pub chunks: Vec<BankChunk>,
}

impl SoundBank {
    pub fn version(&self) -> BankVersion {
        self.header.version
    }

    pub fn chunk(&self, id: ChunkId) -> Option<&BankChunk> {
        self.chunks.iter().find(|chunk| chunk.id() == id)
    }

    pub fn chunks(&self, id: ChunkId) -> impl Iterator<Item = &BankChunk> {
        self.chunks.iter().filter(move |chunk| chunk.id() == id)
    }

    pub fn chunk_mut(&mut self, id: ChunkId) -> Option<&mut BankChunk> {
        self.chunks.iter_mut().find(|chunk| chunk.id() == id)
    }

    /// Validate every known chunk and structured HIRC object using the
    /// crate's canonical, lossless model.
    pub fn validate(&self) -> Result<()> {
        crate::binary::validate_sound_bank(self, false)
    }

    /// Validate the bank against Twinning's stricter top-level chunk order in
    /// addition to all canonical field constraints.
    pub fn validate_twinning_compatibility(&self) -> Result<()> {
        crate::binary::validate_sound_bank(self, true)
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", serde(tag = "chunk", content = "value"))]
pub enum BankChunk {
    MediaIndex(Vec<MediaIndexEntry>),
    MediaData(#[cfg_attr(feature = "serde", serde(with = "serde_bytes"))] Vec<u8>),
    Plugins(Vec<PluginReference>),
    GameSynchronization(GameSynchronization),
    Hierarchy(Vec<HierarchyObject>),
    References(BankReferences),
    Environments(EnvironmentSettings),
    Platform(PlatformSetting),
    Unknown(RawChunk),
}

impl BankChunk {
    pub fn id(&self) -> ChunkId {
        match self {
            Self::MediaIndex(_) => ChunkId::DIDX,
            Self::MediaData(_) => ChunkId::DATA,
            Self::Plugins(_) => ChunkId::INIT,
            Self::GameSynchronization(_) => ChunkId::STMG,
            Self::Hierarchy(_) => ChunkId::HIRC,
            Self::References(_) => ChunkId::STID,
            Self::Environments(_) => ChunkId::ENVS,
            Self::Platform(_) => ChunkId::PLAT,
            Self::Unknown(chunk) => chunk.id,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawChunk {
    pub id: ChunkId,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    pub data: Vec<u8>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaIndexEntry {
    pub id: Identifier,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedMedia<'a> {
    pub id: Identifier,
    pub data: &'a [u8],
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedEmbeddedMedia {
    pub id: Identifier,
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes"))]
    pub data: Vec<u8>,
}
