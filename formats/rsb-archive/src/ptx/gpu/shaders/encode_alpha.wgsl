struct Params {
    width: u32,
    height: u32,
    reserved_0: u32,
    reserved_1: u32,
    output_word_count: u32,
    reserved_2: u32,
    output_offset_words: u32,
    // 1 = raw A8 (four pixels/word), 3 = palette indices (eight pixels/word).
    mode: u32,
}

@group(0) @binding(0) var<storage, read> input_pixels: array<u32>;
@group(0) @binding(1) var<storage, read_write> output_words: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

fn alpha_at(index: u32) -> u32 {
    return (input_pixels[index] >> 24u) & 0xffu;
}

fn reverse_nibble(value: u32) -> u32 {
    return ((value & 1u) << 3u)
        | ((value & 2u) << 1u)
        | ((value & 4u) >> 1u)
        | ((value & 8u) >> 3u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let invocation = gid.y * params.output_word_count + gid.x;
    let pixel_count = params.width * params.height;
    if (params.mode == 1u) {
        let base = invocation * 4u;
        if (base >= pixel_count) {
            return;
        }
        var packed = 0u;
        for (var i = 0u; i < 4u; i += 1u) {
            if (base + i < pixel_count) {
                packed |= alpha_at(base + i) << (i * 8u);
            }
        }
        output_words[params.output_offset_words + invocation] = packed;
    } else if (params.mode == 3u) {
        let base = invocation * 8u;
        if (base >= pixel_count) {
            return;
        }
        var packed = 0u;
        for (var pair = 0u; pair < 4u; pair += 1u) {
            let first = base + pair * 2u;
            var byte = 0u;
            if (first < pixel_count) {
                byte |= reverse_nibble(alpha_at(first) >> 4u);
            }
            if (first + 1u < pixel_count) {
                byte |= reverse_nibble(alpha_at(first + 1u) >> 4u) << 4u;
            }
            packed |= byte << (pair * 8u);
        }
        output_words[params.output_offset_words + invocation] = packed;
    }
}
