use crate::container::WWISE_FORMAT_AAC;
use crate::error::{WemError, WemResult};
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

/// Metadata required by Wwise's AAC WEM header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AacMetadata {
    pub channels: u16,
    pub sample_rate: u32,
    pub average_bytes_per_second: u32,
}

impl AacMetadata {
    fn validate(self) -> WemResult<Self> {
        if self.channels == 0 {
            return Err(WemError::invalid_field(
                "channels",
                0,
                "channel count must be greater than zero",
            ));
        }
        if self.sample_rate == 0 {
            return Err(WemError::invalid_field(
                "sample_rate",
                0,
                "sample rate must be greater than zero",
            ));
        }
        Ok(self)
    }
}

/// Probes AAC-in-MP4/M4A metadata from any Symphonia media source.
///
/// A `Cursor<Vec<u8>>` can be used by browser and WASM callers.
pub fn probe_aac_metadata<R: MediaSource + 'static>(
    input: R,
    extension: Option<&str>,
) -> WemResult<AacMetadata> {
    let byte_len = input.byte_len();
    let media = MediaSourceStream::new(Box::new(input), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            media,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| WemError::audio(format!("failed to probe AAC stream: {error}")))?;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .and_then(|params| params.audio())
                .is_some_and(|audio| audio.codec != CODEC_ID_NULL_AUDIO)
        })
        .ok_or_else(|| WemError::audio("AAC stream has no supported audio track"))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| WemError::audio("AAC audio parameters are unavailable"))?;
    let channels = params
        .channels
        .as_ref()
        .map(|channels| u16::try_from(channels.count()))
        .transpose()
        .map_err(|_| WemError::size_overflow("AAC channel count"))?
        .ok_or_else(|| WemError::audio("AAC channel count is unavailable"))?;
    let sample_rate = params
        .sample_rate
        .ok_or_else(|| WemError::audio("AAC sample rate is unavailable"))?;
    let duration_seconds = track
        .time_base
        .zip(track.duration)
        .map(|(base, duration)| {
            duration.get() as f64 * f64::from(base.numer.get()) / f64::from(base.denom.get())
        })
        .or_else(|| {
            track
                .num_frames
                .map(|frames| frames as f64 / f64::from(sample_rate))
        })
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0);
    let average_bytes_per_second = byte_len
        .zip(duration_seconds)
        .and_then(|(bytes, seconds)| {
            let value = (bytes as f64 / seconds).ceil();
            (value.is_finite() && value > 0.0 && value <= f64::from(u32::MAX))
                .then_some(value as u32)
        })
        .unwrap_or(16_000);

    AacMetadata {
        channels,
        sample_rate,
        average_bytes_per_second,
    }
    .validate()
}

pub fn probe_aac_bytes(input: Vec<u8>, extension: Option<&str>) -> WemResult<AacMetadata> {
    probe_aac_metadata(Cursor::new(input), extension)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn probe_aac_file(path: &std::path::Path) -> WemResult<AacMetadata> {
    let input = std::fs::File::open(path)?;
    probe_aac_metadata(input, path.extension().and_then(std::ffi::OsStr::to_str))
}

/// Wraps an existing AAC-in-MP4/M4A stream in a Wwise AAC WEM container.
pub struct AacWemEncoder<R: Read + Seek> {
    input: R,
    metadata: AacMetadata,
    data_size: u32,
}

impl<R: Read + Seek> AacWemEncoder<R> {
    pub fn new(mut input: R, metadata: AacMetadata) -> WemResult<Self> {
        let metadata = metadata.validate()?;
        let data_size = u32::try_from(input.seek(SeekFrom::End(0))?)
            .map_err(|_| WemError::size_overflow("AAC input"))?;
        input.seek(SeekFrom::Start(0))?;
        Ok(Self {
            input,
            metadata,
            data_size,
        })
    }

    pub fn encode<W: Write>(mut self, mut output: W) -> WemResult<()> {
        const FMT_CHUNK_SIZE: u32 = 0x20;
        let riff_payload_size = 4_u32
            .checked_add(8 + FMT_CHUNK_SIZE)
            .and_then(|size| size.checked_add(8 + self.data_size))
            .ok_or_else(|| WemError::size_overflow("AAC WEM RIFF"))?;

        output.write_all(b"RIFF")?;
        output.write_u32::<LittleEndian>(riff_payload_size)?;
        output.write_all(b"WAVEfmt ")?;
        output.write_u32::<LittleEndian>(FMT_CHUNK_SIZE)?;
        output.write_u16::<LittleEndian>(WWISE_FORMAT_AAC)?;
        output.write_u16::<LittleEndian>(self.metadata.channels)?;
        output.write_u32::<LittleEndian>(self.metadata.sample_rate)?;
        output.write_u32::<LittleEndian>(self.metadata.average_bytes_per_second)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_all(&[0; 14])?;
        output.write_all(b"data")?;
        output.write_u32::<LittleEndian>(self.data_size)?;

        self.input.seek(SeekFrom::Start(0))?;
        let copied = std::io::copy(&mut self.input, &mut output)?;
        if copied != u64::from(self.data_size) {
            return Err(WemError::invalid_chunk(
                "AAC",
                "source length changed while encoding",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WemCodec, WemReader, extract_wem_aac};

    #[test]
    fn wraps_and_extracts_aac_without_changing_payload() {
        let payload = b"synthetic AAC-in-MP4 payload".to_vec();
        let encoder = AacWemEncoder::new(
            Cursor::new(payload.clone()),
            AacMetadata {
                channels: 2,
                sample_rate: 44_100,
                average_bytes_per_second: 16_000,
            },
        )
        .unwrap();
        let mut wem = Vec::new();
        encoder.encode(&mut wem).unwrap();
        let reader = WemReader::new(Cursor::new(&wem)).unwrap();
        assert_eq!(reader.metadata().codec, WemCodec::Aac);

        let mut extracted = Vec::new();
        extract_wem_aac(Cursor::new(wem), &mut extracted).unwrap();
        assert_eq!(extracted, payload);
    }

    #[test]
    fn rejects_missing_required_metadata() {
        let error = AacWemEncoder::new(
            Cursor::new(Vec::<u8>::new()),
            AacMetadata {
                channels: 0,
                sample_rate: 44_100,
                average_bytes_per_second: 0,
            },
        )
        .err()
        .unwrap();
        assert!(matches!(
            error,
            WemError::InvalidField {
                field: "channels",
                ..
            }
        ));
    }
}
