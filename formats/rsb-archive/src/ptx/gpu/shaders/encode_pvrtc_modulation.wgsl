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

fn unpack(pixel: u32) -> vec4<i32> {
    return vec4<i32>(i32(pixel & 0xffu), i32((pixel >> 8u) & 0xffu), i32((pixel >> 16u) & 0xffu), i32(pixel >> 24u));
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
    let color = (packet.y >> 1u) & 0x3fffu;
    if (((packet.y >> 15u) & 1u) != 0u) {
        let r = i32(color >> 9u); let g = i32((color >> 4u) & 0x1fu); let b = i32(color & 0xfu);
        return vec4<i32>((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 4) | b, 255);
    }
    let a = i32((color >> 11u) & 7u); let r = i32((color >> 7u) & 0xfu);
    let g = i32((color >> 3u) & 0xfu); let b = i32(color & 7u);
    return vec4<i32>((r << 4) | r, (g << 4) | g, (b << 5) | (b << 2) | (b >> 1), (a << 5) | (a << 2) | (a >> 1));
}

fn color_b(packet: vec2<u32>) -> vec4<i32> {
    let color = (packet.y >> 16u) & 0x7fffu;
    if (((packet.y >> 31u) & 1u) != 0u) {
        let r = i32(color >> 10u); let g = i32((color >> 5u) & 0x1fu); let b = i32(color & 0x1fu);
        return vec4<i32>((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 3) | (b >> 2), 255);
    }
    let a = i32((color >> 12u) & 7u); let r = i32((color >> 8u) & 0xfu);
    let g = i32((color >> 4u) & 0xfu); let b = i32(color & 0xfu);
    return vec4<i32>((r << 4) | r, (g << 4) | g, (b << 4) | b, (a << 5) | (a << 2) | (a >> 1));
}

fn dot4(a: vec4<i32>, b: vec4<i32>) -> i32 {
    return a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.blocks_x || gid.y >= params.blocks_y) {
        return;
    }
    let mask_x = params.blocks_x - 1u;
    let mask_y = params.blocks_y - 1u;
    var modulation_data = 0u;
    for (var py = 0u; py < 4u; py += 1u) {
        let y0 = (gid.y + select(0xffffffffu, 0u, py >= 2u)) & mask_y;
        let y1 = (y0 + 1u) & mask_y;
        for (var px = 0u; px < 4u; px += 1u) {
            let x0 = (gid.x + select(0xffffffffu, 0u, px >= 2u)) & mask_x;
            let x1 = (x0 + 1u) & mask_x;
            let factor = FACTORS[py * 4u + px];
            let p0 = packets[morton_rect(x0, y0)];
            let p1 = packets[morton_rect(x1, y0)];
            let p2 = packets[morton_rect(x0, y1)];
            let p3 = packets[morton_rect(x1, y1)];
            let ca = color_a(p0) * factor.x + color_a(p1) * factor.y
                + color_a(p2) * factor.z + color_a(p3) * factor.w;
            let cb = color_b(p0) * factor.x + color_b(p1) * factor.y
                + color_b(p2) * factor.z + color_b(p3) * factor.w;
            var source = unpack(input_pixels[(gid.y * 4u + py) * params.width + gid.x * 4u + px]) * 16;
            if ((params.flags & 1u) == 0u) { source.a = 255 * 16; }
            let direction = cb - ca;
            let relative = source - ca;
            let projection = dot4(relative, direction) * 16;
            let length_squared = dot4(direction, direction);
            var modulation = 0u;
            if (projection > 3 * length_squared) { modulation += 1u; }
            if (projection > 8 * length_squared) { modulation += 1u; }
            if (projection > 13 * length_squared) { modulation += 1u; }
            modulation_data |= modulation << ((py * 4u + px) * 2u);
        }
    }
    let index = morton_rect(gid.x, gid.y);
    packets[index] = vec2<u32>(modulation_data, packets[index].y);
}
