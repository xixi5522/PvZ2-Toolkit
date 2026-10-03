pub(crate) const PAK_MAGIC: u32 = 0xBAC0_4AC0;
pub(crate) const PAK_VERSION: u32 = 0;
pub(crate) const PC_XOR_KEY: u8 = 0xF7;
pub(crate) const PC_MAGIC: u32 = PAK_MAGIC ^ u32::from_le_bytes([PC_XOR_KEY; 4]);
