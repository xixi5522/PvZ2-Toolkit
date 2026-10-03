pub(super) fn range_direction(pixels: &[[f32; 4]]) -> [f32; 4] {
    let mut minimum = [f32::INFINITY; 4];
    let mut maximum = [f32::NEG_INFINITY; 4];
    for pixel in pixels {
        for (channel, &source) in pixel.iter().enumerate() {
            minimum[channel] = minimum[channel].min(source);
            maximum[channel] = maximum[channel].max(source);
        }
    }
    std::array::from_fn(|channel| maximum[channel] - minimum[channel])
}

pub(super) fn principal_direction(pixels: &[[f32; 4]], include_alpha: bool) -> [f32; 4] {
    let mut mean = [0.0; 4];
    for pixel in pixels {
        for (channel, &source) in pixel.iter().enumerate() {
            mean[channel] += source;
        }
    }
    for channel in &mut mean {
        *channel /= pixels.len() as f32;
    }

    let mut covariance = [[0.0; 4]; 4];
    let channels = if include_alpha { 4 } else { 3 };
    for pixel in pixels {
        for row in 0..channels {
            for column in 0..channels {
                covariance[row][column] +=
                    (pixel[row] - mean[row]) * (pixel[column] - mean[column]);
            }
        }
    }

    let mut direction = range_direction(pixels);
    if !include_alpha {
        direction[3] = 0.0;
    }
    if normalize(direction).is_none() {
        direction = [1.0, 1.0, 1.0, if include_alpha { 1.0 } else { 0.0 }];
    }
    for _ in 0..8 {
        let next = std::array::from_fn(|row| {
            (0..channels)
                .map(|column| covariance[row][column] * direction[column])
                .sum()
        });
        let Some(normalized) = normalize(next) else {
            break;
        };
        direction = normalized;
    }
    direction
}

pub(super) fn normalize(mut vector: [f32; 4]) -> Option<[f32; 4]> {
    let length = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length <= 1e-5 {
        return None;
    }
    for value in &mut vector {
        *value /= length;
    }
    Some(vector)
}

pub(super) fn endpoints_on_axis(pixels: &[[f32; 4]], direction: [f32; 4]) -> [[f32; 4]; 2] {
    let mut mean = [0.0; 4];
    for pixel in pixels {
        for channel in 0..4 {
            mean[channel] += pixel[channel];
        }
    }
    for channel in &mut mean {
        *channel /= pixels.len() as f32;
    }

    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for pixel in pixels {
        let projection = dot(subtract(*pixel, mean), direction);
        minimum = minimum.min(projection);
        maximum = maximum.max(projection);
    }

    [
        std::array::from_fn(|channel| {
            (mean[channel] + minimum * direction[channel]).clamp(0.0, 255.0)
        }),
        std::array::from_fn(|channel| {
            (mean[channel] + maximum * direction[channel]).clamp(0.0, 255.0)
        }),
    ]
}

pub(super) fn sample_grid_weights(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    grid_width: u32,
    grid_height: u32,
    endpoints: [[f32; 4]; 2],
    channels: Option<std::ops::Range<usize>>,
) -> Vec<u8> {
    let channels = channels.unwrap_or(0..4);
    let mut result = Vec::with_capacity((grid_width * grid_height) as usize);
    for grid_y in 0..grid_height {
        for grid_x in 0..grid_width {
            let x = (grid_x * (block_width - 1) + (grid_width - 1) / 2) / (grid_width - 1);
            let y = (grid_y * (block_height - 1) + (grid_height - 1) / 2) / (grid_height - 1);
            let pixel = pixels[(y * block_width + x) as usize];
            result.push(closest_weight(pixel, endpoints, channels.clone()));
        }
    }
    result
}

pub(super) fn closest_weight(
    pixel: [f32; 4],
    endpoints: [[f32; 4]; 2],
    channels: std::ops::Range<usize>,
) -> u8 {
    let mut best = (f64::INFINITY, 0);
    for encoded in 0..=3 {
        let weight = f32::from(unquantized_weight(encoded)) / 64.0;
        let error = channels
            .clone()
            .map(|channel| {
                let value = endpoints[0][channel] * (1.0 - weight) + endpoints[1][channel] * weight;
                let difference = pixel[channel] - value;
                f64::from(difference * difference)
            })
            .sum();
        if error < best.0 {
            best = (error, encoded);
        }
    }
    best.1
}

pub(super) fn solve_endpoints(
    pixels: &[[f32; 4]],
    weights: &[u8],
    mut endpoints: [[f32; 4]; 2],
    channels: std::ops::Range<usize>,
) -> [[f32; 4]; 2] {
    let mut aa = 0.0;
    let mut ab = 0.0;
    let mut bb = 0.0;
    for &weight in weights {
        let b = f32::from(weight) / 64.0;
        let a = 1.0 - b;
        aa += a * a;
        ab += a * b;
        bb += b * b;
    }
    let determinant = aa * bb - ab * ab;
    if determinant.abs() <= 1e-5 {
        return endpoints;
    }

    for channel in channels {
        let mut ap = 0.0;
        let mut bp = 0.0;
        for (pixel, &weight) in pixels.iter().zip(weights) {
            let b = f32::from(weight) / 64.0;
            let a = 1.0 - b;
            ap += a * pixel[channel];
            bp += b * pixel[channel];
        }
        endpoints[0][channel] = ((ap * bb - bp * ab) / determinant).clamp(0.0, 255.0);
        endpoints[1][channel] = ((bp * aa - ap * ab) / determinant).clamp(0.0, 255.0);
    }
    endpoints
}

pub(super) fn optimize_single_weights(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    weights: &mut [u8],
    endpoints: [[f32; 4]; 2],
) {
    if block_width == 4 && block_height == 4 {
        for (weight, &pixel) in weights.iter_mut().zip(pixels) {
            *weight = closest_weight(pixel, endpoints, 0..4);
        }
        return;
    }

    for index in 0..weights.len() {
        let original = weights[index];
        let mut best = (f64::INFINITY, original);
        for candidate in 0..=3 {
            weights[index] = candidate;
            let error =
                single_plane_error_f32(pixels, block_width, block_height, weights, endpoints);
            if error < best.0 {
                best = (error, candidate);
            }
        }
        weights[index] = best.1;
    }
}

pub(super) fn optimize_dual_weights(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    rgb_weights: &mut [u8],
    alpha_weights: &mut [u8],
    endpoints: [[f32; 4]; 2],
) {
    for plane in 0..2 {
        let length = rgb_weights.len();
        for index in 0..length {
            let original = if plane == 0 {
                rgb_weights[index]
            } else {
                alpha_weights[index]
            };
            let mut best = (f64::INFINITY, original);
            for candidate in 0..=3 {
                if plane == 0 {
                    rgb_weights[index] = candidate;
                } else {
                    alpha_weights[index] = candidate;
                }
                let error = dual_plane_error_f32(
                    pixels,
                    block_width,
                    block_height,
                    rgb_weights,
                    alpha_weights,
                    endpoints,
                );
                if error < best.0 {
                    best = (error, candidate);
                }
            }
            if plane == 0 {
                rgb_weights[index] = best.1;
            } else {
                alpha_weights[index] = best.1;
            }
        }
    }
}

pub(super) fn quantize_and_order_endpoints(
    endpoints: [[f32; 4]; 2],
    mut weights: Vec<u8>,
) -> ([[u8; 4]; 2], Vec<u8>, bool) {
    let mut quantized =
        endpoints.map(|endpoint| endpoint.map(|value| value.round().clamp(0.0, 255.0) as u8));
    let reversed = rgb_sum(quantized[1]) < rgb_sum(quantized[0]);
    if reversed {
        quantized.swap(0, 1);
        invert_weights(&mut weights);
    }
    (quantized, weights, reversed)
}

pub(super) fn invert_weights(weights: &mut [u8]) {
    for weight in weights {
        *weight = 3 - *weight;
    }
}

pub(super) fn rgb_sum(color: [u8; 4]) -> u16 {
    u16::from(color[0]) + u16::from(color[1]) + u16::from(color[2])
}

pub(super) fn infill_weights(
    grid: &[u8],
    grid_width: u32,
    grid_height: u32,
    block_width: u32,
    block_height: u32,
) -> Vec<u8> {
    let source: Vec<u32> = grid
        .iter()
        .map(|&weight| u32::from(unquantized_weight(weight)))
        .collect();
    let mut output = vec![0; (block_width * block_height) as usize];
    let ds = (1024 + block_width / 2) / (block_width - 1);
    let dt = (1024 + block_height / 2) / (block_height - 1);

    for y in 0..block_height {
        for x in 0..block_width {
            let gs = (ds * x * (grid_width - 1) + 32) >> 6;
            let gt = (dt * y * (grid_height - 1) + 32) >> 6;
            let grid_x = gs >> 4;
            let grid_y = gt >> 4;
            let fraction_x = gs & 0xF;
            let fraction_y = gt & 0xF;

            let w11 = (fraction_x * fraction_y + 8) >> 4;
            let w10 = fraction_y - w11;
            let w01 = fraction_x - w11;
            let w00 = 16 + w11 - fraction_x - fraction_y;
            let base = grid_x + grid_y * grid_width;
            let get = |offset_x: u32, offset_y: u32| {
                let index = base + offset_x + offset_y * grid_width;
                source.get(index as usize).copied().unwrap_or(0)
            };
            output[(y * block_width + x) as usize] =
                ((get(0, 0) * w00 + get(1, 0) * w01 + get(0, 1) * w10 + get(1, 1) * w11 + 8) >> 4)
                    as u8;
        }
    }
    output
}

const fn unquantized_weight(weight: u8) -> u8 {
    [0, 21, 43, 64][weight as usize]
}

pub(super) fn single_plane_error(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    weights: &[u8],
    endpoints: [[u8; 4]; 2],
) -> f64 {
    single_plane_error_f32(
        pixels,
        block_width,
        block_height,
        weights,
        endpoints.map(|endpoint| endpoint.map(f32::from)),
    )
}

pub(super) fn single_plane_error_f32(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    weights: &[u8],
    endpoints: [[f32; 4]; 2],
) -> f64 {
    let texel_weights = infill_weights(weights, 4, 4, block_width, block_height);
    reconstruction_error(pixels, &texel_weights, &texel_weights, endpoints)
}

pub(super) fn dual_plane_error(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    rgb_weights: &[u8],
    alpha_weights: &[u8],
    endpoints: [[u8; 4]; 2],
) -> f64 {
    dual_plane_error_f32(
        pixels,
        block_width,
        block_height,
        rgb_weights,
        alpha_weights,
        endpoints.map(|endpoint| endpoint.map(f32::from)),
    )
}

pub(super) fn dual_plane_error_f32(
    pixels: &[[f32; 4]],
    block_width: u32,
    block_height: u32,
    rgb_weights: &[u8],
    alpha_weights: &[u8],
    endpoints: [[f32; 4]; 2],
) -> f64 {
    let rgb = infill_weights(rgb_weights, 3, 3, block_width, block_height);
    let alpha = infill_weights(alpha_weights, 3, 3, block_width, block_height);
    reconstruction_error(pixels, &rgb, &alpha, endpoints)
}

pub(super) fn reconstruction_error(
    pixels: &[[f32; 4]],
    rgb_weights: &[u8],
    alpha_weights: &[u8],
    endpoints: [[f32; 4]; 2],
) -> f64 {
    let mut error = 0.0;
    for ((pixel, &rgb_weight), &alpha_weight) in pixels.iter().zip(rgb_weights).zip(alpha_weights) {
        let alpha_scale = 0.25 + 0.75 * (pixel[3] / 255.0).powi(2);
        for (channel, &source) in pixel.iter().enumerate() {
            let weight = if channel == 3 {
                alpha_weight
            } else {
                rgb_weight
            };
            let weight = f32::from(weight) / 64.0;
            let reconstructed =
                endpoints[0][channel] * (1.0 - weight) + endpoints[1][channel] * weight;
            let difference = f64::from(source - reconstructed);
            let importance = if channel == 3 {
                1.5
            } else {
                f64::from(alpha_scale)
            };
            error += difference * difference * importance;
        }
    }
    error
}

pub(super) fn alpha_varies(pixels: &[[f32; 4]]) -> bool {
    let first = pixels[0][3];
    pixels.iter().any(|pixel| (pixel[3] - first).abs() >= 1.0)
}

pub(super) fn subtract(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|channel| left[channel] - right[channel])
}

pub(super) fn dot(left: [f32; 4], right: [f32; 4]) -> f32 {
    (0..4).map(|channel| left[channel] * right[channel]).sum()
}
