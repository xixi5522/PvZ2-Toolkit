use super::CodebookLibrary;

/// How Wwise audio packets should be interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PacketFormat {
    /// Derive the packet format from the `vorb` chunk.
    #[default]
    Auto,
    /// Interpret packets as Wwise's compact modified representation.
    Modified,
    /// Interpret packets as ordinary Vorbis audio packets.
    Standard,
}

/// How the Wwise setup packet should be interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SetupFormat {
    /// Detect a complete Vorbis setup packet by its first codebook sync pattern.
    #[default]
    Auto,
    /// Rebuild Wwise's stripped setup packet.
    Stripped,
    /// Copy a complete setup packet without rebuilding it.
    Full,
}

/// Options used while decoding a Vorbis WEM stream.
#[derive(Clone)]
pub struct VorbisOptions {
    pub(crate) codebooks: Option<CodebookLibrary>,
    pub(crate) inline_codebooks: bool,
    pub(crate) packet_format: PacketFormat,
    pub(crate) setup_format: SetupFormat,
}

impl Default for VorbisOptions {
    fn default() -> Self {
        Self {
            codebooks: Some(CodebookLibrary::standard()),
            inline_codebooks: false,
            packet_format: PacketFormat::Auto,
            setup_format: SetupFormat::Auto,
        }
    }
}

impl VorbisOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn without_codebooks(mut self) -> Self {
        self.codebooks = None;
        self
    }

    pub fn with_codebooks(mut self, codebooks: CodebookLibrary) -> Self {
        self.codebooks = Some(codebooks);
        self
    }

    pub fn with_inline_codebooks(mut self, value: bool) -> Self {
        self.inline_codebooks = value;
        self
    }

    pub fn with_packet_format(mut self, value: PacketFormat) -> Self {
        self.packet_format = value;
        self
    }

    pub fn with_setup_format(mut self, value: SetupFormat) -> Self {
        self.setup_format = value;
        self
    }
}
