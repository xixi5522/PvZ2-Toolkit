use crate::ptx::Rgba8Surface;
use crate::ptx::error::{PtxError, Result};

pub(crate) fn decode_palette(data: &[u8], pixel_count: usize) -> Result<Vec<u8>> {
    let (&count, mut stream) = data
        .split_first()
        .ok_or_else(|| PtxError::InvalidPalette("missing palette count".into()))?;
    let mut palette = Vec::with_capacity(if count == 0 { 2 } else { count as usize });
    let depth = if count == 0 {
        palette.extend_from_slice(&[0, 255]);
        1
    } else {
        let count = count as usize;
        if stream.len() < count {
            return Err(PtxError::InvalidPalette(format!(
                "expected {count} palette entries, found {}",
                stream.len()
            )));
        }
        for &value in &stream[..count] {
            palette.push((value << 4) | value);
        }
        stream = &stream[count..];
        (usize::BITS - count.saturating_sub(1).leading_zeros()) as usize
    };
    let expected_stream_len = pixel_count
        .checked_mul(depth)
        .ok_or(PtxError::DimensionsOverflow)?
        .div_ceil(8);
    if stream.len() != expected_stream_len {
        return Err(PtxError::InvalidPalette(format!(
            "expected {expected_stream_len} index bytes, found {}",
            stream.len()
        )));
    }

    let mut output = Vec::with_capacity(pixel_count);
    let mut bit_offset = 0;
    for _ in 0..pixel_count {
        let mut index = 0usize;
        for _ in 0..depth {
            let byte = stream[bit_offset / 8];
            let bit = (byte >> (7 - bit_offset % 8)) & 1;
            index = (index << 1) | bit as usize;
            bit_offset += 1;
        }
        output.push(*palette.get(index).unwrap_or(&palette[0]));
    }
    Ok(output)
}

pub(crate) fn encode_palette(surface: Rgba8Surface<'_>) -> Result<Vec<u8>> {
    let pixels = usize::try_from(u64::from(surface.width()) * u64::from(surface.height()))
        .map_err(|_| PtxError::DimensionsOverflow)?;
    let mut output = Vec::with_capacity(17 + pixels.div_ceil(2));
    output.push(16);
    output.extend(0u8..16);

    let mut writer = BitWriter::default();
    for y in 0..surface.height() {
        for x in 0..surface.width() {
            writer.write(surface.pixel(x, y)[3] >> 4, 4);
        }
    }
    output.extend(writer.finish());
    Ok(output)
}

#[derive(Default)]
struct BitWriter {
    byte: u8,
    used: u8,
    output: Vec<u8>,
}

impl BitWriter {
    fn write(&mut self, value: u8, width: u8) {
        for bit_index in (0..width).rev() {
            self.byte |= ((value >> bit_index) & 1) << self.used;
            self.used += 1;
            if self.used == 8 {
                self.output.push(self.byte);
                self.byte = 0;
                self.used = 0;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.used != 0 {
            self.output.push(self.byte);
        }
        self.output
    }
}
