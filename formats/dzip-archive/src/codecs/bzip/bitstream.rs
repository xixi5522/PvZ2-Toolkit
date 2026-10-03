use alloc::vec::Vec;

use crate::codecs::bzip::Error;

pub(super) struct BitWriter {
    output: Vec<u8>,
    byte: u8,
    bits: u8,
}

impl BitWriter {
    pub(super) fn with_output(mut output: Vec<u8>) -> Self {
        output.clear();
        Self {
            output,
            byte: 0,
            bits: 0,
        }
    }

    pub(super) fn write_bits(&mut self, value: u64, count: usize) {
        for index in (0..count).rev() {
            self.byte = self.byte << 1 | (value >> index & 1) as u8;
            self.bits += 1;
            if self.bits == 8 {
                self.output.push(self.byte);
                self.byte = 0;
                self.bits = 0;
            }
        }
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        if self.bits != 0 {
            self.output.push(self.byte << (8 - self.bits));
        }
        self.output
    }
}

pub(super) struct BitReader<'a> {
    input: &'a [u8],
    byte: usize,
    bit: u8,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte: 0,
            bit: 0,
        }
    }

    pub(super) fn read_bits(&mut self, count: usize) -> Result<u64, Error> {
        let mut value = 0u64;
        for _ in 0..count {
            let byte = *self
                .input
                .get(self.byte)
                .ok_or_else(|| Error::new("truncated BZip2 bitstream"))?;
            value = value << 1 | u64::from(byte >> (7 - self.bit) & 1);
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.byte += 1;
            }
        }
        Ok(value)
    }

    pub(super) const fn consumed_bytes(&self) -> usize {
        self.byte + if self.bit == 0 { 0 } else { 1 }
    }
}
