struct Params {
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    blocks_x: u32,
    blocks_y: u32,
    output_offset_words: u32,
    flags: u32,
}

@group(0) @binding(0) var<storage, read> input_pixels: array<u32>;
@group(0) @binding(1) var<storage, read_write> packets: array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params: Params;

fn unpack(pixel: u32) -> vec4<u32> {
    return vec4<u32>(pixel & 0xffu, (pixel >> 8u) & 0xffu, (pixel >> 16u) & 0xffu, pixel >> 24u);
}

fn spread_bits(value: u32) -> u32 {
    var result = value & 0x0000ffffu;
    result = (result | (result << 8u)) & 0x00ff00ffu;
    result = (result | (result << 4u)) & 0x0f0f0f0fu;
    result = (result | (result << 2u)) & 0x33333333u;
    result = (result | (result << 1u)) & 0x55555555u;
    return result;
}

fn morton_rect(x: u32, y: u32) -> u32 {
    let minimum = min(params.blocks_x, params.blocks_y);
    let mask = minimum - 1u;
    let low = (spread_bits(x & mask) << 1u) | spread_bits(y & mask);
    var bits = 0u;
    var extent = minimum;
    loop {
        if (extent <= 1u) { break; }
        extent >>= 1u;
        bits += 1u;
    }
    let high = select(y >> bits, x >> bits, params.blocks_x > params.blocks_y);
    return low | (high << (bits * 2u));
}

fn quantize_floor(value: u32, bits: u32) -> u32 {
    return value * ((1u << bits) - 1u) / 255u;
}

fn quantize_ceil(value: u32, bits: u32) -> u32 {
    return (value * ((1u << bits) - 1u) + 254u) / 255u;
}

fn pack_a(color: vec4<u32>) -> u32 {
    let include_alpha = (params.flags & 1u) != 0u;
    let alpha = select(7u, quantize_floor(color.a, 3u), include_alpha);
    if (alpha == 7u) {
        let packed = (quantize_floor(color.r, 5u) << 9u)
            | (quantize_floor(color.g, 5u) << 4u)
            | quantize_floor(color.b, 4u);
        return (packed << 1u) | (1u << 15u);
    }
    let packed = (alpha << 11u) | (quantize_floor(color.r, 4u) << 7u)
        | (quantize_floor(color.g, 4u) << 3u) | quantize_floor(color.b, 3u);
    return packed << 1u;
}

fn pack_b(color: vec4<u32>) -> u32 {
    let include_alpha = (params.flags & 1u) != 0u;
    let alpha = select(7u, quantize_ceil(color.a, 3u), include_alpha);
    if (alpha == 7u) {
        let packed = (quantize_ceil(color.r, 5u) << 10u)
            | (quantize_ceil(color.g, 5u) << 5u)
            | quantize_ceil(color.b, 5u);
        return (packed << 16u) | (1u << 31u);
    }
    let packed = (alpha << 12u) | (quantize_ceil(color.r, 4u) << 8u)
        | (quantize_ceil(color.g, 4u) << 4u) | quantize_ceil(color.b, 4u);
    return packed << 16u;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.blocks_x || gid.y >= params.blocks_y) {
        return;
    }
    let first = unpack(input_pixels[(gid.y * 4u) * params.width + gid.x * 4u]);
    var minimum = first;
    var maximum = first;
    for (var y = 0u; y < 4u; y += 1u) {
        for (var x = 0u; x < 4u; x += 1u) {
            let color = unpack(input_pixels[(gid.y * 4u + y) * params.width + gid.x * 4u + x]);
            minimum = min(minimum, color);
            maximum = max(maximum, color);
        }
    }
    if ((params.flags & 1u) == 0u) {
        minimum.a = 255u;
        maximum.a = 255u;
    }
    packets[morton_rect(gid.x, gid.y)] = vec2<u32>(0u, pack_a(minimum) | pack_b(maximum));
}
