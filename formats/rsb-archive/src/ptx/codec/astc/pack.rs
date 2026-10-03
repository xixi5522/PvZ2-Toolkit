pub(super) fn pack_gradient_block(
    mode: u16,
    endpoints: [[u8; 4]; 2],
    first_plane: &[u8],
    second_plane: Option<&[u8]>,
) -> [u8; 16] {
    let mut block = u128::from(mode);
    block |= 12u128 << 13; // One partition, direct RGBA endpoint mode.
    let endpoint_values = [
        endpoints[0][0],
        endpoints[1][0],
        endpoints[0][1],
        endpoints[1][1],
        endpoints[0][2],
        endpoints[1][2],
        endpoints[0][3],
        endpoints[1][3],
    ];
    for (index, value) in endpoint_values.into_iter().enumerate() {
        block |= u128::from(value) << (17 + index * 8);
    }

    let mut weight_stream = 0u128;
    let mut bit = 0;
    for (index, &weight) in first_plane.iter().enumerate() {
        weight_stream |= u128::from(weight) << bit;
        bit += 2;
        if let Some(second_plane) = second_plane {
            weight_stream |= u128::from(second_plane[index]) << bit;
            bit += 2;
        }
    }
    if second_plane.is_some() {
        block |= 3u128 << 90; // Alpha is the second weight plane.
    }
    block |= weight_stream.reverse_bits();
    block.to_le_bytes()
}
