use crate::rsg::types::Part1Extra;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic RSB FourCC (`rsb1`).
///
/// RSB stores this numeric value in little-endian order, so the first four
/// bytes on disk are `1bsr`.
pub const RSB_MAGIC: u32 = u32::from_be_bytes(*b"rsb1");

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RsbHeader {
    /// Semantic FourCC value. Written as a little-endian [`u32`].
    pub magic: u32,
    pub version: u32,
    /// Total size of the header + metadata sections (including manifests).
    /// Twinning: `information_section_size`.  Used by PvZ2 RSM init (arg3=1) for allocation.
    pub information_section_size: u32,
    /// Size of the resource path (file list) section.
    /// Twinning: `resource_path_section_size`.
    pub resource_path_section_size: u32,
    /// Byte offset to the resource path (file list) section.
    /// Twinning: `resource_path_section_offset`.
    pub resource_path_section_offset: u32,
    pub rsg_list_length: u32,
    pub rsg_list_begin_offset: u32,
    pub rsg_number: u32,
    pub rsg_info_begin_offset: u32,
    pub rsg_info_each_length: u32, // 204
    pub composite_number: u32,
    pub composite_info_begin_offset: u32,
    pub composite_info_each_length: u32, // 1156
    pub composite_list_length: u32,
    pub composite_list_begin_offset: u32,
    pub autopool_number: u32,
    pub autopool_info_begin_offset: u32,
    pub autopool_info_each_length: u32, // 152
    pub ptx_number: u32,
    pub ptx_info_begin_offset: u32,
    pub ptx_info_each_length: u32,
    pub part1_begin_offset: u32,
    pub part2_begin_offset: u32,
    pub part3_begin_offset: u32,
    /// v4+: size of the information section excluding manifest data.
    /// Equal to `information_section_size` when manifests are empty.
    /// PvZ2 uses this when loading individual resources (arg3=0) to save memory.
    /// Twinning: `information_without_manifest_section_size`
    #[serde(default)]
    pub information_without_manifest_section_size: u32,
}

impl RsbHeader {
    /// Returns the metadata section size for the loading context.
    /// Mirrors PvZ2's `sub_1024b8fb8` arg3 logic:
    /// - `include_manifest=true` → `information_section_size` (offset 0x0C)
    /// - `include_manifest=false` && v4+ → `information_without_manifest_section_size` (offset 0x6C)
    /// - `include_manifest=false` && v1-3 → `information_section_size` (offset 0x0C)
    pub fn metadata_size(&self, include_manifest: bool) -> u32 {
        if !include_manifest && self.version >= 4 {
            self.information_without_manifest_section_size
        } else {
            self.information_section_size
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileListInfo {
    pub name_path: String,
    pub pool_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsgInfo {
    pub name: String,
    pub rsg_offset: u32,
    pub rsg_length: u32,
    pub pool_index: i32,
    pub ptx_number: u32,
    pub ptx_before_number: u32,
    pub packet_head_info: Option<Vec<u8>>, // 32 bytes usually
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeInfo {
    pub name: String,
    pub is_composite: bool,
    pub packet_number: u32,
    pub packet_info: Vec<CompositePacketInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositePacketInfo {
    pub packet_index: i32,
    pub category: [String; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoPoolInfo {
    pub name: String,
    pub part0_size: u32,
    pub part1_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RsbPtxInfo {
    pub ptx_index: i32,
    pub width: i32,
    pub height: i32,
    pub pitch: i32,
    pub format: i32,
    /// PvZ2 China `additional_byte_count` compatibility field.
    pub alpha_size: Option<i32>,
    /// PvZ2 China texture `scale` compatibility field.
    pub alpha_format: Option<i32>,
}

// Structs for description.json serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesDescription {
    pub groups: HashMap<String, DescriptionGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionGroup {
    pub composite: bool,
    pub subgroups: HashMap<String, DescriptionSubGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionSubGroup {
    pub res: String,
    pub language: String,
    pub resources: HashMap<String, DescriptionResources>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescriptionResources {
    #[serde(rename = "type")]
    pub res_type: i32,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ptx_info: Option<PropertiesPtxInfo>,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertiesPtxInfo {
    pub imagetype: String,
    pub aflags: String,
    pub x: String,
    pub y: String,
    pub ax: String,
    pub ay: String,
    pub aw: String,
    pub ah: String,
    pub rows: String,
    pub cols: String,
    pub parent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsbManifest {
    pub version: u32,
    pub ptx_info_size: u32,
    pub path: RsbPathInfo,
    pub group: Vec<ManifestGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsbPathInfo {
    pub rsgs: Vec<String>,
    pub packet_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestGroup {
    pub name: String,
    pub is_composite: bool,
    pub subgroup: Vec<ManifestSubgroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestSubgroup {
    pub name_packet: String,
    pub category: [String; 2],
    pub packet_info: ManifestPacketInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPacketInfo {
    pub version: u32,
    pub compression_flags: u32,
    pub res: Vec<ManifestRes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestRes {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part1_info: Option<Part1Extra>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ptx_info: Option<RsbPtxInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ptx_property: Option<ManifestPtxProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPtxProperty {
    pub format: i32,
    pub pitch: i32,
    /// PvZ2 China `additional_byte_count` compatibility field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_size: Option<i32>,
    /// PvZ2 China texture `scale` compatibility field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha_format: Option<i32>,
}
