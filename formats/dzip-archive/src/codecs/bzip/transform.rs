use alloc::vec;
use alloc::vec::Vec;

use crate::codecs::bzip::Error;

const RUNA: usize = 0;
const RUNB: usize = 1;

pub(super) fn derandomize(bytes: &mut [u8]) {
    let mut remaining = 0u16;
    let mut index = 0usize;
    for byte in bytes {
        if remaining == 0 {
            remaining = crate::codecs::bzip::randtable::BZ2_RNUMS[index];
            index = (index + 1) % crate::codecs::bzip::randtable::BZ2_RNUMS.len();
        }
        remaining -= 1;
        if remaining == 1 {
            *byte ^= 1;
        }
    }
}

pub(super) fn encode_rle1(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut position = 0usize;
    while position < input.len() {
        let byte = input[position];
        let mut run = 1usize;
        while position + run < input.len() && input[position + run] == byte && run < 259 {
            run += 1;
        }
        if run < 4 {
            output.resize(output.len() + run, byte);
        } else {
            output.extend_from_slice(&[byte; 4]);
            output.push((run - 4) as u8);
        }
        position += run;
    }
    output
}

pub(super) fn decode_rle1(input: &[u8], limit: usize) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let mut position = 0usize;
    while position < input.len() {
        let byte = input[position];
        let mut run = 1usize;
        while run < 4 && position + run < input.len() && input[position + run] == byte {
            run += 1;
        }
        let (encoded_width, decoded_run) = if run == 4 {
            let extra = *input
                .get(position + 4)
                .ok_or_else(|| Error::new("truncated BZip2 RLE1 run"))?;
            (5, 4 + usize::from(extra))
        } else {
            (run, run)
        };
        if output.len().saturating_add(decoded_run) > limit {
            return Err(Error::new("BZip2 RLE1 output exceeds declared length"));
        }
        output.resize(output.len() + decoded_run, byte);
        position += encoded_width;
    }
    Ok(output)
}

pub(super) fn bwt(input: &[u8]) -> (Vec<u8>, usize) {
    let length = input.len();
    if length == 0 {
        return (Vec::new(), 0);
    }
    let mut order: Vec<usize> = (0..length).collect();
    let mut rank: Vec<usize> = input.iter().map(|&byte| usize::from(byte)).collect();
    let mut next_rank = vec![0usize; length];
    let mut step = 1usize;
    loop {
        order.sort_by_key(|&index| (rank[index], rank[(index + step) % length], index));
        next_rank[order[0]] = 0;
        for pair in order.windows(2) {
            let previous = pair[0];
            let current = pair[1];
            let differs = (rank[previous], rank[(previous + step) % length])
                != (rank[current], rank[(current + step) % length]);
            next_rank[current] = next_rank[previous] + usize::from(differs);
        }
        rank.copy_from_slice(&next_rank);
        if rank[order[length - 1]] == length - 1 || step >= length {
            break;
        }
        step = step.saturating_mul(2).min(length);
    }
    let original_pointer = order.iter().position(|&index| index == 0).unwrap();
    let transformed = order
        .iter()
        .map(|&index| input[(index + length - 1) % length])
        .collect();
    (transformed, original_pointer)
}

pub(super) fn inverse_bwt(input: &[u8], original_pointer: usize) -> Result<Vec<u8>, Error> {
    let length = input.len();
    if length == 0 || original_pointer >= length {
        return Err(Error::new("invalid BZip2 BWT input"));
    }
    let mut counts = [0usize; 256];
    for &byte in input {
        counts[usize::from(byte)] += 1;
    }
    let mut starts = [0usize; 256];
    let mut total = 0usize;
    for (start, &count) in starts.iter_mut().zip(&counts) {
        *start = total;
        total += count;
    }
    let mut occurrences = [0usize; 256];
    let mut next = vec![0usize; length];
    for (index, &byte) in input.iter().enumerate() {
        let byte_index = usize::from(byte);
        let sorted_index = starts[byte_index] + occurrences[byte_index];
        next[sorted_index] = index;
        occurrences[byte_index] += 1;
    }
    let mut output = Vec::with_capacity(length);
    let mut position = next[original_pointer];
    for _ in 0..length {
        output.push(input[position]);
        position = next[position];
    }
    Ok(output)
}

pub(super) fn mtf_rle2(input: &[u8]) -> (Vec<u8>, Vec<usize>) {
    let mut present = [false; 256];
    for &byte in input {
        present[usize::from(byte)] = true;
    }
    let used: Vec<u8> = present
        .iter()
        .enumerate()
        .filter_map(|(byte, &value)| value.then_some(byte as u8))
        .collect();
    let mut mtf = used.clone();
    let mut symbols = Vec::new();
    let mut zero_run = 0usize;
    for &byte in input {
        let index = mtf.iter().position(|&value| value == byte).unwrap();
        let value = mtf.remove(index);
        mtf.insert(0, value);
        if index == 0 {
            zero_run += 1;
        } else {
            flush_zero_run(&mut symbols, zero_run);
            zero_run = 0;
            symbols.push(index + 1);
        }
    }
    flush_zero_run(&mut symbols, zero_run);
    symbols.push(used.len() + 1);
    (used, symbols)
}

fn flush_zero_run(symbols: &mut Vec<usize>, mut run: usize) {
    if run == 0 {
        return;
    }
    run -= 1;
    loop {
        symbols.push(if run & 1 == 0 { RUNA } else { RUNB });
        if run < 2 {
            break;
        }
        run = (run - 2) / 2;
    }
}
