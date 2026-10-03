pub(super) fn crc32(input: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in input {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                crc << 1 ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    !crc
}
