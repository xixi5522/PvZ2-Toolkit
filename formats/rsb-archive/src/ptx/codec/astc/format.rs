use crate::error::{Result, RsbError};

/// All two-dimensional ASTC footprints accepted by the specification.
pub const ASTC_BLOCK_SIZES: [(u32, u32); 14] = [
    (4, 4),
    (5, 4),
    (5, 5),
    (6, 5),
    (6, 6),
    (8, 5),
    (8, 6),
    (8, 8),
    (10, 5),
    (10, 6),
    (10, 8),
    (10, 10),
    (12, 10),
    (12, 12),
];

/// Search effort used by the pure Rust ASTC encoder.
///
/// The values retain the `0..=100` API used by Twinning, but describe this
/// encoder's search effort rather than promising byte-identical `astcenc`
/// presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AstcQuality(u8);

impl AstcQuality {
    pub const FASTEST: Self = Self(0);
    pub const FAST: Self = Self(10);
    pub const MEDIUM: Self = Self(60);
    pub const THOROUGH: Self = Self(98);
    pub const VERY_THOROUGH: Self = Self(99);
    pub const EXHAUSTIVE: Self = Self(100);

    pub fn new(value: u8) -> Result<Self> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(RsbError::InvalidAstcQuality(value))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for AstcQuality {
    fn default() -> Self {
        Self::MEDIUM
    }
}

impl TryFrom<u8> for AstcQuality {
    type Error = RsbError;

    fn try_from(value: u8) -> Result<Self> {
        Self::new(value)
    }
}

pub const fn is_valid_astc_block_size(width: u32, height: u32) -> bool {
    matches!(
        (width, height),
        (4, 4)
            | (5, 4)
            | (5, 5)
            | (6, 5)
            | (6, 6)
            | (8, 5)
            | (8, 6)
            | (8, 8)
            | (10, 5)
            | (10, 6)
            | (10, 8)
            | (10, 10)
            | (12, 10)
            | (12, 12)
    )
}

/// Returns the size of a raw, headerless ASTC payload.
pub fn astc_data_size(
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
) -> Result<usize> {
    validate_block_size(block_width, block_height)?;
    let blocks_x = u64::from(width.div_ceil(block_width));
    let blocks_y = u64::from(height.div_ceil(block_height));
    let byte_count = blocks_x
        .checked_mul(blocks_y)
        .and_then(|count| count.checked_mul(16))
        .ok_or_else(|| RsbError::Other("ASTC image dimensions overflow".into()))?;
    usize::try_from(byte_count)
        .map_err(|_| RsbError::Other("ASTC payload does not fit in memory".into()))
}

pub(super) fn validate_block_size(width: u32, height: u32) -> Result<()> {
    if is_valid_astc_block_size(width, height) {
        Ok(())
    } else {
        Err(RsbError::InvalidAstcBlockSize { width, height })
    }
}
