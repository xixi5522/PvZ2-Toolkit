const MINIMUM_MATCH: usize = 32;

/// Layout used for newly encoded delta windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VcdiffFormat {
    /// RFC 3284's three separate data, instruction, and address sections.
    Standard,
    /// open-vcdiff's `S` extension with all three streams interleaved.
    #[default]
    Interleaved,
}

/// VCDIFF encoder configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    /// On-disk section layout used by the encoder.
    pub format: VcdiffFormat,
    /// Smallest source or target match represented by a COPY instruction.
    pub minimum_match: usize,
}

impl EncodeOptions {
    /// RFC 3284 sectioned output with the default match threshold.
    pub const fn standard() -> Self {
        Self {
            format: VcdiffFormat::Standard,
            minimum_match: MINIMUM_MATCH,
        }
    }

    /// Twinning-compatible interleaved output with the default match threshold.
    pub const fn interleaved() -> Self {
        Self {
            format: VcdiffFormat::Interleaved,
            minimum_match: MINIMUM_MATCH,
        }
    }
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self::interleaved()
    }
}

/// Resource limit applied while decoding an untrusted delta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Maximum total number of decoded target bytes.
    pub max_output_size: usize,
}

impl DecodeOptions {
    /// Construct a decoder output limit.
    pub const fn new(max_output_size: usize) -> Self {
        Self { max_output_size }
    }
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self::new(0x7FFF_FFFF)
    }
}
