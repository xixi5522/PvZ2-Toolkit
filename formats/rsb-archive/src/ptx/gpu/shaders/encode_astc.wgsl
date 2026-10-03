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
@group(0) @binding(1) var<storage, read_write> output_blocks: array<vec4<u32>>;
@group(0) @binding(2) var<uniform> params: Params;

fn unpack(pixel: u32) -> vec4<u32> {
    return vec4<u32>(pixel & 0xffu, (pixel >> 8u) & 0xffu, (pixel >> 16u) & 0xffu, pixel >> 24u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.blocks_x || gid.y >= params.blocks_y) {
        return;
    }
    var sum = vec4<u32>(0u);
    for (var y = 0u; y < params.block_height; y += 1u) {
        for (var x = 0u; x < params.block_width; x += 1u) {
            let source_x = min(gid.x * params.block_width + x, params.width - 1u);
            let source_y = min(gid.y * params.block_height + y, params.height - 1u);
            sum += unpack(input_pixels[source_y * params.width + source_x]);
        }
    }
    let count = params.block_width * params.block_height;
    let color = (sum + vec4<u32>(count / 2u)) / vec4<u32>(count);
    let red = color.r * 0x101u;
    let green = color.g * 0x101u;
    let blue = color.b * 0x101u;
    let alpha = color.a * 0x101u;
    let index = gid.y * params.blocks_x + gid.x;
    output_blocks[index] = vec4<u32>(
        0xfffffdfcu,
        0xffffffffu,
        red | (green << 16u),
        blue | (alpha << 16u),
    );
}
