use super::decoder::{IMA_INDEX_TABLE, IMA_STEP_TABLE};
use crate::container::WWISE_FORMAT_IMA_ADPCM;
use crate::error::{WemError, WemResult};
use byteorder::{LittleEndian, WriteBytesExt};
use std::io::{Read, Write};

/// Streams 16-bit WAV samples into Wwise interleaved IMA ADPCM blocks.
pub struct AdpcmWemEncoder<R: Read> {
    input: hound::WavReader<R>,
    channels: u16,
    sample_rate: u32,
    block_count: u32,
    data_size: u32,
}

impl<R: Read> AdpcmWemEncoder<R> {
    pub fn new(input: R) -> WemResult<Self> {
        let input = hound::WavReader::new(input)?;
        let spec = input.spec();
        if spec.sample_format != hound::SampleFormat::Int {
            return Err(WemError::unsupported_variant("floating-point PCM WAV"));
        }
        if spec.bits_per_sample != 16 {
            return Err(WemError::unsupported_variant(
                "ADPCM encoding from a WAV bit depth other than 16",
            ));
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

        let channels = spec.channels;
        let sample_rate = spec.sample_rate;
        let block_count = input.duration().div_ceil(64);
        let data_size = block_count
            .checked_mul(36)
            .and_then(|size| size.checked_mul(u32::from(channels)))
            .ok_or_else(|| WemError::size_overflow("ADPCM payload"))?;

        Ok(Self {
            input,
            channels,
            sample_rate,
            block_count,
            data_size,
        })
    }

    pub fn encode<W: Write>(mut self, mut output: W) -> WemResult<()> {
        const FMT_CHUNK_LEN: u32 = 0x12;
        let riff_payload_size = 4_u32
            .checked_add(8 + FMT_CHUNK_LEN)
            .and_then(|size| size.checked_add(8 + self.data_size))
            .ok_or_else(|| WemError::size_overflow("ADPCM WEM RIFF"))?;
        let block_align = self
            .channels
            .checked_mul(36)
            .ok_or_else(|| WemError::size_overflow("ADPCM block alignment"))?;
        let avg_bytes = u32::try_from(u64::from(self.sample_rate) * u64::from(block_align) / 64)
            .map_err(|_| WemError::size_overflow("ADPCM byte rate"))?;

        output.write_all(b"RIFF")?;
        output.write_u32::<LittleEndian>(riff_payload_size)?;
        output.write_all(b"WAVEfmt ")?;
        output.write_u32::<LittleEndian>(FMT_CHUNK_LEN)?;
        output.write_u16::<LittleEndian>(WWISE_FORMAT_IMA_ADPCM)?;
        output.write_u16::<LittleEndian>(self.channels)?;
        output.write_u32::<LittleEndian>(self.sample_rate)?;
        output.write_u32::<LittleEndian>(avg_bytes)?;
        output.write_u16::<LittleEndian>(block_align)?;
        output.write_u16::<LittleEndian>(4)?;
        output.write_u16::<LittleEndian>(0)?;
        output.write_all(b"data")?;
        output.write_u32::<LittleEndian>(self.data_size)?;

        let channel_count = usize::from(self.channels);
        let mut states = vec![AdpcmState::default(); channel_count];
        let mut block = vec![[0_i16; 64]; channel_count];
        let mut samples = self.input.samples::<i16>();
        for _ in 0..self.block_count {
            block.fill([0; 64]);
            for frame in 0..64 {
                for samples_for_channel in &mut block {
                    if let Some(sample) = samples.next() {
                        samples_for_channel[frame] = sample?;
                    }
                }
            }
            for channel in 0..channel_count {
                output.write_all(&encode_block(&block[channel], &mut states[channel]))?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Default)]
struct AdpcmState {
    predictor: i32,
    step_index: i32,
}

fn encode_block(samples: &[i16; 64], state: &mut AdpcmState) -> [u8; 36] {
    // 36 bytes: 4 byte header + 32 bytes data
    let mut block = [0_u8; 36];

    // Header: Predictor (i16) -> 2 bytes
    //         Step Index (u8) -> 1 byte
    //         Reserved (u8) -> 1 byte

    // We use the first sample as the starting predictor for the block?
    // Wwise decoder logic:
    // Header sample IS the first output sample.
    // So `samples[0]` is written to header.
    // AND `samples[0]` becomes the predictor state for the *next* nibbles.

    let predictor = samples[0] as i32;
    // Step index? We need to find a good step index?
    // Or just carry over from previous block?
    // Wwise encoder likely carries over.
    // But for the very first block, defaults to 0.
    // Also, we should probably clamp it.

    // Write Header
    let pred_i16 = predictor as i16;
    block[0] = pred_i16 as u8;
    block[1] = (pred_i16 >> 8) as u8;
    block[2] = state.step_index as u8;
    block[3] = 0;

    // Update state to match what decoder will have after reading header
    state.predictor = predictor;
    // state.step_index remains same

    // Encode remaining 63 samples
    // Layout:
    // Byte 4 contains: Low nibble = Sample 1 (logic index), High nibble = Sample 2
    // Decoder:
    // for i in 1..64:
    //   byte_idx = 4 + (i-1)/2
    //   shift = ((i-1)&1) ? 4 : 0

    let mut bit_buffer = 0u8;

    for (i, &sample) in samples.iter().take(64).enumerate().skip(1) {
        let _diff = sample as i32 - state.predictor;
        let step = IMA_STEP_TABLE[state.step_index as usize];

        // Calculate best nibble
        // Decoder delta = ((nibble & 7) * 2 + 1) * step / 8
        // If nibble & 8, delta = -delta.

        // We want: predictor + delta ~= sample
        // delta ~= diff

        let mut best_nibble = 0;
        let mut min_error = i32::MAX;

        // Brute force 16 nibbles? It's fast enough.
        for nibble in 0..16 {
            let mut delta = nibble & 0x7;
            delta = ((delta * 2 + 1) * step) >> 3;
            if (nibble & 8) != 0 {
                delta = -delta;
            }

            // Predict
            let pred = (state.predictor + delta).clamp(-32768, 32767);
            let error = (sample as i32 - pred).abs();

            if error < min_error {
                min_error = error;
                best_nibble = nibble;
            }
        }

        // Apply best nibble to update state
        let nibble = best_nibble;
        let mut delta = nibble & 0x7;
        delta = ((delta * 2 + 1) * step) >> 3;
        if (nibble & 8) != 0 {
            delta = -delta;
        }
        state.predictor = (state.predictor + delta).clamp(-32768, 32767);
        state.step_index =
            (state.step_index + IMA_INDEX_TABLE[nibble as usize] as i32).clamp(0, 88);

        // Pack nibble
        // Decoder: (byte >> shift) & 0x0F
        // i=1 (first sample after header): (1-1)/2 = 0 -> byte 4. shift=0. -> Low nibble.
        // i=2: (2-1)/2 = 0 -> byte 4. shift=4 -> High nibble.

        if (i - 1) % 2 == 0 {
            bit_buffer = nibble as u8; // Low nibble
        } else {
            bit_buffer |= (nibble as u8) << 4; // High nibble
            // Write byte
            let byte_idx = 4 + (i - 1) / 2;
            block[byte_idx] = bit_buffer;
        }
    }

    // Note: The loop finishes at i=63.
    // i=63: (62)/2 = 31. byte_idx = 35. This is the last byte of 36-byte block (idx 0..35).
    // i=63 is odd -> High nibble written.
    // If we had even samples (impossible with 64), we'd need to flush.

    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_adpcm_roundtrip() {
        // 1. Generate Synthetic WAV (Mono, 16-bit, 44100Hz, Sine Wave)
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut wav_buffer = Vec::new();
        let mut wav_writer = hound::WavWriter::new(Cursor::new(&mut wav_buffer), spec).unwrap();

        // 1000 samples approx 0.02s
        for t in 0..1000 {
            let v = (t as f32 * 0.1).sin() * 10000.0;
            wav_writer.write_sample(v as i16).unwrap();
        }
        wav_writer.finalize().unwrap();

        // 2. Encode to ADPCM WEM
        let mut wem_buffer = Vec::new();
        let encoder = AdpcmWemEncoder::new(Cursor::new(wav_buffer)).unwrap();
        encoder.encode(&mut wem_buffer).unwrap();

        // 3. Decode back to WAV. Non-Vorbis codecs do not require codebooks.
        let mut decoded_wav_buffer = Vec::new();
        crate::decode_wem_to_wav(
            Cursor::new(wem_buffer),
            Cursor::new(&mut decoded_wav_buffer),
            &crate::WemDecodeOptions::new().without_vorbis_codebooks(),
        )
        .expect("Failed to decode ADPCM WEM");

        // 4. Verify Output
        let mut wav_reader = hound::WavReader::new(Cursor::new(decoded_wav_buffer)).unwrap();
        let decoded_spec = wav_reader.spec();

        assert_eq!(decoded_spec.channels, 1);
        assert_eq!(decoded_spec.sample_rate, 44100);
        assert_eq!(decoded_spec.bits_per_sample, 16);

        let samples: Vec<i16> = wav_reader.samples::<i16>().map(|s| s.unwrap()).collect();
        // Wwise ADPCM is block-based (64 samples).
        // 1000 samples -> ceil(1000/64) * 64 = 16 * 64 = 1024 samples expected?
        // Let's check logic.
        // Yes, decoder fills blocks.

        assert!(samples.len() >= 1000);
        assert!(samples.len() <= 1024); // Should be padded to block size
    }
}
