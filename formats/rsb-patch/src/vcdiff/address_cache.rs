use crate::Result;

use super::invalid;

pub(super) const DEFAULT_NEAR_CACHE_SIZE: usize = 4;
pub(super) const DEFAULT_SAME_CACHE_COUNT: usize = 3;
const DEFAULT_SAME_CACHE_SIZE: usize = DEFAULT_SAME_CACHE_COUNT * 256;

#[derive(Debug)]
pub(super) struct AddressEncoder {
    near: [usize; DEFAULT_NEAR_CACHE_SIZE],
    same: [usize; DEFAULT_SAME_CACHE_SIZE],
    next_near: usize,
}

impl Default for AddressEncoder {
    fn default() -> Self {
        Self {
            near: [0; DEFAULT_NEAR_CACHE_SIZE],
            same: [0; DEFAULT_SAME_CACHE_SIZE],
            next_near: 0,
        }
    }
}

impl AddressEncoder {
    pub fn encode(&mut self, address: usize, here: usize) -> (u8, usize) {
        let same_position = address % DEFAULT_SAME_CACHE_SIZE;
        if self.same[same_position] == address {
            self.update(address);
            return ((6 + same_position / 256) as u8, same_position % 256);
        }

        let mut mode = 0_u8;
        let mut encoded = address;
        let here_encoded = here - address;
        if here_encoded < encoded {
            mode = 1;
            encoded = here_encoded;
        }
        for (index, near) in self.near.iter().enumerate() {
            if let Some(near_encoded) = address.checked_sub(*near)
                && near_encoded < encoded
            {
                mode = (index + 2) as u8;
                encoded = near_encoded;
            }
        }
        self.update(address);
        (mode, encoded)
    }

    fn update(&mut self, address: usize) {
        self.near[self.next_near] = address;
        self.next_near = (self.next_near + 1) % DEFAULT_NEAR_CACHE_SIZE;
        self.same[address % DEFAULT_SAME_CACHE_SIZE] = address;
    }
}

#[derive(Debug)]
pub(super) struct AddressCache {
    near: Vec<usize>,
    same: Vec<usize>,
    next_near: usize,
}

impl AddressCache {
    pub fn new(near_cache_size: usize, same_cache_size: usize) -> Self {
        Self {
            near: vec![0; near_cache_size],
            same: vec![0; same_cache_size * 256],
            next_near: 0,
        }
    }

    pub fn is_same_mode(&self, mode: u8) -> bool {
        usize::from(mode) >= 2 + self.near.len()
    }

    pub fn decode(&mut self, mode: u8, encoded: usize, here: usize) -> Result<usize> {
        let mode = usize::from(mode);
        let first_same = 2 + self.near.len();
        let address = if mode == 0 {
            encoded
        } else if mode == 1 {
            here.checked_sub(encoded)
                .ok_or_else(|| invalid("HERE address underflow"))?
        } else if mode < first_same {
            self.near[mode - 2]
                .checked_add(encoded)
                .ok_or_else(|| invalid("NEAR address overflow"))?
        } else {
            let same_index = (mode - first_same)
                .checked_mul(256)
                .and_then(|index| index.checked_add(encoded))
                .ok_or_else(|| invalid("SAME address cache index overflows usize"))?;
            *self
                .same
                .get(same_index)
                .ok_or_else(|| invalid(format!("invalid COPY mode {mode}")))?
        };
        if !self.near.is_empty() {
            self.near[self.next_near] = address;
            self.next_near = (self.next_near + 1) % self.near.len();
        }
        if !self.same.is_empty() {
            let index = address % self.same.len();
            self.same[index] = address;
        }
        Ok(address)
    }
}
