struct Params {
    width: u32,
    height: u32,
    alpha_offset: u32,
    reserved: u32,
    blocks_x: u32,
    blocks_y: u32,
    output_stride_words: u32,
    // 0 = opaque, 1 = A8, 2 = ETC1 alpha, 3 = palette alpha.
    alpha_mode: u32,
}

@group(0) @binding(0) var<storage, read> input_words: array<u32>;
@group(0) @binding(1) var<storage, read_write> output_pixels: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

const MODIFIERS: array<vec2<i32>, 8> = array(
    vec2<i32>(2, 8),
    vec2<i32>(5, 17),
    vec2<i32>(9, 29),
    vec2<i32>(13, 42),
    vec2<i32>(18, 60),
    vec2<i32>(24, 80),
    vec2<i32>(33, 106),
    vec2<i32>(47, 183),
);

fn read_byte(offset: u32) -> u32 {
    return (input_words[offset >> 2u] >> ((offset & 3u) * 8u)) & 0xffu;
}

fn byte_swap(value: u32) -> u32 {
    return ((value & 0x000000ffu) << 24u)
        | ((value & 0x0000ff00u) << 8u)
        | ((value & 0x00ff0000u) >> 8u)
        | ((value & 0xff000000u) >> 24u);
}

fn signed_3(value: u32) -> i32 {
    return i32(value << 29u) >> 29;
}

fn expand_5(value: i32) -> i32 {
    return (value << 3) | ((value & 0x1c) >> 2);
}

fn clamp_byte(value: i32) -> u32 {
    return u32(clamp(value, 0, 255));
}

fn decode_pixel(byte_offset: u32, block_index: u32, local_x: u32, local_y: u32) -> vec3<u32> {
    let word_offset = (byte_offset >> 2u) + block_index * 2u;
    let high = byte_swap(input_words[word_offset]);
    let low = byte_swap(input_words[word_offset + 1u]);
    let differential = ((high >> 1u) & 1u) != 0u;
    let flipped = (high & 1u) != 0u;

    var r1: i32;
    var g1: i32;
    var b1: i32;
    var r2: i32;
    var g2: i32;
    var b2: i32;
    if (differential) {
        let r = i32((high >> 27u) & 0x1fu);
        let g = i32((high >> 19u) & 0x1fu);
        let b = i32((high >> 11u) & 0x1fu);
        r1 = expand_5(r);
        g1 = expand_5(g);
        b1 = expand_5(b);
        r2 = expand_5(r + signed_3((high >> 24u) & 7u));
        g2 = expand_5(g + signed_3((high >> 16u) & 7u));
        b2 = expand_5(b + signed_3((high >> 8u) & 7u));
    } else {
        r1 = i32((high >> 28u) & 0xfu) * 17;
        r2 = i32((high >> 24u) & 0xfu) * 17;
        g1 = i32((high >> 20u) & 0xfu) * 17;
        g2 = i32((high >> 16u) & 0xfu) * 17;
        b1 = i32((high >> 12u) & 0xfu) * 17;
        b2 = i32((high >> 8u) & 0xfu) * 17;
    }

    let table1 = (high >> 5u) & 7u;
    let table2 = (high >> 2u) & 7u;
    let selector_bit = local_x * 4u + local_y;
    let magnitude = (low >> selector_bit) & 1u;
    let negative = ((low >> (selector_bit + 16u)) & 1u) != 0u;
    let first = select(local_x < 2u, local_y < 2u, flipped);
    let table = select(table2, table1, first);
    var modifier = MODIFIERS[table][magnitude];
    if (negative) {
        modifier = -modifier;
    }
    return vec3<u32>(
        clamp_byte(select(r2, r1, first) + modifier),
        clamp_byte(select(g2, g1, first) + modifier),
        clamp_byte(select(b2, b1, first) + modifier),
    );
}

fn palette_alpha(pixel_index: u32) -> u32 {
    let alpha_offset = params.alpha_offset;
    let count = read_byte(alpha_offset);
    var palette_count = count;
    var palette_offset = alpha_offset + 1u;
    var stream_offset = palette_offset + count;
    var depth = 1u;
    if (count == 0u) {
        palette_count = 2u;
        stream_offset = palette_offset;
    } else {
        var table_size = 2u;
        loop {
            if (count <= table_size) {
                break;
            }
            table_size *= 2u;
            depth += 1u;
        }
    }

    var index = 0u;
    for (var i = 0u; i < depth; i += 1u) {
        let bit_index = pixel_index * depth + i;
        let byte = read_byte(stream_offset + bit_index / 8u);
        let bit = (byte >> (7u - (bit_index & 7u))) & 1u;
        index = (index << 1u) | bit;
    }
    if (index >= palette_count) {
        index = 0u;
    }
    if (count == 0u) {
        return select(0u, 255u, index == 1u);
    }
    let value = read_byte(palette_offset + index);
    return (value << 4u) | value;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }

    let block_x = gid.x / 4u;
    let block_y = gid.y / 4u;
    let block_index = block_y * params.blocks_x + block_x;
    let local_x = gid.x & 3u;
    let local_y = gid.y & 3u;
    let color = decode_pixel(0u, block_index, local_x, local_y);
    let pixel_index = gid.y * params.width + gid.x;
    var alpha = 255u;
    if (params.alpha_mode == 1u) {
        alpha = read_byte(params.alpha_offset + pixel_index);
    } else if (params.alpha_mode == 2u) {
        alpha = decode_pixel(params.alpha_offset, block_index, local_x, local_y).g;
    } else if (params.alpha_mode == 3u) {
        alpha = palette_alpha(pixel_index);
    }
    output_pixels[gid.y * params.output_stride_words + gid.x] =
        color.r | (color.g << 8u) | (color.b << 16u) | (alpha << 24u);
}
