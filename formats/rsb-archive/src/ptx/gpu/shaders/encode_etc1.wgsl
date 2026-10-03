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
@group(0) @binding(1) var<storage, read_write> output_blocks: array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params: Params;

fn unpack(pixel: u32) -> vec3<i32> {
    if (params.flags == 2u) {
        let alpha = i32((pixel >> 24u) & 0xffu);
        return vec3<i32>(alpha);
    }
    return vec3<i32>(i32(pixel & 0xffu), i32((pixel >> 8u) & 0xffu), i32((pixel >> 16u) & 0xffu));
}

fn byte_swap(value: u32) -> u32 {
    return ((value & 0x000000ffu) << 24u)
        | ((value & 0x0000ff00u) << 8u)
        | ((value & 0x00ff0000u) >> 8u)
        | ((value & 0xff000000u) >> 24u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.blocks_x || gid.y >= params.blocks_y) {
        return;
    }
    var sum = vec3<i32>(0);
    for (var y = 0u; y < 4u; y += 1u) {
        for (var x = 0u; x < 4u; x += 1u) {
            let sx = min(gid.x * 4u + x, params.width - 1u);
            let sy = min(gid.y * 4u + y, params.height - 1u);
            sum += unpack(input_pixels[sy * params.width + sx]);
        }
    }
    let average = (sum + vec3<i32>(8)) / vec3<i32>(16);
    let quantized = vec3<u32>(clamp((average + vec3<i32>(8)) / vec3<i32>(17), vec3<i32>(0), vec3<i32>(15)));
    let base = vec3<i32>(quantized) * 17;
    var high = (quantized.r << 28u) | (quantized.r << 24u)
        | (quantized.g << 20u) | (quantized.g << 16u)
        | (quantized.b << 12u) | (quantized.b << 8u);
    var low = 0u;
    for (var y = 0u; y < 4u; y += 1u) {
        for (var x = 0u; x < 4u; x += 1u) {
            let sx = min(gid.x * 4u + x, params.width - 1u);
            let sy = min(gid.y * 4u + y, params.height - 1u);
            let color = unpack(input_pixels[sy * params.width + sx]);
            let difference = ((color.r + color.g + color.b) - (base.r + base.g + base.b)) / 3;
            let negative = difference < 0;
            let magnitude = abs(difference);
            let selector = select(0u, 1u, abs(magnitude - 8) < abs(magnitude - 2));
            let bit = x * 4u + y;
            low |= selector << bit;
            if (negative) {
                low |= 1u << (bit + 16u);
            }
        }
    }
    let index = params.output_offset_words / 2u + gid.y * params.blocks_x + gid.x;
    output_blocks[index] = vec2<u32>(byte_swap(high), byte_swap(low));
}
