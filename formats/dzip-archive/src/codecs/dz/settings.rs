use crate::codecs::dz::{DzError, Result};
use alloc::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeSettings {
    pub win_size: u8,
    pub flags: u8,
    pub offset_table_size: u8,
    pub offset_tables: u8,
    pub offset_contexts: u8,
    pub ref_length_table_size: u8,
    pub ref_length_tables: u8,
    pub ref_offset_table_size: u8,
    pub ref_offset_tables: u8,
    pub big_min_match: u8,
}

impl Default for RangeSettings {
    fn default() -> Self {
        Self {
            win_size: 16,
            flags: 1,
            offset_table_size: 8,
            offset_tables: 3,
            offset_contexts: 3,
            ref_length_table_size: 7,
            ref_length_tables: 1,
            ref_offset_table_size: 7,
            ref_offset_tables: 3,
            big_min_match: 15,
        }
    }
}

impl RangeSettings {
    pub const USE_COMBUF_STATIC_TABLES: u8 = 1;
    pub const USE_DZ_STATIC_TABLES: u8 = 2;

    pub fn validate(self) -> Result<Self> {
        if self.win_size > 30 {
            return Err(DzError::invalid_options(format!(
                "WinSize {} exceeds 30",
                self.win_size
            )));
        }
        if self.flags > 3 {
            return Err(DzError::invalid_options(format!(
                "unsupported RangeSettings flags {:#x}",
                self.flags
            )));
        }
        if self.flags & Self::USE_DZ_STATIC_TABLES != 0 {
            return Err(DzError::invalid_options(
                "DZ static tables are rejected by dzip.exe",
            ));
        }
        if self.offset_contexts == 0 || self.offset_contexts > 8 {
            return Err(DzError::invalid_options(format!(
                "OffsetContexts {} is outside 1..=8",
                self.offset_contexts
            )));
        }
        if self.offset_table_size == 0 || self.offset_table_size > 15 {
            return Err(DzError::invalid_options(format!(
                "OffsetTableSize {} is outside 1..=15",
                self.offset_table_size
            )));
        }
        for (name, bits) in [
            ("RefLengthTableSize", self.ref_length_table_size),
            ("RefOffsetTableSize", self.ref_offset_table_size),
        ] {
            if bits > 15 {
                return Err(DzError::invalid_options(format!(
                    "{name} {bits} exceeds 15"
                )));
            }
        }
        if self.offset_tables == 0 {
            return Err(DzError::invalid_options("OffsetTables must not be zero"));
        }
        Ok(self)
    }

    pub fn combuf_static_prefix_size(self) -> usize {
        if self.flags & Self::USE_COMBUF_STATIC_TABLES == 0 {
            return 0;
        }
        514 + usize::from(self.offset_tables)
            * usize::from(self.offset_contexts)
            * (1usize << self.offset_table_size)
    }
}
