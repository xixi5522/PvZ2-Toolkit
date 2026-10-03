use crate::error::{WemError, WemResult};
use std::io::{Read, Seek, SeekFrom, Write};

pub(crate) const IMA_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

pub(crate) const IMA_INDEX_TABLE: [i8; 16] =
    [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn clamp_s16(val: i32) -> i16 {
    if val < -32768 {
        -32768
    } else if val > 32767 {
        32767
    } else {
        val as i16
    }
}

/// Expands a single nibble using the Wwise IMA ADPCM algorithm (multiplication variant)
fn expand_nibble(nibble: u8, predictor: &mut i32, step_index: &mut i32) {
    let step = IMA_STEP_TABLE[*step_index as usize];
    let mut delta = (nibble & 0x7) as i32;

    // (delta * 2 + 1) * step / 8
    delta = ((delta * 2 + 1) * step) >> 3;

    if (nibble & 8) != 0 {
        delta = -delta;
    }

    *predictor += delta;
    *predictor = clamp_s16(*predictor) as i32;

    *step_index += IMA_INDEX_TABLE[nibble as usize] as i32;
    *step_index = (*step_index).clamp(0, 88);
}

pub struct AdpcmParams {
    pub channels: u16,
    pub sample_rate: u32,
    pub block_align: u16,
    pub is_little_endian: bool,
    pub data_offset: u64,
    pub data_size: u32,
}

pub fn process_adpcm<R: Read + Seek, W: Write + Seek>(
    mut input: R,
    output: W,
    params: AdpcmParams,
) -> WemResult<()> {
    let AdpcmParams {
        channels,
        sample_rate,
        block_align,
        is_little_endian,
        data_offset,
        data_size,
    } = params;

    if channels == 0 {
        return Err(WemError::parse(
            "ADPCM channel count must be greater than zero",
        ));
    }
    if sample_rate == 0 {
        return Err(WemError::parse(
            "ADPCM sample rate must be greater than zero",
        ));
    }
    input.seek(SeekFrom::Start(data_offset))?;

    let data_size = usize::try_from(data_size)
        .map_err(|_| WemError::parse("ADPCM payload does not fit in memory"))?;
    let mut buffer = vec![0u8; data_size];
    input.read_exact(&mut buffer)?;

    // Wwise IMA ADPCM Block Size is typically 36 bytes.
    // If multiple channels, blocks are usually interleaved?
    // vgmstream suggests:
    // Mono: contiguous blocks
    // Stereo/Multi: interleaved blocks of size (0x24 * channels)? No, vgmstream says "external interleave (fixed size)".
    // Usually means L block, R block, L block, R block.
    // Let's assume standard Wwise block interleave of 36 bytes.

    const BLOCK_SIZE: usize = 36;
    let frame_size = BLOCK_SIZE
        .checked_mul(usize::from(channels))
        .ok_or_else(|| WemError::parse("ADPCM frame size overflows usize"))?;
    if block_align != 0
        && usize::from(block_align) != BLOCK_SIZE
        && usize::from(block_align) != frame_size
    {
        return Err(WemError::parse(format!(
            "unsupported ADPCM block alignment {block_align}"
        )));
    }
    if buffer.len() % frame_size != 0 {
        return Err(WemError::parse(format!(
            "ADPCM payload size {} is not aligned to its {frame_size}-byte channel frame",
            buffer.len()
        )));
    }

    // Total samples per block = 64.
    // We need to decode all blocks.

    // Output WAV setup
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav_writer = hound::WavWriter::new(output, spec).map_err(WemError::Wav)?;

    // Decode logic
    // We will decode one block for each channel, then write interleaved samples?
    // Or decode all into a buffer and then write?
    // Writing interleaved is better for streaming, but we have all data in memory.

    // Iterate over blocks (interleaved frames)
    // Frame = 36 bytes * channels

    let mut offset = 0;

    while offset + frame_size <= buffer.len() {
        // For each frame (contains 1 block per channel)

        // We need to collect 64 samples for each channel, then interleave them.
        let mut channel_samples: Vec<Vec<i16>> = vec![Vec::with_capacity(64); channels as usize];

        for (ch, channel_sample) in channel_samples
            .iter_mut()
            .enumerate()
            .take(channels as usize)
        {
            let block_offset = offset + ch * BLOCK_SIZE;
            let block = &buffer[block_offset..block_offset + BLOCK_SIZE];

            // Header
            let predictor = if is_little_endian {
                i16::from_le_bytes([block[0], block[1]]) as i32
            } else {
                i16::from_be_bytes([block[0], block[1]]) as i32
            };

            let mut step_index = (block[2] as i32).clamp(0, 88);

            // Header sample is the first output sample
            let mut current_predictor = predictor;
            channel_sample.push(current_predictor as i16);

            // Data: 32 bytes (64 nibbles)
            // But last nibble is skipped!
            // First nibble is low nibble of byte 4.

            // i goes from 0 to 62 (63 nibbles).
            // Byte index starts at 4.

            // vgmstream:
            // for i = first_sample (1) to < 64
            // byte_offset = 4 + (i-1)/2
            // nibble_shift = (i-1)&1 ? 4 : 0

            for i in 1..64 {
                let byte_idx = 4 + (i - 1) / 2;
                let byte = block[byte_idx];
                let shift = if ((i - 1) & 1) != 0 { 4 } else { 0 };
                let nibble = (byte >> shift) & 0x0F;

                expand_nibble(nibble, &mut current_predictor, &mut step_index);
                channel_sample.push(current_predictor as i16);
            }
        }

        // Interleave and write
        for i in 0..64 {
            for (_, channel_sample) in channel_samples.iter().enumerate().take(channels as usize) {
                wav_writer
                    .write_sample(channel_sample[i])
                    .map_err(WemError::Wav)?;
            }
        }

        offset += frame_size;
    }

    wav_writer.finalize().map_err(WemError::Wav)?;
    Ok(())
}
