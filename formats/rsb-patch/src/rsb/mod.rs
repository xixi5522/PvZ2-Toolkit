mod apply;
mod create;
mod metadata;
mod options;
mod raw_packet;
mod view;

pub use apply::apply_archive_patch;
pub use create::create_archive_patch;
pub use options::{ArchiveDecodeOptions, ArchiveEncodeOptions, PacketPatchMode};

use crate::PatchError;

pub(super) fn invalid(message: impl Into<String>) -> PatchError {
    PatchError::InvalidArchive(message.into())
}
