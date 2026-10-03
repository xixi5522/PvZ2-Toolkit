use crate::error::{WemError, WemResult};
use std::io::{Read, Seek, SeekFrom, Write};

pub struct PcmParams {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub is_little_endian: bool,
    pub data_offset: u64,
    pub data_size: u32,
}

pub fn process_pcm<R: Read + Seek, W: Write + Seek>(
    mut input: R,
    output: W,
    params: PcmParams,
) -> WemResult<()> {
    if params.channels == 0 {
        return Err(WemError::parse(
            "PCM channel count must be greater than zero",
        ));
    }
    if params.sample_rate == 0 {
        return Err(WemError::parse("PCM sample rate must be greater than zero"));
    }
    input.seek(SeekFrom::Start(params.data_offset))?;

    let spec = hound::WavSpec {
        channels: params.channels,
        sample_rate: params.sample_rate,
        bits_per_sample: params.bits_per_sample,
        sample_format: hound::SampleFormat::Int,
    };

    let mut wav_writer = hound::WavWriter::new(output, spec).map_err(WemError::Wav)?;
    let mut buffer = vec![0u8; params.data_size as usize];
    input.read_exact(&mut buffer)?;

    match params.bits_per_sample {
        8 => {
            for byte in buffer {
                let sample = (i16::from(byte) - 128) as i8;
                wav_writer.write_sample(sample).map_err(WemError::Wav)?;
            }
        }
        16 => {
            if !buffer.len().is_multiple_of(2) {
                return Err(WemError::parse("16-bit PCM payload has a trailing byte"));
            }
            for bytes in buffer.as_chunks::<2>().0 {
                let sample = if params.is_little_endian {
                    i16::from_le_bytes(*bytes)
                } else {
                    i16::from_be_bytes(*bytes)
                };
                wav_writer.write_sample(sample).map_err(WemError::Wav)?;
            }
        }
        24 => {
            if !buffer.len().is_multiple_of(3) {
                return Err(WemError::parse("24-bit PCM payload is not sample-aligned"));
            }
            for bytes in buffer.as_chunks::<3>().0 {
                let value = if params.is_little_endian {
                    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
                } else {
                    (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2])
                };
                let sample = ((value << 8) as i32) >> 8;
                wav_writer.write_sample(sample).map_err(WemError::Wav)?;
            }
        }
        32 => {
            if !buffer.len().is_multiple_of(4) {
                return Err(WemError::parse("32-bit PCM payload is not sample-aligned"));
            }
            for bytes in buffer.as_chunks::<4>().0 {
                let sample = if params.is_little_endian {
                    i32::from_le_bytes(*bytes)
                } else {
                    i32::from_be_bytes(*bytes)
                };
                wav_writer.write_sample(sample).map_err(WemError::Wav)?;
            }
        }
        _ => {
            return Err(WemError::parse(format!(
                "unsupported PCM bit depth: {}",
                params.bits_per_sample
            )));
        }
    }

    wav_writer.finalize().map_err(WemError::Wav)?;
    Ok(())
}
