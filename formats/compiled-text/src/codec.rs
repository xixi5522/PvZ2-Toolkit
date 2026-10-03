use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use smf_container::SmfMetadata;

use crate::{
    CompiledTextError, CompiledTextMetadata, DecodeOptions, DecodedCompiledText, EncodeOptions,
    Result, crypto,
};

/// Decode a Compiled Text file using safe defaults.
pub fn decode(data: &[u8], seed: &str) -> Result<Vec<u8>> {
    decode_with_options(data, seed, DecodeOptions::default())
}

/// Decode a Compiled Text file with explicit safety controls.
pub fn decode_with_options(data: &[u8], seed: &str, options: DecodeOptions) -> Result<Vec<u8>> {
    Ok(decode_detailed(data, seed, options)?.data)
}

/// Decode a Compiled Text file and return its container metadata.
pub fn decode_detailed(
    data: &[u8],
    seed: &str,
    options: DecodeOptions,
) -> Result<DecodedCompiledText> {
    let prepared = prepare(data, seed, options)?;
    let decoded = smf_container::decode(Cursor::new(&prepared.container))?;
    Ok(DecodedCompiledText {
        data: decoded,
        metadata: prepared.metadata,
    })
}

/// Decrypt and inspect a Compiled Text file without inflating its payload.
pub fn inspect(data: &[u8], seed: &str, options: DecodeOptions) -> Result<CompiledTextMetadata> {
    Ok(prepare(data, seed, options)?.metadata)
}

/// Encode a payload using the compact 32-bit header and compression level 9.
pub fn encode(data: &[u8], seed: &str) -> Result<Vec<u8>> {
    encode_with_options(data, seed, EncodeOptions::default())
}

/// Encode a payload into a Base64 Compiled Text file.
pub fn encode_with_options(data: &[u8], seed: &str, options: EncodeOptions) -> Result<Vec<u8>> {
    let container = smf_container::encode(
        data,
        smf_container::EncodeOptions::new(options.variant, options.compression_level),
    )?;
    let encrypted = crypto::encrypt(&container, seed)?;
    Ok(STANDARD.encode(encrypted).into_bytes())
}

struct PreparedContainer {
    container: Vec<u8>,
    metadata: CompiledTextMetadata,
}

fn prepare(data: &[u8], seed: &str, options: DecodeOptions) -> Result<PreparedContainer> {
    let ciphertext = decode_base64(data, options.allow_base64_whitespace)?;
    let container = crypto::decrypt(&ciphertext, seed)?;
    let smf = smf_container::inspect(&mut Cursor::new(&container))?;
    enforce_output_limit(smf, options.max_output_size)?;
    Ok(PreparedContainer {
        metadata: metadata(data.len(), ciphertext.len(), container.len(), smf),
        container,
    })
}

fn decode_base64(data: &[u8], allow_whitespace: bool) -> Result<Vec<u8>> {
    if allow_whitespace && data.iter().any(u8::is_ascii_whitespace) {
        let compact = data
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        Ok(STANDARD.decode(compact)?)
    } else {
        Ok(STANDARD.decode(data)?)
    }
}

fn enforce_output_limit(metadata: SmfMetadata, limit: u64) -> Result<()> {
    if metadata.uncompressed_size > limit {
        return Err(CompiledTextError::OutputLimitExceeded {
            declared: metadata.uncompressed_size,
            limit,
        });
    }
    Ok(())
}

fn metadata(
    encoded_size: usize,
    ciphertext_size: usize,
    container_size: usize,
    smf: SmfMetadata,
) -> CompiledTextMetadata {
    CompiledTextMetadata {
        variant: smf.variant,
        encoded_size: encoded_size as u64,
        ciphertext_size: ciphertext_size as u64,
        container_size: container_size as u64,
        decoded_size: smf.uncompressed_size,
    }
}
