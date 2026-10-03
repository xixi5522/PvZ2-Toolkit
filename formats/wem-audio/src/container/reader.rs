use super::{RiffEndian, WemChunk, WemChunks, WemCodec, WemMetadata};
use crate::error::{WemError, WemResult};
use std::io::{Read, Seek, SeekFrom, Write};

/// Parsed WEM container backed by a seekable source.
pub struct WemReader<R> {
    input: R,
    metadata: WemMetadata,
    chunks: WemChunks,
}

impl<R: Read + Seek> WemReader<R> {
    pub fn new(mut input: R) -> WemResult<Self> {
        let file_size = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;

        let mut header = [0_u8; 12];
        input.read_exact(&mut header)?;
        let endian = match &header[..4] {
            b"RIFF" => RiffEndian::Little,
            b"RIFX" => RiffEndian::Big,
            actual => {
                return Err(WemError::invalid_riff(format!(
                    "expected RIFF or RIFX, found {actual:02X?}"
                )));
            }
        };
        if &header[8..12] != b"WAVE" {
            return Err(WemError::invalid_riff("RIFF form type is not WAVE"));
        }

        let declared_riff_size =
            u64::from(endian.read_u32([header[4], header[5], header[6], header[7]]))
                .checked_add(8)
                .ok_or_else(|| WemError::size_overflow("RIFF size"))?;
        let scan_end = declared_riff_size.min(file_size);
        let mut chunks = WemChunks::default();
        let mut offset = 12_u64;

        while offset
            .checked_add(8)
            .is_some_and(|header_end| header_end <= scan_end)
        {
            input.seek(SeekFrom::Start(offset))?;
            let mut chunk_header = [0_u8; 8];
            input.read_exact(&mut chunk_header)?;
            let id = [
                chunk_header[0],
                chunk_header[1],
                chunk_header[2],
                chunk_header[3],
            ];
            let declared_size = endian.read_u32([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]);
            let data_offset = offset
                .checked_add(8)
                .ok_or_else(|| WemError::size_overflow("RIFF chunk offset"))?;
            let available = file_size
                .saturating_sub(data_offset)
                .min(u64::from(declared_size));
            let chunk = WemChunk::new(id, data_offset, available, declared_size);

            match &id {
                b"fmt " if chunks.fmt.is_none() => chunks.fmt = Some(chunk),
                b"data" if chunks.data.is_none() => chunks.data = Some(chunk),
                b"vorb" if chunks.vorb.is_none() => chunks.vorb = Some(chunk),
                b"cue " if chunks.cue.is_none() => chunks.cue = Some(chunk),
                b"LIST" if chunks.list.is_none() => chunks.list = Some(chunk),
                b"smpl" if chunks.smpl.is_none() => chunks.smpl = Some(chunk),
                _ => {}
            }

            let padded_size = u64::from(declared_size)
                .checked_add(u64::from(declared_size & 1))
                .ok_or_else(|| WemError::size_overflow("RIFF padded chunk size"))?;
            offset = data_offset
                .checked_add(padded_size)
                .ok_or_else(|| WemError::size_overflow("RIFF next chunk offset"))?;
        }

        let fmt = chunks.fmt.ok_or_else(|| WemError::missing_chunk("fmt "))?;
        let data = chunks.data.ok_or_else(|| WemError::missing_chunk("data"))?;
        let mut fmt_bytes = [0_u8; 16];
        read_chunk_exact_from(&mut input, fmt, 0, &mut fmt_bytes)?;

        let format_tag = endian.read_u16([fmt_bytes[0], fmt_bytes[1]]);
        let channels = endian.read_u16([fmt_bytes[2], fmt_bytes[3]]);
        let sample_rate = endian.read_u32([fmt_bytes[4], fmt_bytes[5], fmt_bytes[6], fmt_bytes[7]]);
        let average_bytes_per_second =
            endian.read_u32([fmt_bytes[8], fmt_bytes[9], fmt_bytes[10], fmt_bytes[11]]);
        let block_align = endian.read_u16([fmt_bytes[12], fmt_bytes[13]]);
        let bits_per_sample = endian.read_u16([fmt_bytes[14], fmt_bytes[15]]);
        if channels == 0 {
            return Err(WemError::invalid_field(
                "channels",
                0,
                "channel count must be greater than zero",
            ));
        }
        if sample_rate == 0 {
            return Err(WemError::invalid_field(
                "sample_rate",
                0,
                "sample rate must be greater than zero",
            ));
        }

        let metadata = WemMetadata {
            endian,
            codec: WemCodec::from_format_tag(format_tag),
            channels,
            sample_rate,
            average_bytes_per_second,
            block_align,
            bits_per_sample,
            data_offset: data.offset(),
            data_size: data.size(),
            declared_data_size: data.declared_size(),
            file_size,
            declared_riff_size,
        };

        Ok(Self {
            input,
            metadata,
            chunks,
        })
    }

    pub const fn metadata(&self) -> &WemMetadata {
        &self.metadata
    }

    pub const fn chunks(&self) -> &WemChunks {
        &self.chunks
    }

    pub fn input_mut(&mut self) -> &mut R {
        &mut self.input
    }

    pub fn into_inner(self) -> R {
        self.input
    }

    #[cfg(feature = "vorbis")]
    pub(crate) fn into_parts(self) -> (R, WemMetadata, WemChunks) {
        (self.input, self.metadata, self.chunks)
    }

    pub fn read_chunk_exact(
        &mut self,
        chunk: WemChunk,
        relative_offset: u64,
        output: &mut [u8],
    ) -> WemResult<()> {
        read_chunk_exact_from(&mut self.input, chunk, relative_offset, output)
    }

    pub fn copy_chunk_to<W: Write>(&mut self, chunk: WemChunk, mut output: W) -> WemResult<u64> {
        self.input.seek(SeekFrom::Start(chunk.offset()))?;
        let mut source = (&mut self.input).take(chunk.size());
        let copied = std::io::copy(&mut source, &mut output)?;
        if copied != chunk.size() {
            return Err(WemError::invalid_chunk(
                chunk_name(chunk.id()),
                "payload ended before its validated size",
            ));
        }
        Ok(copied)
    }
}

pub fn inspect_wem<R: Read + Seek>(input: &mut R) -> WemResult<WemMetadata> {
    Ok(WemReader::new(input)?.metadata().clone())
}

pub(crate) fn read_chunk_exact_from<R: Read + Seek>(
    input: &mut R,
    chunk: WemChunk,
    relative_offset: u64,
    output: &mut [u8],
) -> WemResult<()> {
    let length = u64::try_from(output.len()).map_err(|_| WemError::size_overflow("read length"))?;
    let end = relative_offset
        .checked_add(length)
        .ok_or_else(|| WemError::size_overflow("chunk read range"))?;
    if end > chunk.size() {
        return Err(WemError::invalid_chunk(
            chunk_name(chunk.id()),
            format!(
                "read range {relative_offset}..{end} exceeds available size {}",
                chunk.size()
            ),
        ));
    }
    let absolute = chunk
        .offset()
        .checked_add(relative_offset)
        .ok_or_else(|| WemError::size_overflow("absolute chunk read offset"))?;
    input.seek(SeekFrom::Start(absolute))?;
    input.read_exact(output)?;
    Ok(())
}

fn chunk_name(id: [u8; 4]) -> &'static str {
    match &id {
        b"fmt " => "fmt ",
        b"data" => "data",
        b"vorb" => "vorb",
        b"cue " => "cue ",
        b"LIST" => "LIST",
        b"smpl" => "smpl",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pcm_wem(fmt_size: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(4 + 8 + fmt_size + 8 + 4).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&fmt_size.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&44_100_u32.to_le_bytes());
        bytes.extend_from_slice(&176_400_u32.to_le_bytes());
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.resize(12 + 8 + fmt_size as usize, 0);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes
    }

    #[test]
    fn inspects_pcm_metadata() {
        let reader = WemReader::new(Cursor::new(pcm_wem(16))).unwrap();
        assert_eq!(reader.metadata().endian, RiffEndian::Little);
        assert_eq!(reader.metadata().codec, WemCodec::Pcm);
        assert_eq!(reader.metadata().channels, 2);
        assert_eq!(reader.metadata().sample_rate, 44_100);
        assert_eq!(reader.metadata().data_size, 4);
    }

    #[test]
    fn bounds_fmt_reads_to_the_chunk() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&26_u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 0]);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        let error = match WemReader::new(Cursor::new(bytes)) {
            Ok(_) => panic!("short fmt chunk unexpectedly parsed"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            WemError::InvalidChunk { chunk: "fmt ", .. }
        ));
    }

    #[test]
    fn reports_truncated_prefetch_data() {
        let mut bytes = pcm_wem(16);
        bytes[40..44].copy_from_slice(&100_u32.to_le_bytes());
        let reader = WemReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.metadata().data_size, 4);
        assert_eq!(reader.metadata().declared_data_size, 100);
        assert!(reader.metadata().is_prefetch());
    }
}
