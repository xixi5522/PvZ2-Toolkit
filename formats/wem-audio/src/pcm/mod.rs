#[cfg_attr(not(feature = "transcode"), allow(dead_code))]
mod decoder;
mod encoder;

#[cfg(feature = "transcode")]
pub(crate) use decoder::{PcmParams, process_pcm};
pub use encoder::PcmWemEncoder;
