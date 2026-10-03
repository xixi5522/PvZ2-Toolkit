use crate::path::resolve_relative_path;
use crate::{DzipError, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Seek, Write};
use std::path::PathBuf;

pub trait WriteSeek: Write + Seek {}
impl<T: Write + Seek> WriteSeek for T {}

pub trait VolumeSink {
    /// Begin a new archive transaction. Implementations that retain handles
    /// should discard previous volume state here.
    fn begin_archive(&mut self) -> Result<()> {
        Ok(())
    }

    fn open_volume(&mut self, id: u16, name: &str) -> Result<&mut dyn WriteSeek>;
}

pub struct FileSystemVolumeSink {
    output_dir: PathBuf,
    files: HashMap<u16, File>,
}

impl FileSystemVolumeSink {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            files: HashMap::new(),
        }
    }
}

impl VolumeSink for FileSystemVolumeSink {
    fn begin_archive(&mut self) -> Result<()> {
        self.files.clear();
        Ok(())
    }

    fn open_volume(&mut self, id: u16, name: &str) -> Result<&mut dyn WriteSeek> {
        if !self.files.contains_key(&id) {
            let relative = resolve_relative_path(name)?;
            let path = self.output_dir.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = File::create(path)?;
            self.files.insert(id, file);
        }
        Ok(self
            .files
            .get_mut(&id)
            .expect("volume was inserted before lookup"))
    }
}

#[derive(Default)]
pub struct MemoryVolumeSink {
    volumes: HashMap<u16, Cursor<Vec<u8>>>,
    names: HashMap<u16, String>,
}

impl MemoryVolumeSink {
    pub fn volume(&self, id: u16) -> Option<&[u8]> {
        self.volumes
            .get(&id)
            .map(|writer| writer.get_ref().as_slice())
    }

    pub fn name(&self, id: u16) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    pub fn into_volumes(self) -> HashMap<u16, Vec<u8>> {
        self.volumes
            .into_iter()
            .map(|(id, writer)| (id, writer.into_inner()))
            .collect()
    }
}

impl VolumeSink for MemoryVolumeSink {
    fn begin_archive(&mut self) -> Result<()> {
        self.volumes.clear();
        self.names.clear();
        Ok(())
    }

    fn open_volume(&mut self, id: u16, name: &str) -> Result<&mut dyn WriteSeek> {
        self.names.entry(id).or_insert_with(|| name.to_string());
        Ok(self.volumes.entry(id).or_default())
    }
}

pub(super) struct SingleVolumeSink<'a, W> {
    pub(super) writer: &'a mut W,
}

impl<W: Write + Seek> VolumeSink for SingleVolumeSink<'_, W> {
    fn open_volume(&mut self, id: u16, _name: &str) -> Result<&mut dyn WriteSeek> {
        if id != 0 {
            return Err(DzipError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "single-writer output cannot contain auxiliary volumes",
            )));
        }
        Ok(self.writer)
    }
}
