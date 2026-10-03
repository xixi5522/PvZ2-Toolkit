pub(super) fn insert_position(
    input: &[u8],
    position: usize,
    head: &mut [usize],
    previous: &mut [usize],
) {
    let Some(hash) = hash3(input, position) else {
        return;
    };
    previous[position] = head[hash];
    head[hash] = position;
}

pub(super) fn find_match(
    input: &[u8],
    position: usize,
    head: &[usize],
    previous: &[usize],
) -> (usize, usize) {
    let Some(hash) = hash3(input, position) else {
        return (0, 0);
    };
    let max_length = 258.min(input.len() - position);
    let mut candidate = head[hash];
    let mut best_length = 0usize;
    let mut best_distance = 0usize;
    let mut attempts = 0usize;
    while candidate != usize::MAX && attempts < 96 {
        let distance = position - candidate;
        if distance > 32_768 {
            break;
        }
        if best_length < max_length
            && input[candidate + best_length] == input[position + best_length]
        {
            let mut length = 0usize;
            while length < max_length && input[candidate + length] == input[position + length] {
                length += 1;
            }
            if length > best_length && length >= 3 {
                best_length = length;
                best_distance = distance;
                if length == max_length {
                    break;
                }
            }
        }
        candidate = previous[candidate];
        attempts += 1;
    }
    (best_length, best_distance)
}

fn hash3(input: &[u8], position: usize) -> Option<usize> {
    let bytes = input.get(position..position + 3)?;
    let value =
        (usize::from(bytes[0]) << 10) ^ (usize::from(bytes[1]) << 5) ^ usize::from(bytes[2]);
    Some(value.wrapping_mul(0x9e37) & 0xffff)
}
