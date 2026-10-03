#[cfg(feature = "decode")]
#[derive(Debug, Clone)]
pub struct ReadLimits {
    pub max_entries: usize,
    pub max_chunks: usize,
    pub max_chunk_references: usize,
    pub max_string_length: usize,
    pub max_metadata_bytes: usize,
    pub max_chunk_input_size: usize,
    pub max_chunk_output_size: usize,
    pub max_codec_workspace: usize,
    pub max_entry_size: u64,
    pub max_total_output: u64,
}

#[cfg(feature = "decode")]
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            max_entries: u16::MAX as usize,
            max_chunks: u16::MAX as usize,
            max_chunk_references: 1_000_000,
            max_string_length: 1024 * 1024,
            max_metadata_bytes: 64 * 1024 * 1024,
            max_chunk_input_size: u32::MAX as usize,
            max_chunk_output_size: u32::MAX as usize,
            max_codec_workspace: 1024 * 1024 * 1024,
            max_entry_size: 16 * 1024 * 1024 * 1024,
            max_total_output: 256 * 1024 * 1024 * 1024,
        }
    }
}

#[cfg(feature = "decode")]
impl ReadLimits {
    pub const fn unlimited() -> Self {
        Self {
            max_entries: usize::MAX,
            max_chunks: usize::MAX,
            max_chunk_references: usize::MAX,
            max_string_length: usize::MAX - 1,
            max_metadata_bytes: usize::MAX,
            max_chunk_input_size: usize::MAX,
            max_chunk_output_size: usize::MAX,
            max_codec_workspace: usize::MAX,
            max_entry_size: u64::MAX,
            max_total_output: u64::MAX,
        }
    }
}

#[cfg(feature = "decode")]
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    pub limits: ReadLimits,
}

#[cfg(feature = "encode")]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PackOptions {
    pub volume_names: Vec<String>,
    pub alignment: u32,
    /// Archive-wide native DZ and COMBUF settings.
    pub dz: crate::codec::DzOptions,
}

#[cfg(feature = "encode")]
impl Default for PackOptions {
    fn default() -> Self {
        Self {
            volume_names: vec!["archive.dz".to_string()],
            alignment: 0,
            dz: crate::codec::DzOptions::default(),
        }
    }
}

#[cfg(feature = "encode")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EntryOptions {
    pub compression: crate::codec::Compression,
    pub volume: u16,
    pub random_access: bool,
    pub content_hint: Option<crate::codec::ContentHint>,
    /// Exact on-disk flags to write for this entry.
    ///
    /// This is primarily intended for compatibility frontends that need to
    /// preserve dzip.exe's combined `.dcl` flags. When present, these flags
    /// also select the encoder using the original packer registration order;
    /// [`Self::compression`] is ignored.
    pub raw_flags: Option<u16>,
}

#[cfg(feature = "encode")]
impl Default for EntryOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "encode")]
impl EntryOptions {
    pub const fn new() -> Self {
        Self {
            compression: crate::codec::Compression::Dz,
            volume: 0,
            random_access: false,
            content_hint: None,
            raw_flags: None,
        }
    }

    pub const fn compression(mut self, compression: crate::codec::Compression) -> Self {
        self.compression = compression;
        self
    }

    pub const fn volume(mut self, volume: u16) -> Self {
        self.volume = volume;
        self
    }

    pub const fn random_access(mut self, enabled: bool) -> Self {
        self.random_access = enabled;
        self
    }

    pub const fn content_hint(mut self, hint: crate::codec::ContentHint) -> Self {
        self.content_hint = Some(hint);
        self
    }

    /// Override the exact flags stored in the chunk table.
    pub const fn raw_flags(mut self, flags: u16) -> Self {
        self.raw_flags = Some(flags);
        self
    }
}
