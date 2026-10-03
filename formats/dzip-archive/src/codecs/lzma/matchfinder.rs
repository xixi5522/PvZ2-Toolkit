pub(super) fn insert_position(
    input: &[u8],
    position: usize,
    head: &mut [usize],
    chain: &mut [usize],
) {
    let Some(hash) = hash4(input, position) else {
        return;
    };
    chain[position] = head[hash];
    head[hash] = position;
}

pub(super) fn find_match(
    input: &[u8],
    position: usize,
    dictionary_size: usize,
    fast_bytes: usize,
    max_attempts: usize,
    head: &[usize],
    chain: &[usize],
) -> (usize, usize) {
    let Some(hash) = hash4(input, position) else {
        return (0, 0);
    };
    let mut candidate = head[hash];
    let mut best_length = 0usize;
    let mut best_distance = 0usize;
    let mut attempts = 0usize;
    while candidate != usize::MAX && attempts < max_attempts {
        let distance = position - candidate;
        if distance > dictionary_size {
            break;
        }
        let length = match_length(input, position, candidate, 273);
        if length > best_length {
            best_length = length;
            best_distance = distance;
            if length >= fast_bytes || length == 273 || position + length == input.len() {
                break;
            }
        }
        candidate = chain[candidate];
        attempts += 1;
    }
    (best_length, best_distance)
}

pub(super) fn match_length(input: &[u8], left: usize, right: usize, maximum: usize) -> usize {
    let maximum = maximum.min(input.len() - left);
    let mut length = 0usize;
    while length < maximum && input[left + length] == input[right + length] {
        length += 1;
    }
    length
}

fn hash4(input: &[u8], position: usize) -> Option<usize> {
    let bytes = input.get(position..position + 4)?;
    let mut hash = 0x811c_9dc5u32;
    for &byte in bytes {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    Some((hash >> 16) as usize)
}
