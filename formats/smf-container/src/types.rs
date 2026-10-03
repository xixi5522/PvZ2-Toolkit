/// Header layout used by an SMF container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SmfVariant {
    /// 32-bit magic and 32-bit uncompressed size (8-byte header).
    #[default]
    Compact32,
    /// Zero-extended 64-bit magic and 64-bit uncompressed size (16-byte header).
    Extended64,
}

impl SmfVariant {
    pub const fn header_len(self) -> u64 {
        match self {
            Self::Compact32 => 8,
            Self::Extended64 => 16,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Compact32 => "32-bit / 8-byte",
            Self::Extended64 => "64-bit / 16-byte",
        }
    }
}

/// Metadata available without retaining the decoded payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmfMetadata {
    pub variant: SmfVariant,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub total_size: u64,
}

impl SmfMetadata {
    pub fn compression_ratio(self) -> f64 {
        if self.uncompressed_size == 0 {
            0.0
        } else {
            self.compressed_size as f64 / self.uncompressed_size as f64
        }
    }
}

/// Options used when producing a new SMF container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncodeOptions {
    pub variant: SmfVariant,
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
