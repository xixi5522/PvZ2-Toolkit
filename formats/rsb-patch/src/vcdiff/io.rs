use crate::Result;

use super::invalid;

#[derive(Clone, Copy)]
pub(super) struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub fn remaining(self) -> usize {
        self.data.len() - self.position
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or_else(|| invalid("unexpected end of VCDIFF data"))?;
        self.position += 1;
        Ok(byte)
    }

    pub fn read_exact(&mut self, size: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(size)
            .ok_or_else(|| invalid("section length overflows usize"))?;
        let bytes = self
            .data
            .get(self.position..end)
            .ok_or_else(|| invalid("unexpected end of VCDIFF data"))?;
        self.position = end;
        Ok(bytes)
    }

    pub fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        self.read_exact(size)
    }

    pub fn take_cursor(&mut self, size: usize) -> Result<Self> {
        Ok(Self::new(self.take(size)?))
    }

    pub fn read_varint(&mut self) -> Result<usize> {
        let mut value = 0_usize;
        for _ in 0..10 {
            let byte = self.read_u8()?;
            if value > (usize::MAX >> 7) {
                return Err(invalid("VCDIFF varint overflows usize"));
            }
            value = (value << 7) | usize::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(invalid("VCDIFF varint is too long"))
    }

    pub fn finish(self, section: &str) -> Result<()> {
        if self.remaining() != 0 {
            return Err(invalid(format!(
                "{section} has {} unconsumed bytes",
                self.remaining()
            )));
        }
        Ok(())
    }
}

pub(super) fn write_varint(mut value: usize, output: &mut Vec<u8>) {
    let mut bytes = [0_u8; 10];
    let mut index = bytes.len() - 1;
    bytes[index] = (value & 0x7F) as u8;
    value >>= 7;
    while value != 0 {
        index -= 1;
        bytes[index] = ((value & 0x7F) as u8) | 0x80;
        value >>= 7;
    }
    output.extend_from_slice(&bytes[index..]);
}

pub(super) fn checked_segment<'a>(
    data: &'a [u8],
    position: usize,
    length: usize,
    name: &str,
) -> Result<&'a [u8]> {
    let end = position
        .checked_add(length)
        .ok_or_else(|| invalid(format!("{name} segment overflows usize")))?;
    data.get(position..end)
        .ok_or_else(|| invalid(format!("{name} segment is out of bounds")))
}
