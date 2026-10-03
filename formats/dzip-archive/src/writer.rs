use crate::error::Result;
use crate::format::*;
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::Write;

pub struct DzipWriter<W: Write> {
    writer: W,
}

impl<W: Write> DzipWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write_archive_settings(&mut self, settings: &ArchiveSettings) -> Result<()> {
        self.writer.write_u32::<LittleEndian>(settings.header)?; // Should be 0x5A525444
        self.writer
            .write_u16::<LittleEndian>(settings.num_user_files)?;
        self.writer
            .write_u16::<LittleEndian>(settings.num_directories)?;
        self.writer.write_u8(settings.version)?;
        Ok(())
    }

    pub fn write_strings(&mut self, strings: &[String]) -> Result<()> {
        for s in strings {
            self.writer.write_all(s.as_bytes())?;
            self.writer.write_u8(0)?; // null terminator
        }
        Ok(())
    }

    pub fn write_raw_strings(&mut self, strings: &[ArchiveString]) -> Result<()> {
        for string in strings {
            self.writer.write_all(string.as_bytes())?;
            self.writer.write_u8(0)?;
        }
        Ok(())
    }

    pub fn write_file_chunk_map(&mut self, map: &[(u16, Vec<u16>)]) -> Result<()> {
        for (dir_id, chunks) in map {
            self.writer.write_u16::<LittleEndian>(*dir_id)?;
            for &chunk_id in chunks {
                self.writer.write_u16::<LittleEndian>(chunk_id)?;
            }
            self.writer.write_u16::<LittleEndian>(0xFFFF)?; // Terminator
        }
        Ok(())
    }

    pub fn write_raw_file_chunk_map(&mut self, map: &[RawFileRecord]) -> Result<()> {
        for file in map {
            self.writer.write_u16::<LittleEndian>(file.directory_id)?;
            for &chunk_id in &file.chunk_ids {
                self.writer.write_u16::<LittleEndian>(chunk_id)?;
            }
            self.writer.write_u16::<LittleEndian>(0xffff)?;
        }
        Ok(())
    }

    pub fn write_chunk_settings(&mut self, settings: &ChunkSettings) -> Result<()> {
        self.writer
            .write_u16::<LittleEndian>(settings.num_archive_files)?;
        self.writer.write_u16::<LittleEndian>(settings.num_chunks)?;
        Ok(())
    }

    pub fn write_chunks(&mut self, chunks: &[Chunk]) -> Result<()> {
        for chunk in chunks {
            self.writer.write_u32::<LittleEndian>(chunk.offset)?;
            self.writer
                .write_u32::<LittleEndian>(chunk.compressed_length)?;
            self.writer
                .write_u32::<LittleEndian>(chunk.decompressed_length)?;
            self.writer.write_u16::<LittleEndian>(chunk.flags)?;
            self.writer.write_u16::<LittleEndian>(chunk.file)?;
        }
        Ok(())
    }

    pub fn write_global_settings(&mut self, settings: &RangeSettings) -> Result<()> {
        self.writer.write_u8(settings.win_size)?;
        self.writer.write_u8(settings.flags)?;
        self.writer.write_u8(settings.offset_table_size)?;
        self.writer.write_u8(settings.offset_tables)?;
        self.writer.write_u8(settings.offset_contexts)?;
        self.writer.write_u8(settings.ref_length_table_size)?;
        self.writer.write_u8(settings.ref_length_tables)?;
        self.writer.write_u8(settings.ref_offset_table_size)?;
        self.writer.write_u8(settings.ref_offset_tables)?;
        self.writer.write_u8(settings.big_min_match)?;
        Ok(())
    }
}

impl RawArchive {
    /// Serialize the metadata block exactly as represented by this value.
    /// Payload bytes are intentionally not copied.
    pub fn write_metadata_to<W: Write>(&self, writer: W) -> Result<()> {
        let mut writer = DzipWriter::new(writer);
        writer.write_archive_settings(&self.settings)?;
        writer.write_raw_strings(&self.strings)?;
        writer.write_raw_file_chunk_map(&self.files)?;
        writer.write_chunk_settings(&self.chunk_settings)?;
        writer.write_chunks(&self.chunks)?;
        writer.write_raw_strings(&self.volume_files)?;
        if let Some(settings) = self.range_settings {
            writer.write_global_settings(&settings)?;
        }
        Ok(())
    }
}

#[cfg(feature = "encode")]
pub fn compress_data(data: &[u8], compression: crate::Compression) -> Result<(u16, Vec<u8>)> {
    Ok((
        compression.flag(),
        crate::codec::encode(compression, data, RangeSettings::default())?,
    ))
}
