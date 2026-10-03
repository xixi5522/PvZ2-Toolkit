use crate::container::WAVE_FORMAT_PCM;
use crate::error::{WemError, WemResult};
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{Read, Write};

/// Streams integer PCM samples from a WAV reader into a PCM WEM container.
pub struct PcmWemEncoder<R: Read> {
    input: hound::WavReader<R>,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    data_size: u32,
}

impl<R: Read> PcmWemEncoder<R> {
    pub fn new(input: R) -> WemResult<Self> {
        let input = hound::WavReader::new(input)?;
        let spec = input.spec();
        if spec.sample_format != hound::SampleFormat::Int {
            return Err(WemError::unsupported_variant("floating-point PCM WAV"));
        }
        if spec.channels == 0 {
            return Err(WemError::invalid_field(
                "channels",
                0,
                "channel count must be greater than zero",
            ));
        }
        if spec.sample_rate == 0 {
            return Err(WemError::invalid_field(
                "sample_rate",
                0,
                "sample rate must be greater than zero",
            ));
        }
        if !matches!(spec.bits_per_sample, 8 | 16 | 24 | 32) {
            return Err(WemError::unsupported_variant("PCM bit depth"));
        }
        let data_size = input
            .len()
            .checked_mul(u32::from(spec.bits_per_sample))
            .and_then(|bits| bits.checked_div(8))
            .ok_or_else(|| WemError::size_overflow("PCM payload"))?;

        Ok(Self {
            input,
            channels: spec.channels,
            sample_rate: spec.sample_rate,
            bits_per_sample: spec.bits_per_sample,
            data_size,
        })
    }

    pub fn encode<W: Write>(mut self, mut output: W) -> WemResult<()> {
        const FMT_CHUNK_SIZE: u32 = 16;
        let riff_payload_size = 4_u32
            .checked_add(8 + FMT_CHUNK_SIZE)
            .and_then(|size| size.checked_add(8 + self.data_size))
            .ok_or_else(|| WemError::size_overflow("PCM WEM RIFF"))?;
        let block_align = self
            .channels
            .checked_mul(self.bits_per_sample)
            .and_then(|bits| bits.checked_div(8))
            .ok_or_else(|| WemError::size_overflow("PCM block alignment"))?;
        let average_bytes_per_second = self
            .sample_rate
            .checked_mul(u32::from(block_align))
            .ok_or_else(|| WemError::size_overflow("PCM byte rate"))?;

        output.write_all(b"RIFF")?;
        output.write_u32::<LittleEndian>(riff_payload_size)?;
        output.write_all(b"WAVEfmt ")?;
        output.write_u32::<LittleEndian>(FMT_CHUNK_SIZE)?;
        output.write_u16::<LittleEndian>(WAVE_FORMAT_PCM)?;
        output.write_u16::<LittleEndian>(self.channels)?;
        output.write_u32::<LittleEndian>(self.sample_rate)?;
        output.write_u32::<LittleEndian>(average_bytes_per_second)?;
        output.write_u16::<LittleEndian>(block_align)?;
        output.write_u16::<LittleEndian>(self.bits_per_sample)?;
        output.write_all(b"data")?;
        output.write_u32::<LittleEndian>(self.data_size)?;

        match self.bits_per_sample {
            8 => {
                for sample in self.input.samples::<i8>() {
                    output.write_u8((i16::from(sample?) + 128) as u8)?;
                }
            }
            16 => {
                for sample in self.input.samples::<i16>() {
                    output.write_i16::<LittleEndian>(sample?)?;
                }
            }
            24 => {
                for sample in self.input.samples::<i32>() {
                    output.write_i24::<LittleEndian>(sample?)?;
                }
            }
            32 => {
                for sample in self.input.samples::<i32>() {
                    output.write_i32::<LittleEndian>(sample?)?;
                }
            }
            _ => unreachable!("bit depth was validated in the constructor"),
        }
        Ok(())
    }
}
