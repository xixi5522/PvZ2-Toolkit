/// Standard integer PCM format tag.
pub const WAVE_FORMAT_PCM: u16 = 0x0001;
/// WAVEFORMATEXTENSIBLE format tag.
pub const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// Audiokinetic Wwise Vorbis format tag.
pub const WWISE_FORMAT_VORBIS: u16 = 0xFFFF;
/// Audiokinetic Wwise IMA ADPCM format tag.
pub const WWISE_FORMAT_IMA_ADPCM: u16 = 0x8311;
/// Audiokinetic Wwise AAC format tag.
pub const WWISE_FORMAT_AAC: u16 = 0xAAC0;

/// RIFF integer byte order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiffEndian {
    Little,
    Big,
}

impl RiffEndian {
    pub const fn is_little(self) -> bool {
        matches!(self, Self::Little)
    }

    pub(crate) const fn read_u16(self, bytes: [u8; 2]) -> u16 {
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }

    pub(crate) const fn read_u32(self, bytes: [u8; 4]) -> u32 {
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }
}

/// Audio payload stored by a WEM container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WemCodec {
    Pcm,
    ExtensiblePcm,
    Vorbis,
    ImaAdpcm,
    Aac,
    Unknown(u16),
}

impl WemCodec {
    pub const fn from_format_tag(format_tag: u16) -> Self {
        match format_tag {
            WAVE_FORMAT_PCM => Self::Pcm,
            WAVE_FORMAT_EXTENSIBLE => Self::ExtensiblePcm,
            WWISE_FORMAT_VORBIS => Self::Vorbis,
            WWISE_FORMAT_IMA_ADPCM => Self::ImaAdpcm,
            WWISE_FORMAT_AAC => Self::Aac,
            value => Self::Unknown(value),
        }
    }

    pub const fn format_tag(self) -> u16 {
        match self {
            Self::Pcm => WAVE_FORMAT_PCM,
            Self::ExtensiblePcm => WAVE_FORMAT_EXTENSIBLE,
            Self::Vorbis => WWISE_FORMAT_VORBIS,
            Self::ImaAdpcm => WWISE_FORMAT_IMA_ADPCM,
            Self::Aac => WWISE_FORMAT_AAC,
            Self::Unknown(value) => value,
        }
    }
}

/// Validated location of a RIFF chunk payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WemChunk {
    pub(crate) id: [u8; 4],
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) declared_size: u32,
}

impl WemChunk {
    pub(crate) const fn new(id: [u8; 4], offset: u64, size: u64, declared_size: u32) -> Self {
        Self {
            id,
            offset,
            size,
            declared_size,
        }
    }

    pub const fn id(self) -> [u8; 4] {
        self.id
    }

    pub const fn offset(self) -> u64 {
        self.offset
    }

    /// Payload bytes actually available in the source.
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Payload size declared by the RIFF chunk header.
    pub const fn declared_size(self) -> u32 {
        self.declared_size
    }

    pub const fn is_truncated(self) -> bool {
        self.size < self.declared_size as u64
    }
}

/// WEM chunks understood by this crate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WemChunks {
    pub fmt: Option<WemChunk>,
    pub data: Option<WemChunk>,
    pub vorb: Option<WemChunk>,
    pub cue: Option<WemChunk>,
    pub list: Option<WemChunk>,
    pub smpl: Option<WemChunk>,
}

/// Common, validated WEM metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WemMetadata {
    pub endian: RiffEndian,
    pub codec: WemCodec,
    pub channels: u16,
    pub sample_rate: u32,
    pub average_bytes_per_second: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub data_offset: u64,
    /// Number of payload bytes actually available in the source.
    pub data_size: u64,
    /// Size declared by the RIFF data chunk.
    pub declared_data_size: u32,
    /// Complete source length.
    pub file_size: u64,
    /// Complete RIFF size declared by the container, including the 8-byte header.
    pub declared_riff_size: u64,
}

impl WemMetadata {
    pub const fn is_prefetch(&self) -> bool {
        self.data_size < self.declared_data_size as u64 || self.file_size < self.declared_riff_size
    }
}
