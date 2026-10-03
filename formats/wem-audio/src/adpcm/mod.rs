#[cfg_attr(not(feature = "transcode"), allow(dead_code))]
mod decoder;
mod encoder;

#[cfg(feature = "transcode")]
pub(crate) use decoder::{AdpcmParams, process_adpcm};
pub use encoder::AdpcmWemEncoder;
