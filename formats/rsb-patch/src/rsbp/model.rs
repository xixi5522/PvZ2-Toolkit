use std::io::{Read, Write};

use crate::{Result, delta, vcdiff};

use super::{ContainerDecodeOptions, io};

/// Complete PopCap RSBPatch container.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RsbPatch {
    /// Size of the reconstructed RSB region represented by the patch.
    pub all_after_size: u32,
    /// MD5 of the original RSB information section.
    pub before_hash: [u8; 16],
    /// Optional VCDIFF for the RSB information section.
    pub information_patch: Option<Vec<u8>>,
    /// Packet records in the target RSB order.
    pub packets: Vec<PacketPatch>,
}

impl RsbPatch {
    /// Parse an RSBP container using the default allocation limits.
    pub fn read<R: Read>(reader: R) -> Result<Self> {
        io::read(reader)
    }

    /// Parse an RSBP container with explicit allocation limits.
    pub fn read_with_options<R: Read>(reader: R, options: ContainerDecodeOptions) -> Result<Self> {
        io::read_with_options(reader, options)
    }

    /// Serialize this RSBP container to a writer.
    pub fn write<W: Write>(&self, writer: W) -> Result<()> {
        io::write(writer, self)
    }

    /// Serialize this RSBP container into an owned byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        self.write(&mut output)?;
        Ok(output)
    }

    /// Verify and patch the original RSB information section.
    pub fn apply_information(&self, before: &[u8]) -> Result<Vec<u8>> {
        self.apply_information_with_options(before, vcdiff::DecodeOptions::default())
    }

    /// Verify and patch the original information section using explicit
    /// VCDIFF resource limits.
    pub fn apply_information_with_options(
        &self,
        before: &[u8],
        options: vcdiff::DecodeOptions,
    ) -> Result<Vec<u8>> {
        delta::apply(
            "RSB information section",
            before,
            self.before_hash,
            self.information_patch.as_deref(),
            options,
        )
    }

    #[cfg(feature = "rsb")]
    /// Create a complete patch from two RSB v4 archives.
    pub fn create(
        before: &[u8],
        after: &[u8],
        options: crate::ArchiveEncodeOptions,
    ) -> Result<Self> {
        crate::create_archive_patch(before, after, options)
    }

    #[cfg(feature = "rsb")]
    /// Apply this patch to an RSB v4 archive.
    pub fn apply_to_archive(
        &self,
        before: &[u8],
        options: crate::ArchiveDecodeOptions,
    ) -> Result<Vec<u8>> {
        crate::apply_archive_patch(before, self, options)
    }
}

/// One fixed-size packet descriptor and its optional VCDIFF payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PacketPatch {
    /// Packet name stored in the fixed-width RSBP record.
    pub name: String,
    /// MD5 of the original packet. Newly introduced packets use the MD5 of an
    /// empty source buffer.
    pub before_hash: [u8; 16],
    /// Optional VCDIFF payload; `None` means the packet is unchanged.
    pub patch: Option<Vec<u8>>,
}

impl PacketPatch {
    /// Verify and patch an original packet.
    pub fn apply(&self, before: &[u8]) -> Result<Vec<u8>> {
        self.apply_with_options(before, vcdiff::DecodeOptions::default())
    }

    /// Verify and patch an original packet using explicit VCDIFF resource
    /// limits.
    pub fn apply_with_options(
        &self,
        before: &[u8],
        options: vcdiff::DecodeOptions,
    ) -> Result<Vec<u8>> {
        delta::apply(
            format!("packet {}", self.name),
            before,
            self.before_hash,
            self.patch.as_deref(),
            options,
        )
    }
}
