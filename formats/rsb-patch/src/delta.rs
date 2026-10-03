#[cfg(feature = "rsb")]
use crate::md5_hash;
use crate::{Result, vcdiff, verify_hash};

#[cfg(feature = "rsb")]
pub(crate) fn create(
    before: &[u8],
    after: &[u8],
    options: vcdiff::EncodeOptions,
) -> Result<([u8; 16], Option<Vec<u8>>)> {
    let patch = (before != after)
        .then(|| vcdiff::encode_with_options(before, after, options))
        .transpose()?;
    Ok((md5_hash(before), patch))
}

pub(crate) fn apply(
    context: impl Into<String>,
    before: &[u8],
    before_hash: [u8; 16],
    patch: Option<&[u8]>,
    options: vcdiff::DecodeOptions,
) -> Result<Vec<u8>> {
    verify_hash(context, before, before_hash)?;
    match patch {
        Some(patch) => vcdiff::decode_with_options(before, patch, options),
        None => Ok(before.to_vec()),
    }
}
