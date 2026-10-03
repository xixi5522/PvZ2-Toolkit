use smf_container::SmfVariant;

/// Safety controls used while decoding untrusted Compiled Text input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Maximum accepted uncompressed payload size.
    pub max_output_size: u64,
    /// Whether ASCII whitespace may appear inside the Base64 input.
    pub allow_base64_whitespace: bool,
}

impl DecodeOptions {
    /// Default limit of 512 MiB with whitespace-tolerant Base64 input.
    pub const fn safe() -> Self {
        Self {
            max_output_size: 512 * 1024 * 1024,
            allow_base64_whitespace: true,
        }
    }

    /// Disable the decoded-size limit. Use only for trusted input.
    pub const fn unlimited() -> Self {
        Self {
            max_output_size: u64::MAX,
            allow_base64_whitespace: true,
        }
    }
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self::safe()
    }
}

/// Options used to create a Compiled Text file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    /// SMF header layout stored inside the encrypted payload.
    pub variant: SmfVariant,
    /// Zlib compression level in the inclusive range `0..=9`.
    pub compression_level: u32,
}

impl EncodeOptions {
    pub const fn new(variant: SmfVariant, compression_level: u32) -> Self {
        Self {
            variant,
            compression_level,
        }
    }

    pub const fn compact() -> Self {
        Self::new(SmfVariant::Compact32, 9)
    }

    pub const fn extended() -> Self {
        Self::new(SmfVariant::Extended64, 9)
    }
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self::compact()
    }
}
