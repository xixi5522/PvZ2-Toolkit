//! Lossless, undecoded Dzip archive images.
//!
//! [`ArchiveImage`] retains every volume byte-for-byte. It is useful for
//! inspection, transport, and exact rewrites where decoding and rebuilding an
//! archive would be both unnecessary and lossy.

use crate::format::RawArchive;
use crate::options::ReadOptions;
use crate::path::resolve_relative_path;
use crate::{DzipError, Result};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveImage {
    metadata: RawArchive,
    volumes: Vec<Vec<u8>>,
}

impl ArchiveImage {
    /// Parse an archive from volume bytes ordered by numeric volume ID.
    /// Volume zero must be the main `.dz` file.
    pub fn from_volumes(volumes: Vec<Vec<u8>>) -> Result<Self> {
        Self::from_volumes_with_options(volumes, ReadOptions::default())
    }

    pub fn from_volumes_with_options(volumes: Vec<Vec<u8>>, options: ReadOptions) -> Result<Self> {
        let main = volumes
            .first()
            .ok_or_else(|| invalid_archive("archive image has no main volume"))?;
        let metadata = RawArchive::read_from_with_options(Cursor::new(main.as_slice()), options)?;
        validate_volume_count(&metadata, volumes.len())?;
        Ok(Self { metadata, volumes })
    }

    /// Load the main archive and every named auxiliary volume from disk.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_path_with_options(path, ReadOptions::default())
    }

    pub fn open_path_with_options(path: impl AsRef<Path>, options: ReadOptions) -> Result<Self> {
        let path = path.as_ref();
        let main = std::fs::read(path)?;
        let metadata = RawArchive::read_from_with_options(Cursor::new(main.as_slice()), options)?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut volumes =
            Vec::with_capacity(usize::from(metadata.chunk_settings.num_archive_files));
        volumes.push(main);
        for name in &metadata.volume_files {
            let relative = resolve_relative_path(name.as_str()?)?;
            volumes.push(std::fs::read(base_dir.join(relative))?);
        }
        validate_volume_count(&metadata, volumes.len())?;
        Ok(Self { metadata, volumes })
    }

    pub const fn metadata(&self) -> &RawArchive {
        &self.metadata
    }

    pub fn volume(&self, id: u16) -> Option<&[u8]> {
        self.volumes.get(usize::from(id)).map(Vec::as_slice)
    }

    pub fn volumes(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.volumes.iter().map(Vec::as_slice)
    }

    pub fn into_volumes(self) -> Vec<Vec<u8>> {
        self.volumes
    }

    /// Write every retained volume without changing any byte.
    ///
    /// The supplied path names volume zero. Auxiliary names come from the
    /// archive's lossless metadata, matching the original lookup behavior.
    pub fn write_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        // Resolve every stored name before creating anything so an invalid
        // auxiliary path cannot leave a partially rewritten archive behind.
        let auxiliary_paths = self
            .metadata
            .volume_files
            .iter()
            .map(|name| Ok(base_dir.join(resolve_relative_path(name.as_str()?)?)))
            .collect::<Result<Vec<_>>>()?;

        if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &self.volumes[0])?;
        for (output, bytes) in auxiliary_paths.iter().zip(&self.volumes[1..]) {
            if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output, bytes)?;
        }
        Ok(())
    }
}

fn validate_volume_count(metadata: &RawArchive, actual: usize) -> Result<()> {
    let declared = usize::from(metadata.chunk_settings.num_archive_files);
    let named = metadata
        .volume_files
        .len()
        .checked_add(1)
        .ok_or_else(|| invalid_archive("archive volume count overflow"))?;
    if declared != named {
        return Err(invalid_archive(format!(
            "archive declares {declared} volumes but names {named}"
        )));
    }
    if actual != declared {
        return Err(invalid_archive(format!(
            "archive image contains {actual} volumes but declares {declared}"
        )));
    }
    Ok(())
}

fn invalid_archive(message: impl Into<String>) -> DzipError {
    DzipError::InvalidArchive(message.into())
}
