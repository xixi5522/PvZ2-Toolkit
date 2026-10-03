use alloc::vec::Vec;

use crate::codecs::lzma::Error;

const PROB_TOTAL: u32 = 1 << 11;
const MOVE_BITS: u32 = 5;
const TOP_VALUE: u32 = 1 << 24;

pub(super) struct RangeDecoder<'a> {
    input: &'a [u8],
    position: usize,
    range: u32,
    code: u32,
}

impl<'a> RangeDecoder<'a> {
    pub(super) fn new(input: &'a [u8]) -> Result<Self, Error> {
        if input.len() < 5 || input[0] != 0 {
            return Err(Error::new("truncated or invalid LZMA range prefix"));
        }
        let mut code = 0u32;
        for &byte in &input[1..5] {
            code = code << 8 | u32::from(byte);
        }
        Ok(Self {
            input,
            position: 5,
            range: u32::MAX,
            code,
        })
    }

    fn read_byte(&mut self) -> Result<u8, Error> {
        let byte = *self
            .input
            .get(self.position)
            .ok_or_else(|| Error::new("truncated LZMA range stream"))?;
        self.position += 1;
        Ok(byte)
    }

    fn normalize(&mut self) -> Result<(), Error> {
        if self.range < TOP_VALUE {
            self.range <<= 8;
            self.code = self.code << 8 | u32::from(self.read_byte()?);
        }
        Ok(())
    }

    pub(super) fn decode_bit(&mut self, probability: &mut u16) -> Result<u32, Error> {
        let bound = (self.range >> 11) * u32::from(*probability);
        let bit = if self.code < bound {
            self.range = bound;
            *probability += (PROB_TOTAL as u16 - *probability) >> MOVE_BITS;
            0
        } else {
            self.range -= bound;
            self.code -= bound;
            *probability -= *probability >> MOVE_BITS;
            1
        };
        self.normalize()?;
        Ok(bit)
    }

    pub(super) fn decode_direct_bits(&mut self, count: u32) -> Result<u32, Error> {
        let mut result = 0u32;
        for _ in 0..count {
            self.range >>= 1;
            let mut bit = 0;
            if self.code >= self.range {
                self.code -= self.range;
                bit = 1;
            }
            result = result << 1 | bit;
            self.normalize()?;
        }
        Ok(result)
    }
}

pub(super) struct RangeEncoder {
    output: Vec<u8>,
    low: u64,
    range: u32,
    cache: u8,
    cache_size: usize,
}

impl RangeEncoder {
    pub(super) fn with_output(mut output: Vec<u8>) -> Self {
        output.clear();
        Self {
            output,
            low: 0,
            range: u32::MAX,
            cache: 0,
            cache_size: 1,
        }
    }

    pub(super) fn encode_bit(&mut self, probability: &mut u16, bit: u32) {
        let bound = (self.range >> 11) * u32::from(*probability);
        if bit == 0 {
            self.range = bound;
            *probability += (PROB_TOTAL as u16 - *probability) >> MOVE_BITS;
        } else {
            self.low += u64::from(bound);
            self.range -= bound;
            *probability -= *probability >> MOVE_BITS;
        }
        if self.range < TOP_VALUE {
            self.range <<= 8;
            self.shift_low();
        }
    }

    pub(super) fn encode_direct_bits(&mut self, value: u32, count: u32) {
        for bit_index in (0..count).rev() {
            self.range >>= 1;
            if value >> bit_index & 1 != 0 {
                self.low += u64::from(self.range);
            }
            if self.range < TOP_VALUE {
                self.range <<= 8;
                self.shift_low();
            }
        }
    }

    fn shift_low(&mut self) {
        let low32 = self.low as u32;
        let carry = (self.low >> 32) as u8;
        if low32 < 0xff00_0000 || carry != 0 {
            let mut byte = self.cache;
            loop {
                self.output.push(byte.wrapping_add(carry));
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
                byte = 0xff;
            }
            self.cache = (low32 >> 24) as u8;
        }
        self.cache_size += 1;
        self.low = u64::from(low32 << 8);
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.output
    }
}
