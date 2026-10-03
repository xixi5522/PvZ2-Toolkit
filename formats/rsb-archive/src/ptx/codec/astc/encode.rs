use super::format::{AstcQuality, astc_data_size, validate_block_size};
use super::pack::pack_gradient_block;
use super::search::*;
use crate::error::Result;
use crate::ptx::Rgba8Surface;

/// Encodes canonical RGBA8 pixels as raw ASTC LDR blocks.
///
/// The encoder searches single-plane 4x4 weight grids and, for images with a
/// varying alpha channel, an independent-alpha dual-plane 3x3 grid. Both block
/// modes are standard ASTC encodings and work with every two-dimensional
/// footprint.
pub fn encode_astc_rgba8(
    surface: Rgba8Surface<'_>,
    block_width: u32,
    block_height: u32,
    quality: AstcQuality,
) -> Result<Vec<u8>> {
    validate_block_size(block_width, block_height)?;
    let mut output = Vec::with_capacity(astc_data_size(
        surface.width(),
        surface.height(),
        block_width,
        block_height,
    )?);

    for block_y in 0..surface.height().div_ceil(block_height) {
        for block_x in 0..surface.width().div_ceil(block_width) {
            let pixels = read_block(surface, block_x, block_y, block_width, block_height);
            output.extend_from_slice(&encode_block(&pixels, block_width, block_height, quality));
        }
    }

    Ok(output)
}

fn read_block(
    surface: Rgba8Surface<'_>,
    block_x: u32,
    block_y: u32,
    block_width: u32,
    block_height: u32,
) -> Vec<[f32; 4]> {
    let mut pixels = Vec::with_capacity((block_width * block_height) as usize);
    for y in 0..block_height {
        for x in 0..block_width {
            let source_x = (block_x * block_width + x).min(surface.width().saturating_sub(1));
            let source_y = (block_y * block_height + y).min(surface.height().saturating_sub(1));
            let pixel = surface.pixel(source_x, source_y);
            pixels.push(pixel.map(f32::from));
        }
    }
    pixels
}

fn encode_block(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    quality: AstcQuality,
) -> [u8; 16] {
    if let Some(color) = constant_color(pixels) {
        return encode_void_extent(color);
    }

    let effort = quality.get();
    let mut directions = vec![range_direction(pixels)];
    if effort >= 10 {
        directions.push(principal_direction(pixels, true));
    }
    if effort >= 75 {
        directions.push(principal_direction(pixels, false));
        directions.push([0.299, 0.587, 0.114, 0.0]);
        directions.push([0.0, 0.0, 0.0, 1.0]);
    }

    let mut best = directions
        .into_iter()
        .filter_map(|direction| {
            encode_single_plane_candidate(pixels, block_width, block_height, direction, effort)
        })
        .min_by(|left, right| left.error.total_cmp(&right.error))
        .expect("a non-constant ASTC block has a usable direction");

    if effort >= 35
        && alpha_varies(pixels)
        && let Some(dual) = encode_dual_plane_candidate(pixels, block_width, block_height, effort)
        && dual.error < best.error
    {
        best = dual;
    }

    best.data
}

fn constant_color(pixels: &[[f32; 4]]) -> Option<[u8; 4]> {
    let first = pixels.first()?;
    if pixels.iter().all(|pixel| pixel == first) {
        Some(first.map(|value| value as u8))
    } else {
        None
    }
}

fn encode_void_extent(color: [u8; 4]) -> [u8; 16] {
    // Low 12 bits are the LDR void-extent block marker. Setting all four
    // 13-bit extents to one describes a constant color covering the block.
    let mut block = 0x0DFCu128 | (((1u128 << 52) - 1) << 12);
    for (channel, value) in color.into_iter().enumerate() {
        let unorm16 = u16::from(value) * 0x101;
        block |= u128::from(unorm16) << (64 + channel * 16);
    }
    block.to_le_bytes()
}

struct EncodedCandidate {
    data: [u8; 16],
    error: f64,
}

fn encode_single_plane_candidate(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    direction: [f32; 4],
    effort: u8,
) -> Option<EncodedCandidate> {
    let direction = normalize(direction)?;
    let mut endpoints = endpoints_on_axis(pixels, direction);
    let mut weights = sample_grid_weights(pixels, block_width, block_height, 4, 4, endpoints, None);

    let iterations = match effort {
        0..=9 => 1,
        10..=74 => 2,
        _ => 3,
    };
    for iteration in 0..iterations {
        let texel_weights = infill_weights(&weights, 4, 4, block_width, block_height);
        endpoints = solve_endpoints(pixels, &texel_weights, endpoints, 0..4);
        if effort >= 10 || iteration > 0 {
            optimize_single_weights(pixels, block_width, block_height, &mut weights, endpoints);
        }
    }

    let (endpoints, weights, _) = quantize_and_order_endpoints(endpoints, weights);
    let error = single_plane_error(pixels, block_width, block_height, &weights, endpoints);
    Some(EncodedCandidate {
        data: pack_gradient_block(0x042, endpoints, &weights, None),
        error,
    })
}

fn encode_dual_plane_candidate(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    effort: u8,
) -> Option<EncodedCandidate> {
    let rgb_direction = normalize(principal_direction(pixels, false))?;
    let mut endpoints = endpoints_on_axis(pixels, rgb_direction);
    let min_alpha = pixels
        .iter()
        .map(|pixel| pixel[3])
        .fold(f32::INFINITY, f32::min);
    let max_alpha = pixels
        .iter()
        .map(|pixel| pixel[3])
        .fold(f32::NEG_INFINITY, f32::max);
    endpoints[0][3] = min_alpha;
    endpoints[1][3] = max_alpha;

    let mut rgb_weights = sample_grid_weights(
        pixels,
        block_width,
        block_height,
        3,
        3,
        endpoints,
        Some(0..3),
    );
    let mut alpha_weights = sample_grid_weights(
        pixels,
        block_width,
        block_height,
        3,
        3,
        endpoints,
        Some(3..4),
    );

    let iterations = match effort {
        0..=74 => 1,
        75..=94 => 2,
        _ => 3,
    };
    for _ in 0..iterations {
        let rgb_texel = infill_weights(&rgb_weights, 3, 3, block_width, block_height);
        let alpha_texel = infill_weights(&alpha_weights, 3, 3, block_width, block_height);
        endpoints = solve_endpoints(pixels, &rgb_texel, endpoints, 0..3);
        endpoints = solve_endpoints(pixels, &alpha_texel, endpoints, 3..4);
        optimize_dual_weights(
            pixels,
            block_width,
            block_height,
            &mut rgb_weights,
            &mut alpha_weights,
            endpoints,
        );
    }

    let (endpoints, rgb_weights, reversed) = quantize_and_order_endpoints(endpoints, rgb_weights);
    if reversed {
        // `quantize_and_order_endpoints` already reversed the RGB grid. Keep
        // the independently encoded alpha grid on the same endpoint order.
        invert_weights(&mut alpha_weights);
    }
    let error = dual_plane_error(
        pixels,
        block_width,
        block_height,
        &rgb_weights,
        &alpha_weights,
        endpoints,
    );
    Some(EncodedCandidate {
        data: pack_gradient_block(0x5AE, endpoints, &rgb_weights, Some(&alpha_weights)),
        error,
    })
}
