use super::{PtxGpuError, Result};

pub(super) fn usize_to_u32(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| PtxGpuError::DimensionsOverflow)
}

pub(super) fn align_usize(value: usize, alignment: usize) -> Result<usize> {
    value
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or(PtxGpuError::DimensionsOverflow)
}

pub(super) const fn align_u32(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
