use crate::error::{DzipError, Result};
use crate::reader::{ReadSeek, VolumeSource};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::path::{Path, PathBuf};

/// A volume manager that reads volumes from the filesystem using a base directory and a file list.
pub struct FileSystemVolumeManager {
    base_dir: PathBuf,
    file_list: Vec<String>,
    open_files: HashMap<u16, File>,
}

impl FileSystemVolumeManager {
    /// Creates a new FileSystemVolumeManager.
    ///
    /// # Arguments
    /// * `base_dir` - The directory containing the volume files.
    /// * `file_list` - List of filenames for auxiliary volumes (Volume 1, Volume 2, ...).
    pub fn new(base_dir: PathBuf, file_list: Vec<String>) -> Self {
        Self {
            base_dir,
            file_list,
            open_files: HashMap::new(),
        }
    }

    pub fn file_list(&self) -> &[String] {
        &self.file_list
    }
}

pub type PathVolumeSource = FileSystemVolumeManager;

/// An in-memory auxiliary-volume provider useful for embedded callers and tests.
pub struct MemoryVolumeSource {
    volumes: HashMap<u16, std::io::Cursor<Vec<u8>>>,
}

impl MemoryVolumeSource {
    pub fn new(volumes: impl IntoIterator<Item = (u16, Vec<u8>)>) -> Self {
        Self {
            volumes: volumes
                .into_iter()
                .map(|(id, bytes)| (id, std::io::Cursor::new(bytes)))
                .collect(),
        }
    }

    pub fn insert(&mut self, id: u16, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.volumes
            .insert(id, std::io::Cursor::new(bytes))
            .map(std::io::Cursor::into_inner)
    }

    pub fn contains(&self, id: u16) -> bool {
        self.volumes.contains_key(&id)
    }

    pub fn available_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.volumes.keys().copied()
    }
}

impl VolumeSource for MemoryVolumeSource {
    fn open_volume(&mut self, id: u16) -> Result<&mut dyn ReadSeek> {
        self.volumes
            .get_mut(&id)
            .map(|reader| reader as &mut dyn ReadSeek)
            .ok_or(DzipError::VolumeNotFound(id))
    }

    fn volume_len(&mut self, id: u16) -> Result<Option<u64>> {
        Ok(self
            .volumes
            .get(&id)
            .map(|reader| reader.get_ref().len() as u64))
    }
}

impl VolumeSource for FileSystemVolumeManager {
    fn open_volume(&mut self, id: u16) -> Result<&mut dyn ReadSeek> {
        // ID 0 is reserved for the main file, which is typically handled by the DzipReader itself
        // before calling into VolumeSource for other chunks. However, if open_volume IS called with 0,
        // it implies the caller expects the manager to handle it.
        // In dzip-archive, VolumeSource is used for "Auxiliary" volumes.
        // The DzipReader usually manages the main reader.
        if id == 0 {
            return Err(DzipError::Io(std::io::Error::other(
                "Volume ID 0 is reserved for main file",
            )));
        }

        let list_index = (id - 1) as usize;
        if list_index >= self.file_list.len() {
            return Err(DzipError::VolumeNotFound(id));
        }

        match self.open_files.entry(id) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                let file_name = &self.file_list[list_index];
                let relative = crate::path::resolve_relative_path(file_name).map_err(|error| {
                    DzipError::VolumeOpenError(id, format!("unsafe volume path: {error}"))
                })?;
                let file = open_windows_compatible(&self.base_dir, &relative)
                    .map_err(|e| DzipError::VolumeOpenError(id, e.to_string()))?;
                Ok(e.insert(file))
            }
        }
    }
}

fn open_windows_compatible(base: &Path, relative: &Path) -> std::io::Result<File> {
    let exact = base.join(relative);
    match File::open(&exact) {
        Ok(file) => return Ok(file),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound || cfg!(windows) => {
            return Err(error);
        }
        Err(_) => {}
    }

    let mut current = base.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(expected) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "auxiliary volume path is not relative",
            ));
        };
        let expected = expected.to_string_lossy();
        let mut matches = std::fs::read_dir(&current)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&expected)
            })
            .map(|entry| entry.path());
        let Some(matching) = matches.next() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "{} was not found",
                    current.join(expected.as_ref()).display()
                ),
            ));
        };
        if matches.next().is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "ambiguous case-insensitive volume component {}",
                    current.join(expected.as_ref()).display()
                ),
            ));
        }
        current = matching;
    }
    File::open(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filesystem_volumes_use_windows_case_comparison_on_every_host() {
        let root = std::env::temp_dir().join(format!("dzip-volume-case-{}", std::process::id()));
        let nested = root.join("assets");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("game.001"), b"volume").unwrap();
        let mut source =
            FileSystemVolumeManager::new(root.clone(), vec!["ASSETS/GAME.001".to_string()]);
        let mut bytes = Vec::new();
        source
            .open_volume(1)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"volume");
        std::fs::remove_dir_all(root).unwrap();
    }
}
