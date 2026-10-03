use crate::vcdiff;

/// Whether packet deltas operate on stored RSG bytes or their uncompressed
/// resource sections. The mode is external to the RSBP format and must match
/// when creating and applying a patch, just like Twinning's `use_raw_packet`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PacketPatchMode {
    /// Diff complete on-disk RSG packet bytes.
    #[default]
    Stored,
    /// Diff uncompressed RSG resource sections and recompress on apply.
    Raw,
}

/// Controls complete RSB patch creation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveEncodeOptions {
    /// Packet representation used for VCDIFF creation.
    pub packet_mode: PacketPatchMode,
    /// VCDIFF encoder configuration.
    pub vcdiff: vcdiff::EncodeOptions,
}

impl ArchiveEncodeOptions {
    /// Twinning-compatible stored-packet mode.
    pub const fn stored() -> Self {
        Self {
            packet_mode: PacketPatchMode::Stored,
            vcdiff: vcdiff::EncodeOptions::interleaved(),
        }
    }

    /// Twinning-compatible raw-packet mode.
    pub const fn raw() -> Self {
        Self {
            packet_mode: PacketPatchMode::Raw,
            vcdiff: vcdiff::EncodeOptions::interleaved(),
        }
    }
}

/// Controls complete RSB patch application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArchiveDecodeOptions {
    /// Packet representation expected by this patch.
    pub packet_mode: PacketPatchMode,
    /// VCDIFF decoder resource limits.
    pub vcdiff: vcdiff::DecodeOptions,
}

impl ArchiveDecodeOptions {
    /// Twinning-compatible stored-packet mode.
    pub const fn stored() -> Self {
        Self {
            packet_mode: PacketPatchMode::Stored,
            vcdiff: vcdiff::DecodeOptions::new(0x7FFF_FFFF),
        }
    }

    /// Twinning-compatible raw-packet mode.
    pub const fn raw() -> Self {
        Self {
            packet_mode: PacketPatchMode::Raw,
            vcdiff: vcdiff::DecodeOptions::new(0x7FFF_FFFF),
        }
    }
}
