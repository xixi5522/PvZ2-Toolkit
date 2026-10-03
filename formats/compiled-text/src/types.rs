use smf_container::SmfVariant;

/// Metadata available after decrypting the outer Compiled Text layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledTextMetadata {
    pub variant: SmfVariant,
    pub encoded_size: u64,
    pub ciphertext_size: u64,
    pub container_size: u64,
    pub decoded_size: u64,
}

/// A decoded payload together with its container metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedCompiledText {
    pub data: Vec<u8>,
    pub metadata: CompiledTextMetadata,
}
