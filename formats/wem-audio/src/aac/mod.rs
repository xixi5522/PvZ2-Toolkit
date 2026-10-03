mod decoder;
mod encoder;

#[cfg(feature = "transcode")]
pub(crate) use decoder::decode_aac_to_wav;
pub use decoder::extract_wem_aac;
#[cfg(not(target_arch = "wasm32"))]
pub use encoder::probe_aac_file;
pub use encoder::{AacMetadata, AacWemEncoder, probe_aac_bytes, probe_aac_metadata};
