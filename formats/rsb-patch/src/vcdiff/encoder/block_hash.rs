pub(super) const BLOCK_SIZE: usize = 16;
const MAX_MATCHES_TO_CHECK: usize = 64;
const MAX_HASH_PROBES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub(super) enum Operation<'a> {
    Add(&'a [u8]),
    Copy { address: usize, size: usize },
}

pub(super) fn build_operations<'a>(
    source: &'a [u8],
    target: &'a [u8],
    minimum_match: usize,
) -> Vec<Operation<'a>> {
    let mut operations = Vec::new();
    if target.len() < BLOCK_SIZE {
        if !target.is_empty() {
            operations.push(Operation::Add(target));
        }
        return operations;
    }

    let dictionary_hash = BlockHash::populated(source, 0);
    let mut target_hash = BlockHash::empty(target, source.len());
    let mut next_encode = 0_usize;
    let mut candidate = 0_usize;
    let last_candidate = target.len() - BLOCK_SIZE;

    loop {
        let hash = rolling_hash(&target[candidate..candidate + BLOCK_SIZE]);
        let mut best = Match::default();
        dictionary_hash.find_best_match(hash, target, candidate, next_encode, &mut best);
        target_hash.find_best_match(hash, target, candidate, next_encode, &mut best);

        if best.size >= minimum_match {
            if best.target_offset != 0 {
                operations.push(Operation::Add(
                    &target[next_encode..next_encode + best.target_offset],
                ));
            }
            operations.push(Operation::Copy {
                address: best.source_offset,
                size: best.size,
            });
            next_encode += best.target_offset + best.size;
            candidate = next_encode;
            if candidate > last_candidate {
                break;
            }
            target_hash.add_all_blocks_through(next_encode);
        } else {
            target_hash.add_one_index_hash(candidate, hash);
            candidate += 1;
            if candidate > last_candidate {
                break;
            }
        }
    }
    if next_encode < target.len() {
        operations.push(Operation::Add(&target[next_encode..]));
    }
    operations
}

#[derive(Clone, Copy, Debug, Default)]
struct Match {
    size: usize,
    source_offset: usize,
    target_offset: usize,
}

impl Match {
    fn replace_if_better(&mut self, size: usize, source_offset: usize, target_offset: usize) {
        if size > self.size {
            *self = Self {
                size,
                source_offset,
                target_offset,
            };
        }
    }
}

/// Bentley/McIlroy block hash used by open-vcdiff. Keeping its aligned
/// 16-byte blocks, insertion order, collision limits, and strict tie-breaking
/// makes the generated COPY sequence match Twinning's encoder.
struct BlockHash<'a> {
    data: &'a [u8],
    starting_offset: usize,
    hash_mask: usize,
    hash_table: Vec<i32>,
    next_block: Vec<i32>,
    last_block: Vec<i32>,
    last_block_added: i32,
}

impl<'a> BlockHash<'a> {
    fn populated(data: &'a [u8], starting_offset: usize) -> Self {
        let mut result = Self::empty(data, starting_offset);
        result.add_all_blocks_through(data.len());
        result
    }

    fn empty(data: &'a [u8], starting_offset: usize) -> Self {
        let table_size = (data.len() / size_of::<i32>() + 1).next_power_of_two();
        let block_count = data.len() / BLOCK_SIZE;
        Self {
            data,
            starting_offset,
            hash_mask: table_size - 1,
            hash_table: vec![-1; table_size],
            next_block: vec![-1; block_count],
            last_block: vec![-1; block_count],
            last_block_added: -1,
        }
    }

    fn next_index_to_add(&self) -> usize {
        usize::try_from(self.last_block_added + 1).expect("block index is non-negative")
            * BLOCK_SIZE
    }

    fn add_one_index_hash(&mut self, index: usize, hash: u32) {
        if index == self.next_index_to_add() && index + BLOCK_SIZE <= self.data.len() {
            self.add_block(hash);
        }
    }

    fn add_all_blocks_through(&mut self, end_index: usize) {
        if self.data.len() < BLOCK_SIZE {
            return;
        }
        let end_limit = end_index.min(self.data.len() - BLOCK_SIZE + 1);
        while self.next_index_to_add() < end_limit {
            let index = self.next_index_to_add();
            self.add_block(rolling_hash(&self.data[index..index + BLOCK_SIZE]));
        }
    }

    fn add_block(&mut self, hash: u32) {
        let block = self.last_block_added + 1;
        let block_index = usize::try_from(block).expect("block index is non-negative");
        let slot = hash as usize & self.hash_mask;
        let first = self.hash_table[slot];
        if first < 0 {
            self.hash_table[slot] = block;
            self.last_block[block_index] = block;
        } else {
            let first_index = usize::try_from(first).expect("block index is non-negative");
            let last = self.last_block[first_index];
            let last_index = usize::try_from(last).expect("block index is non-negative");
            self.next_block[last_index] = block;
            self.last_block[first_index] = block;
        }
        self.last_block_added = block;
    }

    fn find_best_match(
        &self,
        hash: u32,
        target: &[u8],
        candidate: usize,
        target_start: usize,
        best: &mut Match,
    ) {
        let candidate_block = &target[candidate..candidate + BLOCK_SIZE];
        let mut block = self.first_matching_block(hash, candidate_block);
        let mut matches = 0_usize;
        while block >= 0 {
            matches += 1;
            if matches > MAX_MATCHES_TO_CHECK {
                break;
            }

            let block_index = usize::try_from(block).expect("block index is non-negative");
            let source_block_start = block_index * BLOCK_SIZE;
            let source_block_end = source_block_start + BLOCK_SIZE;
            let target_offset = candidate - target_start;
            let target_block_end = target_offset + BLOCK_SIZE;

            let left_limit = source_block_start.min(target_offset);
            let left = matching_length_to_left(
                &self.data[..source_block_start],
                &target[target_start..candidate],
                left_limit,
            );
            let right_limit = (self.data.len() - source_block_end)
                .min(target.len() - target_start - target_block_end);
            let right = matching_length(
                &self.data[source_block_end..source_block_end + right_limit],
                &target[candidate + BLOCK_SIZE..candidate + BLOCK_SIZE + right_limit],
            );
            best.replace_if_better(
                BLOCK_SIZE + left + right,
                self.starting_offset + source_block_start - left,
                target_offset - left,
            );
            block = self.next_matching_block(block_index, candidate_block);
        }
    }

    fn first_matching_block(&self, hash: u32, block: &[u8]) -> i32 {
        self.skip_non_matching(self.hash_table[hash as usize & self.hash_mask], block)
    }

    fn next_matching_block(&self, previous: usize, block: &[u8]) -> i32 {
        self.skip_non_matching(self.next_block[previous], block)
    }

    fn skip_non_matching(&self, mut candidate: i32, block: &[u8]) -> i32 {
        let mut probes = 0_usize;
        while candidate >= 0 {
            let index = usize::try_from(candidate).expect("block index is non-negative");
            let start = index * BLOCK_SIZE;
            if self.data[start..start + BLOCK_SIZE] == *block {
                break;
            }
            probes += 1;
            if probes > MAX_HASH_PROBES {
                return -1;
            }
            candidate = self.next_block[index];
        }
        candidate
    }
}

fn rolling_hash(data: &[u8]) -> u32 {
    debug_assert_eq!(data.len(), BLOCK_SIZE);
    const MULTIPLIER: u32 = 257;
    const MASK: u32 = (1 << 23) - 1;

    let mut hash = u32::from(data[0]) * MULTIPLIER + u32::from(data[1]);
    for byte in &data[2..] {
        hash = (hash.wrapping_mul(MULTIPLIER) + u32::from(*byte)) & MASK;
    }
    hash
}

fn matching_length(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn matching_length_to_left(left: &[u8], right: &[u8], limit: usize) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take(limit)
        .take_while(|(left, right)| left == right)
        .count()
}
