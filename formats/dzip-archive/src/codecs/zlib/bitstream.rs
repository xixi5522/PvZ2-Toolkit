use alloc::vec::Vec;

use crate::codecs::zlib::Error;

pub(super) struct BitReader<'a> {
    input: &'a [u8],
    position: usize,
    bits: u64,
    bit_count: u8,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            position: 0,
            bits: 0,
            bit_count: 0,
        }
    }

    fn fill(&mut self, count: u8) {
        while self.bit_count < count && self.position < self.input.len() {
            self.bits |= u64::from(self.input[self.position]) << self.bit_count;
            self.bit_count += 8;
            self.position += 1;
        }
    }

    fn remaining_bits(&self) -> usize {
        usize::from(self.bit_count) + (self.input.len() - self.position) * 8
    }

    pub(super) fn peek_padded(&mut self, count: u8) -> Result<u32, Error> {
        self.fill(count);
        if self.bit_count == 0 {
            return Err(Error::new("truncated DEFLATE bitstream"));
        }
        let mask = (1u64 << count) - 1;
        Ok((self.bits & mask) as u32)
    }

    pub(super) fn read_bits(&mut self, count: u8) -> Result<u32, Error> {
        if count == 0 {
            return Ok(0);
        }
        self.fill(count);
        if self.bit_count < count {
            return Err(Error::new("truncated DEFLATE bitstream"));
        }
        let mask = (1u64 << count) - 1;
        let value = (self.bits & mask) as u32;
        self.bits >>= count;
        self.bit_count -= count;
        Ok(value)
    }

    pub(super) fn drop_bits(&mut self, count: u8) -> Result<(), Error> {
        if self.remaining_bits() < usize::from(count) {
            return Err(Error::new("truncated Huffman code"));
        }
        self.fill(count);
        self.bits >>= count;
        self.bit_count -= count;
        Ok(())
    }

    pub(super) fn align_to_byte(&mut self) {
        let remainder = self.bit_count % 8;
        self.bits >>= remainder;
        self.bit_count -= remainder;
    }
}

pub(super) struct BitWriter {
    output: Vec<u8>,
    bits: u64,
    bit_count: u8,
}

impl BitWriter {
    pub(super) fn with_output(output: Vec<u8>) -> Self {
        Self {
            output,
            bits: 0,
            bit_count: 0,
        }
    }

    pub(super) fn write_bits(&mut self, value: u32, count: u8) {
        if count == 0 {
            return;
        }
        self.bits |= u64::from(value) << self.bit_count;
        self.bit_count += count;
        while self.bit_count >= 8 {
            self.output.push(self.bits as u8);
            self.bits >>= 8;
            self.bit_count -= 8;
        }
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        if self.bit_count != 0 {
            self.output.push(self.bits as u8);
        }
        self.output
    }
}
