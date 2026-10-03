use crate::codecs::dz::model::DzFixedCosts;
use crate::codecs::dz::range::AdaptiveModel;
use crate::codecs::dz::{DzipError, MAX_MATCH, MIN_MATCH, Result};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::ToString;
use alloc::vec::Vec;

struct LzMatcher {
    window: usize,
    allow_position_zero: bool,
    chains: BTreeMap<u32, VecDeque<usize>>,
    stale_keys: BTreeSet<u32>,
}

pub(crate) struct LazyLzParser {
    matcher: LzMatcher,
    pending: Option<MatchCandidate>,
    search_position: usize,
    initial_length: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LzDecision {
    Literal { position: usize },
    Match { length: usize, distance: usize },
}

#[derive(Clone, Copy, Debug)]
struct MatchCandidate {
    length: usize,
    distance: usize,
    savings: i32,
}

#[derive(Clone, Copy)]
pub(crate) struct MatchCost<'a> {
    pub(crate) scoring: MatchScoring<'a>,
    pub(crate) recent_offsets: &'a [usize; 4],
}

#[derive(Clone, Copy)]
pub(crate) enum MatchScoring<'a> {
    Heuristic,
    Fixed {
        costs: &'a DzFixedCosts,
        dynamic_offsets: &'a [Vec<AdaptiveModel>],
    },
}

impl LazyLzParser {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            matcher: LzMatcher::new(window),
            pending: None,
            search_position: 0,
            initial_length: 1,
        }
    }

    pub(crate) fn new_common(window: usize) -> Self {
        Self {
            matcher: LzMatcher::new_common(window),
            pending: None,
            search_position: 0,
            initial_length: 1,
        }
    }

    pub(crate) fn new_with_stale_keys(
        window: usize,
        allow_position_zero: bool,
        stale_keys: &BTreeSet<u32>,
    ) -> Self {
        Self {
            matcher: LzMatcher::new_with_stale_keys(window, allow_position_zero, stale_keys),
            pending: None,
            search_position: 0,
            initial_length: 1,
        }
    }

    pub(crate) fn skip_boundary(
        &mut self,
        input: &[u8],
        position: usize,
        cost: MatchCost<'_>,
    ) -> Result<()> {
        self.pending = None;
        self.search_position = position;
        let scan_maximum = usize::min(MAX_MATCH, input.len().saturating_sub(position));
        let candidate =
            self.matcher
                .find(input, position, scan_maximum, 0, 1, 10_000, cost, None)?;
        self.matcher.insert(input, position);
        self.search_position = position.saturating_add(1);
        self.initial_length = candidate.map_or(1, |candidate| candidate.length);
        Ok(())
    }

    pub(crate) fn skip_range(&mut self, input: &[u8], start: usize, end: usize) {
        self.pending = None;
        self.search_position = start;
        for position in start..end {
            self.matcher.insert(input, position);
        }
        self.search_position = end;
        self.initial_length = 1;
    }

    pub(crate) fn next(&mut self, input: &[u8], cost: MatchCost<'_>) -> Result<Option<LzDecision>> {
        self.next_bounded(input, input.len(), cost)
    }

    pub(crate) fn next_bounded(
        &mut self,
        input: &[u8],
        boundary: usize,
        cost: MatchCost<'_>,
    ) -> Result<Option<LzDecision>> {
        if boundary > input.len() {
            return Err(DzipError::InvalidDz(
                "LZ decision boundary exceeds its lookahead".to_string(),
            ));
        }
        loop {
            if self.pending.is_none() {
                if self.search_position >= boundary {
                    return Ok(None);
                }
                let position = self.search_position;
                let scan_maximum = usize::min(MAX_MATCH, input.len() - position);
                let stored_maximum = usize::min(scan_maximum, boundary - position);
                let candidate = self
                    .matcher
                    .find(
                        input,
                        position,
                        scan_maximum,
                        stored_maximum,
                        self.initial_length,
                        10_000,
                        cost,
                        None,
                    )?
                    .unwrap_or(MatchCandidate {
                        length: 1,
                        distance: 0,
                        savings: 0,
                    });
                self.initial_length = 1;
                self.matcher.insert(input, position);
                self.pending = Some(candidate);
                self.search_position += 1;
                continue;
            }

            let pending = self.pending.expect("pending candidate");
            let pending_position = self.search_position - 1;
            let pending_emitted_length =
                usize::min(pending.length, boundary.saturating_sub(pending_position));
            if pending_emitted_length >= MAX_MATCH {
                if self.search_position < boundary {
                    self.matcher.insert(input, self.search_position);
                }
                for inserted in pending_position + 2..pending_position + pending_emitted_length {
                    self.matcher.insert(input, inserted);
                }
                self.search_position = pending_position + pending_emitted_length;
                self.pending = None;
                return Ok(Some(LzDecision::Match {
                    length: pending_emitted_length,
                    distance: pending.distance,
                }));
            }
            if self.search_position < boundary {
                let position = self.search_position;
                let scan_maximum = usize::min(MAX_MATCH, input.len() - position);
                let stored_maximum = usize::min(scan_maximum, boundary - position);
                let candidate_limit = if pending.length >= 64 { 2_500 } else { 10_000 };
                let current = self
                    .matcher
                    .find(
                        input,
                        position,
                        scan_maximum,
                        stored_maximum,
                        pending.length,
                        candidate_limit,
                        cost,
                        None,
                    )?
                    .unwrap_or(MatchCandidate {
                        length: 1,
                        distance: 0,
                        savings: 0,
                    });
                self.matcher.insert(input, position);

                if pending.length >= MIN_MATCH && current.savings <= pending.savings {
                    let emitted_length = pending_emitted_length;
                    for inserted in pending_position + 2..pending_position + emitted_length {
                        self.matcher.insert(input, inserted);
                    }
                    self.search_position = pending_position + emitted_length;
                    self.pending = None;
                    return Ok(Some(if emitted_length >= MIN_MATCH {
                        LzDecision::Match {
                            length: emitted_length,
                            distance: pending.distance,
                        }
                    } else {
                        LzDecision::Literal {
                            position: pending_position,
                        }
                    }));
                }

                self.pending = Some(current);
                self.search_position += 1;
                return Ok(Some(LzDecision::Literal {
                    position: pending_position,
                }));
            }

            self.pending = None;
            let emitted_length =
                usize::min(pending.length, boundary.saturating_sub(pending_position));
            if emitted_length >= MIN_MATCH {
                return Ok(Some(LzDecision::Match {
                    length: emitted_length,
                    distance: pending.distance,
                }));
            }
            return Ok(Some(LzDecision::Literal {
                position: pending_position,
            }));
        }
    }
}

impl LzMatcher {
    fn new(window: usize) -> Self {
        Self {
            window,
            allow_position_zero: false,
            chains: BTreeMap::new(),
            stale_keys: BTreeSet::new(),
        }
    }

    fn new_common(window: usize) -> Self {
        Self {
            window,
            allow_position_zero: true,
            chains: BTreeMap::new(),
            stale_keys: BTreeSet::new(),
        }
    }

    fn new_with_stale_keys(
        window: usize,
        allow_position_zero: bool,
        stale_keys: &BTreeSet<u32>,
    ) -> Self {
        Self {
            window,
            allow_position_zero,
            chains: BTreeMap::new(),
            stale_keys: stale_keys.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn find(
        &self,
        input: &[u8],
        position: usize,
        scan_maximum: usize,
        stored_maximum: usize,
        initial_length: usize,
        candidate_limit: usize,
        cost: MatchCost<'_>,
        extra_candidate: Option<usize>,
    ) -> Result<Option<MatchCandidate>> {
        if scan_maximum < MIN_MATCH || position + MIN_MATCH > input.len() {
            return Ok(None);
        }
        let Some(key) = local_match_key(input, position) else {
            return Ok(None);
        };
        let candidates = self.chains.get(&key);
        let extra_candidate =
            extra_candidate.filter(|&candidate| local_match_key(input, candidate) == Some(key));
        let scan_maximum = usize::min(scan_maximum, input.len() - position);
        let stored_maximum = usize::min(stored_maximum, scan_maximum);
        // sub_490770 reserves MAX_MATCH + 3 bytes at the back of the LZ
        // window and uses zero as the empty-chain sentinel.  Consequently
        // position zero is never a valid match source.
        let minimum_position = position
            .saturating_sub(self.window.saturating_sub(MAX_MATCH + 3))
            .max(usize::from(!self.allow_position_zero));
        let newest_candidate =
            extra_candidate.or_else(|| candidates.and_then(|positions| positions.back().copied()));
        if newest_candidate.is_none() && self.stale_keys.contains(&key) {
            // sub_490770 keeps its hash heads across source restarts. If a
            // key only exists in a previous source segment, it returns
            // the previous lazy candidate's length instead of a zero score.
            // That quirk suppresses a pending match when the following byte
            // first encounters a stale hash head.
            return Ok(Some(MatchCandidate {
                length: 1,
                distance: 0,
                savings: i32::try_from(initial_length).unwrap_or(i32::MAX),
            }));
        }
        if candidates.is_none() && extra_candidate.is_none() {
            return Ok(None);
        }
        let mut best_full_length = 0usize;
        let mut best_stored_length = 0usize;
        let mut best_distance = 0usize;
        let mut best_savings = 0i32;
        let mut length_threshold = initial_length;
        let chain = candidates
            .into_iter()
            .flat_map(|positions| positions.iter().rev().copied());
        for candidate in extra_candidate
            .into_iter()
            .chain(chain)
            .take(candidate_limit)
        {
            if candidate < minimum_position || candidate >= position {
                continue;
            }
            if input[candidate..candidate + MIN_MATCH] != input[position..position + MIN_MATCH] {
                continue;
            }
            let current_tail = if length_threshold == 0 {
                position
                    .checked_sub(1)
                    .and_then(|start| input.get(start..start + 2))
            } else {
                let start = position + length_threshold - 1;
                input.get(start..start + 2)
            };
            let candidate_tail = if length_threshold == 0 {
                candidate
                    .checked_sub(1)
                    .and_then(|start| input.get(start..start + 2))
            } else {
                let start = candidate + length_threshold - 1;
                input.get(start..start + 2)
            };
            if current_tail.is_none() || current_tail != candidate_tail {
                continue;
            }
            let mut length = MIN_MATCH;
            while length < scan_maximum && input[candidate + length] == input[position + length] {
                length += 1;
            }
            let distance = position - candidate;
            let savings = match cost.scoring {
                MatchScoring::Heuristic => {
                    let is_recent = cost.recent_offsets[..3].contains(&distance);
                    let base_savings = (8 * length) as i32 - 17;
                    if is_recent {
                        base_savings + 4
                    } else {
                        base_savings - i32::try_from(usize::min(distance / 50, 20)).unwrap_or(20)
                    }
                }
                MatchScoring::Fixed {
                    costs,
                    dynamic_offsets,
                } => {
                    let literal_cost = input[position..position + length]
                        .iter()
                        .map(|&byte| costs.top[usize::from(byte)])
                        .sum::<i32>();
                    let code = cost
                        .recent_offsets
                        .iter()
                        .position(|&recent| recent == distance)
                        .unwrap_or(distance + 3);
                    let context = usize::min(length - MIN_MATCH, costs.offsets.len() - 1);
                    let grouped_cost =
                        costs.grouped_cost(dynamic_offsets, context, costs.offset_bits, code)?;
                    let match_cost = costs.top[length + 254].saturating_add(grouped_cost);
                    literal_cost.saturating_sub(match_cost)
                }
            };
            if savings > best_savings {
                best_savings = savings;
                best_full_length = length;
                best_stored_length = usize::min(length, stored_maximum);
                best_distance = distance;
                length_threshold = length;
                if length >= stored_maximum {
                    break;
                }
            }
        }
        Ok((best_full_length >= MIN_MATCH).then_some(MatchCandidate {
            length: best_stored_length,
            distance: best_distance,
            savings: best_savings,
        }))
    }

    fn insert(&mut self, input: &[u8], position: usize) {
        if position == 0 && !self.allow_position_zero {
            return;
        }
        let Some(key) = local_match_key(input, position) else {
            return;
        };
        let chain = self.chains.entry(key).or_default();
        if chain.back().copied() == Some(position) {
            return;
        }
        chain.push_back(position);
        if chain.len() > 10_000 {
            chain.pop_front();
        }
    }
}

pub(crate) fn local_match_key(input: &[u8], position: usize) -> Option<u32> {
    let bytes = input.get(position..position + MIN_MATCH)?;
    // sub_490300 uses a 0x8000-entry hash table and advances the two-byte
    // rolling hash as ((byte0 << 8) ^ byte1) & 0x7fff.
    Some(((u32::from(bytes[0]) << 8) ^ u32::from(bytes[1])) & 0x7fff)
}

pub(crate) fn common_match_hash(input: &[u8], position: usize, length: usize) -> Option<u32> {
    let bytes = input.get(position..position.checked_add(length)?)?;
    let shift = (length + 17) / length;
    Some(bytes.iter().fold(0u32, |hash, &byte| {
        ((hash << shift) ^ u32::from(byte)) & 0x3ffff
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_restart_hash_uses_previous_lazy_length_as_score() {
        let input = [0x10, 0x20, 0x30];
        let key = local_match_key(&input, 1).unwrap();
        let stale_keys = BTreeSet::from([key]);
        let matcher = LzMatcher::new_with_stale_keys(1 << 16, false, &stale_keys);
        let recent_offsets = [0; 4];

        let candidate = matcher
            .find(
                &input,
                1,
                2,
                2,
                2,
                10_000,
                MatchCost {
                    scoring: MatchScoring::Heuristic,
                    recent_offsets: &recent_offsets,
                },
                None,
            )
            .unwrap()
            .unwrap();

        assert_eq!(candidate.length, 1);
        assert_eq!(candidate.distance, 0);
        assert_eq!(candidate.savings, 2);
    }
}
