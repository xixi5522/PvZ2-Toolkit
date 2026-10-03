use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reanim {
    pub do_scale: Option<i8>,
    pub fps: f32,
    pub tracks: Vec<ReanimTrack>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReanimTrack {
    pub name: String,
    pub transforms: Vec<ReanimTransform>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReanimTransform {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub kx: Option<f32>,
    pub ky: Option<f32>,
    pub sx: Option<f32>,
    pub sy: Option<f32>,
    pub f: Option<f32>,
    pub a: Option<f32>,
    /// i can be a String or an Int (in C# it is an `object?` with type checking).
    /// To serialize nicely in Rust, we can make it an enum or a String. Let's make it a string,
    /// and if it's an int we serialize the int to string.
    pub i: Option<String>,
    pub resource: Option<String>,
    pub i2: Option<String>,
    pub resource2: Option<String>,
    pub font: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReanimVersion {
    #[default]
    PC = 1,
    Phone32 = 2,
    Phone64 = 3,
}
