//! Safe filesystem extraction for indexed Dzip archives.

use crate::archive::Archive;
use crate::reader::VolumeSource;
use crate::{DzipError, Result};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct ExtractOptions {
    pub overwrite: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self { overwrite: true }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExtractionReport {
    pub files: usize,
    pub bytes: u64,
}

impl<R: Read + Seek, V: VolumeSource> Archive<R, V> {
    pub fn extract_to(
        &mut self,
        output_dir: impl AsRef<Path>,
        options: ExtractOptions,
    ) -> Result<ExtractionReport> {
        let output_dir = output_dir.as_ref();
        std::fs::create_dir_all(output_dir)?;
        let root = std::fs::canonicalize(output_dir)?;
        let entries: Vec<_> = self.entries().iter().map(|entry| entry.id()).collect();
        let mut report = ExtractionReport::default();

        for id in entries {
            let relative = self
                .entry(id)
                .expect("entry IDs originate from this archive")
                .path()
                .to_path_buf();
            let target = prepare_safe_target(&root, &relative, options.overwrite)?;
            let mut file = if options.overwrite {
                File::create(&target)?
            } else {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?
            };
            let bytes = self.read_entry_to(id, &mut file)?;
            report.files += 1;
            report.bytes = report
                .bytes
                .checked_add(bytes)
                .ok_or_else(|| invalid_archive("extraction byte count overflow"))?;
        }
        Ok(report)
    }
}

fn prepare_safe_target(root: &Path, relative: &Path, overwrite: bool) -> Result<PathBuf> {
    let relative = crate::path::sanitize_path(relative)?;
    let components: Vec<_> = relative.components().collect();
    if components.is_empty() {
        return Err(invalid_archive("archive entry has an empty path"));
    }

    let mut current = root.to_path_buf();
    for component in &components[..components.len() - 1] {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(invalid_archive(format!(
                        "unsafe extraction parent {}",
                        current.display()
                    )));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }

    let target = root.join(&relative);
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() || metadata.is_dir() {
            return Err(invalid_archive(format!(
                "unsafe extraction target {}",
                target.display()
            )));
        }
        if !overwrite {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("extraction target already exists: {}", target.display()),
            )
            .into());
        }
    }
    Ok(target)
}

fn invalid_archive(message: impl Into<String>) -> DzipError {
    DzipError::InvalidArchive(message.into())
}
