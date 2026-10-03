use crate::codecs::dz::{DzError as DzipError, Result};
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};

const TOP: u32 = 0x0100_0000;
const BOTTOM: u32 = 0x0001_0000;
const MODEL_INCREMENT: u16 = 16;
const MODEL_MAX_TOTAL: u32 = 0x0001_0000;

#[derive(Clone, Debug)]
pub(crate) struct AdaptiveModel {
    frequencies: Vec<u16>,
    fenwick: Vec<u32>,
    total: u32,
    increment: u16,
}

impl AdaptiveModel {
    pub(crate) fn new(symbols: usize) -> Result<Self> {
        Self::new_with_increment(symbols, MODEL_INCREMENT)
    }

    pub(crate) fn new_with_increment(symbols: usize, increment: u16) -> Result<Self> {
        if symbols == 0 || symbols > u16::MAX as usize {
            return Err(DzipError::InvalidDz(format!(
                "invalid adaptive model size {}",
                symbols
            )));
        }
        let mut model = Self {
            frequencies: vec![1; symbols],
            fenwick: vec![0; symbols + 1],
            total: symbols as u32,
            increment,
        };
        model.rebuild_fenwick();
        Ok(model)
    }

    pub(crate) fn from_normalized(frequencies: &[u8]) -> Result<Self> {
        Self::from_normalized_with_increment(frequencies, MODEL_INCREMENT)
    }

    pub(crate) fn from_normalized_with_increment(
        frequencies: &[u8],
        increment: u16,
    ) -> Result<Self> {
        if frequencies.is_empty() {
            return Err(DzipError::InvalidDz(
                "empty static frequency table".to_string(),
            ));
        }
        let mut model = Self {
            frequencies: frequencies
                .iter()
                .map(|&frequency| u16::from(frequency.max(1)))
                .collect(),
            fenwick: vec![0; frequencies.len() + 1],
            total: frequencies
                .iter()
                .map(|&frequency| u32::from(frequency.max(1)))
                .sum(),
            increment,
        };
        model.rebuild_fenwick();
        Ok(model)
    }

    pub(crate) fn symbol_count(&self) -> usize {
        self.frequencies.len()
    }

    pub(crate) fn total(&self) -> u32 {
        self.total
    }

    pub(crate) fn estimated_cost(&self, symbol: usize) -> Result<i32> {
        let frequency = self.frequencies.get(symbol).copied().ok_or_else(|| {
            DzipError::InvalidDz(format!(
                "symbol {} outside model of {} symbols",
                symbol,
                self.symbol_count()
            ))
        })?;
        if frequency == 0 {
            return Ok(-1);
        }
        let bit_length = |value: u32| (u32::BITS - value.leading_zeros()) as i32;
        Ok(bit_length(self.total >> 1) - bit_length(u32::from(frequency) >> 1))
    }

    pub(crate) fn interval(&self, symbol: usize) -> Result<(u32, u32)> {
        let frequency = self.frequencies.get(symbol).copied().ok_or_else(|| {
            DzipError::InvalidDz(format!(
                "symbol {} outside model of {} symbols",
                symbol,
                self.symbol_count()
            ))
        })?;
        Ok((self.prefix_sum(symbol), u32::from(frequency)))
    }

    pub(crate) fn symbol_for_count(&self, count: u32) -> Result<(usize, u32, u32)> {
        if count >= self.total {
            return Err(DzipError::InvalidDz(format!(
                "range count {} exceeds model total {}",
                count, self.total
            )));
        }

        let mut index = 0usize;
        let mut accumulated = 0u32;
        let mut bit = 1usize;
        while bit < self.fenwick.len() {
            bit <<= 1;
        }
        bit >>= 1;

        while bit != 0 {
            let next = index + bit;
            if next < self.fenwick.len() && accumulated.saturating_add(self.fenwick[next]) <= count
            {
                accumulated += self.fenwick[next];
                index = next;
            }
            bit >>= 1;
        }

        let symbol = index;
        let frequency = u32::from(self.frequencies[symbol]);
        Ok((symbol, accumulated, frequency))
    }

    pub(crate) fn update(&mut self, symbol: usize) {
        let increment = u32::from(self.increment);
        self.frequencies[symbol] = self.frequencies[symbol].wrapping_add(self.increment);
        self.total += increment;
        self.add_fenwick(symbol, increment);

        if self.total > MODEL_MAX_TOTAL {
            self.total = 0;
            for frequency in &mut self.frequencies {
                *frequency -= *frequency >> 1;
                self.total += u32::from(*frequency);
            }
            self.rebuild_fenwick();
        }
    }

    fn prefix_sum(&self, symbol: usize) -> u32 {
        let mut index = symbol;
        let mut sum = 0u32;
        while index != 0 {
            sum += self.fenwick[index];
            index &= index - 1;
        }
        sum
    }

    // Keep the bit-twiddling form compatible with stable compilers that predate
    // `usize::isolate_lowest_one`.
    #[allow(unknown_lints)]
    #[allow(clippy::manual_isolate_lowest_one)]
    fn add_fenwick(&mut self, symbol: usize, amount: u32) {
        let mut index = symbol + 1;
        while index < self.fenwick.len() {
            self.fenwick[index] += amount;
            index += index & index.wrapping_neg();
        }
    }

    fn rebuild_fenwick(&mut self) {
        self.fenwick.fill(0);
        for symbol in 0..self.frequencies.len() {
            self.add_fenwick(symbol, u32::from(self.frequencies[symbol]));
        }
    }
}

pub(crate) struct RangeDecoder<'a> {
    input: &'a [u8],
    position: usize,
    low: u32,
    code: u32,
    range: u32,
}

impl<'a> RangeDecoder<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Result<Self> {
        let mut decoder = Self {
            input,
            position: 0,
            low: 0,
            code: 0,
            range: u32::MAX,
        };
        for _ in 0..4 {
            decoder.code = decoder.code.wrapping_shl(8) | u32::from(decoder.read_byte());
        }
        Ok(decoder)
    }

    pub(crate) fn decode(&mut self, model: &mut AdaptiveModel) -> Result<usize> {
        let unit = self.range / model.total();
        if unit == 0 {
            return Err(DzipError::InvalidDz(
                "range coder interval collapsed".to_string(),
            ));
        }
        let count = self.code.wrapping_sub(self.low) / unit;
        let (symbol, cumulative, frequency) = model.symbol_for_count(count)?;

        self.low = self.low.wrapping_add(unit.wrapping_mul(cumulative));
        self.range = unit.wrapping_mul(frequency);
        model.update(symbol);
        self.renormalize();
        Ok(symbol)
    }

    pub(crate) fn consumed(&self) -> usize {
        self.position
    }

    /// Return the byte position immediately after the minimal range-coded
    /// stream terminator.
    ///
    /// The decoder keeps up to four bytes of look-ahead in `code`.  dzip
    /// dzip.exe's `sub_408C20` derives the shortest midpoint representation of
    /// the final interval and seeks backwards over the unused look-ahead
    /// before starting the next COMBUF sub-stream.
    pub(crate) fn finished_position(&self) -> usize {
        let midpoint = self.low.wrapping_add(self.range >> 1);
        let high = self.low.wrapping_add(self.range).wrapping_sub(1);
        let mut mask = 0xff00_0000u32;
        let mut differs_from_low = (midpoint & mask) != (self.low & mask);
        let mut differs_from_high = (midpoint & mask) != (high & mask);
        let mut encoded_bytes = 1usize;

        while !differs_from_low || !differs_from_high {
            mask >>= 8;
            encoded_bytes += 1;
            differs_from_low |= (midpoint & mask) != (self.low & mask);
            differs_from_high |= (midpoint & mask) != (high & mask);
        }

        self.position
            .saturating_sub(4usize.saturating_sub(encoded_bytes))
    }

    fn renormalize(&mut self) {
        loop {
            if (self.low ^ self.low.wrapping_add(self.range)) < TOP {
                // Stable high byte.
            } else if self.range >= BOTTOM {
                break;
            } else {
                self.range = u32::from(0u16.wrapping_sub(self.low as u16));
            }
            self.range = self.range.wrapping_shl(8);
            self.code = self.code.wrapping_shl(8) | u32::from(self.read_byte());
            self.low = self.low.wrapping_shl(8);
        }
    }

    fn read_byte(&mut self) -> u8 {
        let value = self.input.get(self.position).copied().unwrap_or(0xff);
        self.position = self.position.saturating_add(1);
        value
    }
}

#[derive(Default)]
pub(crate) struct RangeEncoder {
    output: Vec<u8>,
    low: u32,
    range: u32,
}

impl RangeEncoder {
    pub(crate) fn new() -> Self {
        Self::with_output(Vec::new())
    }

    pub(crate) fn with_output(mut output: Vec<u8>) -> Self {
        output.clear();
        Self {
            output,
            low: 0,
            range: u32::MAX,
        }
    }

    pub(crate) fn encode(&mut self, model: &mut AdaptiveModel, symbol: usize) -> Result<()> {
        let (cumulative, frequency) = model.interval(symbol)?;
        let unit = self.range / model.total();
        if unit == 0 {
            return Err(DzipError::InvalidDz(
                "range coder interval collapsed".to_string(),
            ));
        }
        self.range = unit;
        self.low = self.low.wrapping_add(cumulative.wrapping_mul(unit));
        self.range = self.range.wrapping_mul(frequency);
        self.renormalize();
        model.update(symbol);
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        let midpoint = self.low.wrapping_add(self.range >> 1);
        let high = self.low.wrapping_add(self.range).wrapping_sub(1);
        let original_low = self.low;
        let mut mask = 0xff00_0000u32;
        let mut shift = 24u32;
        let mut differs_from_low = (midpoint & mask) != (original_low & mask);
        let mut differs_from_high = (midpoint & mask) != (high & mask);

        while !differs_from_low || !differs_from_high {
            self.output.push((midpoint >> shift) as u8);
            shift = shift.saturating_sub(8);
            mask >>= 8;
            differs_from_low |= (midpoint & mask) != (original_low & mask);
            differs_from_high |= (midpoint & mask) != (high & mask);
        }
        self.output.push((midpoint >> shift) as u8);
        self.output
    }

    fn renormalize(&mut self) {
        loop {
            if (self.low ^ self.low.wrapping_add(self.range)) < TOP {
                // Stable high byte.
            } else if self.range >= BOTTOM {
                break;
            } else {
                self.range = u32::from(0u16.wrapping_sub(self.low as u16));
            }
            self.output.push((self.low >> 24) as u8);
            self.low = self.low.wrapping_shl(8);
            self.range = self.range.wrapping_shl(8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_range_round_trip() {
        let symbols = [0usize, 1, 2, 255, 2, 2, 1, 0, 255, 128, 64, 2, 513];
        let mut encoder = RangeEncoder::new();
        let mut encode_model = AdaptiveModel::new(514).unwrap();
        for &symbol in &symbols {
            encoder.encode(&mut encode_model, symbol).unwrap();
        }
        let encoded = encoder.finish();

        let mut decoder = RangeDecoder::new(&encoded).unwrap();
        let mut decode_model = AdaptiveModel::new(514).unwrap();
        let decoded: Vec<_> = (0..symbols.len())
            .map(|_| decoder.decode(&mut decode_model).unwrap())
            .collect();
        assert_eq!(decoded, symbols);
    }
}
