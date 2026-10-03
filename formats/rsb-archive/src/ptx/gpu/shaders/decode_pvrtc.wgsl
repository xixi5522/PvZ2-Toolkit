struct Params {
    width: u32,
    height: u32,
    alpha_offset: u32,
    reserved: u32,
    blocks_x: u32,
    blocks_y: u32,
    output_stride_words: u32,
    alpha_mode: u32,
}

@group(0) @binding(0) var<storage, read> packets: array<vec2<u32>>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

const FACTORS: array<vec4<i32>, 16> = array(
    vec4<i32>(4, 4, 4, 4), vec4<i32>(2, 6, 2, 6),
    vec4<i32>(8, 0, 8, 0), vec4<i32>(6, 2, 6, 2),
    vec4<i32>(2, 2, 6, 6), vec4<i32>(1, 3, 3, 9),
    vec4<i32>(4, 0, 12, 0), vec4<i32>(3, 1, 9, 3),
    vec4<i32>(8, 8, 0, 0), vec4<i32>(4, 12, 0, 0),
    vec4<i32>(16, 0, 0, 0), vec4<i32>(12, 4, 0, 0),
    vec4<i32>(6, 6, 2, 2), vec4<i32>(3, 9, 1, 3),
    vec4<i32>(12, 0, 4, 0), vec4<i32>(9, 3, 3, 1),
);

const WEIGHTS: array<u32, 32> = array(
    8u, 0u, 8u, 0u, 5u, 3u, 5u, 3u,
    3u, 5u, 3u, 5u, 0u, 8u, 0u, 8u,
    8u, 0u, 8u, 0u, 4u, 4u, 4u, 4u,
    4u, 4u, 0u, 0u, 0u, 8u, 0u, 8u,
);

fn read_byte(offset: u32) -> u32 {
    let packet = packets[offset >> 3u];
    let word = select(packet.x, packet.y, (offset & 4u) != 0u);
    return (word >> ((offset & 3u) * 8u)) & 0xffu;
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

fn color_a(packet: vec2<u32>) -> vec4<i32> {
    let word = packet.y;
    let color = (word >> 1u) & 0x3fffu;
    if (((word >> 15u) & 1u) != 0u) {
        let r = i32(color >> 9u);
        let g = i32((color >> 4u) & 0x1fu);
        let b = i32(color & 0xfu);
        return vec4<i32>((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 4) | b, 255);
    }
    let a = i32((color >> 11u) & 7u);
    let r = i32((color >> 7u) & 0xfu);
    let g = i32((color >> 3u) & 0xfu);
    let b = i32(color & 7u);
    return vec4<i32>(
        (r << 4) | r, (g << 4) | g,
        (b << 5) | (b << 2) | (b >> 1),
        (a << 5) | (a << 2) | (a >> 1),
    );
}

fn color_b(packet: vec2<u32>) -> vec4<i32> {
    let word = packet.y;
    let color = (word >> 16u) & 0x7fffu;
    if (((word >> 31u) & 1u) != 0u) {
        let r = i32(color >> 10u);
        let g = i32((color >> 5u) & 0x1fu);
        let b = i32(color & 0x1fu);
        return vec4<i32>((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 3) | (b >> 2), 255);
    }
    let a = i32((color >> 12u) & 7u);
    let r = i32((color >> 8u) & 0xfu);
    let g = i32((color >> 4u) & 0xfu);
    let b = i32(color & 0xfu);
    return vec4<i32>(
        (r << 4) | r, (g << 4) | g, (b << 4) | b,
        (a << 5) | (a << 2) | (a >> 1),
    );
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    let block_x = gid.x / 4u;
    let block_y = gid.y / 4u;
    let local_x = gid.x & 3u;
    let local_y = gid.y & 3u;
    let mask_x = params.blocks_x - 1u;
    let mask_y = params.blocks_y - 1u;
    let offset_x = select(0xffffffffu, 0u, local_x >= 2u);
    let offset_y = select(0xffffffffu, 0u, local_y >= 2u);
    let x0 = (block_x + offset_x) & mask_x;
    let y0 = (block_y + offset_y) & mask_y;
    let x1 = (x0 + 1u) & mask_x;
    let y1 = (y0 + 1u) & mask_y;
    let p0 = packets[morton_rect(x0, y0)];
    let p1 = packets[morton_rect(x1, y0)];
    let p2 = packets[morton_rect(x0, y1)];
    let p3 = packets[morton_rect(x1, y1)];
    let factors = FACTORS[local_y * 4u + local_x];
    let ca = color_a(p0) * factors.x + color_a(p1) * factors.y
        + color_a(p2) * factors.z + color_a(p3) * factors.w;
    let cb = color_b(p0) * factors.x + color_b(p1) * factors.y
        + color_b(p2) * factors.z + color_b(p3) * factors.w;

    let packet = packets[morton_rect(block_x, block_y)];
    let modulation = (packet.x >> ((local_y * 4u + local_x) * 2u)) & 3u;
    let punch = (packet.y & 1u) != 0u;
    let weight_index = select(0u, 16u, punch) + modulation * 4u;
    let color = (ca * i32(WEIGHTS[weight_index])
        + cb * i32(WEIGHTS[weight_index + 1u])) / vec4<i32>(128);
    var alpha = (ca.a * i32(WEIGHTS[weight_index + 2u])
        + cb.a * i32(WEIGHTS[weight_index + 3u])) >> 7;
    if (params.alpha_mode == 1u) {
        alpha = i32(read_byte(params.alpha_offset + gid.y * params.width + gid.x));
    }
    output_pixels[gid.y * params.output_stride_words + gid.x] =
        u32(clamp(color.r, 0, 255))
        | (u32(clamp(color.g, 0, 255)) << 8u)
        | (u32(clamp(color.b, 0, 255)) << 16u)
        | (u32(clamp(alpha, 0, 255)) << 24u);
}
