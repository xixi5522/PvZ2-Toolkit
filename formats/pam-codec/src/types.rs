use serde::{Deserialize, Serialize};

pub const PAM_MAGIC: u32 = 0xBAF01954;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PamInfo {
    pub version: i32,
    pub frame_rate: i32,
    pub position: [f64; 2],
    pub size: [f64; 2],
    pub image: Vec<ImageInfo>,
    pub sprite: Vec<SpriteInfo>,
    #[serde(default)]
    pub main_sprite: Option<SpriteInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub name: String,
    #[serde(default)]
    pub size: Option<[i32; 2]>,
    pub transform: Vec<f64>, // Using Vec because length varies
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub frame_rate: Option<f64>,
    #[serde(default)]
    pub work_area: Option<[i32; 2]>,
    pub frame: Vec<FrameInfo>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameInfo {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub stop: bool,
    #[serde(default)]
    pub command: Vec<[String; 2]>,
    #[serde(default)]
    pub remove: Vec<RemovesInfo>,
    #[serde(default)]
    pub append: Vec<AddsInfo>,
    #[serde(default)]
    pub change: Vec<MovesInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemovesInfo {
    pub index: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddsInfo {
    pub index: i32,
    #[serde(default)]
    pub name: Option<String>,
    /// Image or sprite resource index (wire format: u8, extended to u16 in v6+).
    pub resource: u32,
    #[serde(default)]
    pub sprite: bool,
    #[serde(default)]
    pub additive: bool,
    /// Preload frame number (wire format: signed i16 when present).
    #[serde(default)]
    pub preload_frame: i32,
    #[serde(default = "default_time_scale")]
    pub time_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rectangle {
    pub position: [f64; 2],
    pub size: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovesInfo {
    pub index: i32,
    pub transform: Vec<f64>,
    #[serde(default)]
    pub color: Option<[f64; 4]>,
    #[serde(default)]
    pub source_rectangle: Option<Rectangle>,
    /// Sprite frame number (wire format: signed i16 when present).
    #[serde(default)]
    pub sprite_frame_number: Option<i32>,
}

fn default_time_scale() -> f32 {
    1.0
}

bitflags::bitflags! {
    pub struct FrameFlags: u8 {
        const REMOVES = 1;
        const ADDS = 2;
        const MOVES = 4;
        const FRAME_NAME = 8;
        const STOP = 16;
        const COMMANDS = 32;
    }
}

bitflags::bitflags! {
    pub struct MoveFlags: u16 {
        const SRC_RECT = 32768;
        const ROTATE = 16384;
        const COLOR = 8192;
        const MATRIX = 4096;
        const LONG_COORDS = 2048;
        const ANIM_FRAME_NUM = 1024;
    }
}
