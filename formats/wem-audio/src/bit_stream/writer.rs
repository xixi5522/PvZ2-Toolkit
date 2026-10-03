pub struct BitWriter {
    buffer: Vec<u8>,
    bit_buffer: u8,
    bits_stored: u8,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            bit_buffer: 0,
            bits_stored: 0,
        }
    }

    pub fn write_bit(&mut self, bit: bool) {
        if bit {
            self.bit_buffer |= 1 << self.bits_stored;
        }
        self.bits_stored += 1;

        if self.bits_stored == 8 {
            self.buffer.push(self.bit_buffer);
            self.bit_buffer = 0;
            self.bits_stored = 0;
        }
    }

    pub fn write_bits(&mut self, value: u32, count: u8) {
        for i in 0..count {
            self.write_bit((value & (1 << i)) != 0);
        }
    }

    pub fn into_inner(mut self) -> Vec<u8> {
        if self.bits_stored > 0 {
            self.buffer.push(self.bit_buffer);
        }
        self.buffer
    }
}
