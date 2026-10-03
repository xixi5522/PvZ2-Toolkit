use crate::codecs::dz::range::AdaptiveModel;
use crate::codecs::dz::{DzipError, RangeSettings, Result};
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};

pub(crate) struct DzModels {
    pub(crate) top: AdaptiveModel,
    pub(crate) offsets: Vec<Vec<AdaptiveModel>>,
    pub(crate) reference_lengths: Vec<AdaptiveModel>,
    pub(crate) reference_offsets: Vec<AdaptiveModel>,
    pub(crate) reference_sign: AdaptiveModel,
    pub(crate) reference_base: AdaptiveModel,
    pub(crate) reference_bases: [i64; 8],
}

impl DzModels {
    pub(crate) fn new(settings: RangeSettings, has_combuf: bool) -> Result<Self> {
        let top_symbols = if has_combuf {
            514usize + (1usize << settings.ref_length_table_size)
        } else {
            514
        };
        let mut offsets = Vec::with_capacity(usize::from(settings.offset_contexts));
        for _ in 0..settings.offset_contexts {
            let mut context = Vec::with_capacity(usize::from(settings.offset_tables));
            for table in 0..settings.offset_tables {
                context.push(AdaptiveModel::new_with_increment(
                    1usize << settings.offset_table_size,
                    32 + 4 * u16::from(table),
                )?);
            }
            offsets.push(context);
        }
        let mut reference_lengths = Vec::with_capacity(usize::from(settings.ref_length_tables));
        for _ in 0..settings.ref_length_tables {
            reference_lengths.push(AdaptiveModel::new_with_increment(
                1usize << settings.ref_length_table_size,
                32,
            )?);
        }
        let mut reference_offsets = Vec::with_capacity(usize::from(settings.ref_offset_tables));
        for _ in 0..settings.ref_offset_tables {
            reference_offsets.push(AdaptiveModel::new_with_increment(
                1usize << settings.ref_offset_table_size,
                32,
            )?);
        }
        Ok(Self {
            top: AdaptiveModel::new(top_symbols)?,
            offsets,
            reference_lengths,
            reference_offsets,
            reference_sign: AdaptiveModel::new(2)?,
            reference_base: AdaptiveModel::new(8)?,
            reference_bases: [0; 8],
        })
    }
}

#[derive(Clone)]
pub(crate) struct CommonModels {
    pub(crate) top: AdaptiveModel,
    pub(crate) offsets: Vec<Vec<AdaptiveModel>>,
}

impl CommonModels {
    pub(crate) fn uniform(settings: RangeSettings) -> Result<Self> {
        let mut offsets = Vec::with_capacity(usize::from(settings.offset_contexts));
        for _ in 0..settings.offset_contexts {
            let mut context = Vec::with_capacity(usize::from(settings.offset_tables));
            for table in 0..settings.offset_tables {
                context.push(AdaptiveModel::new_with_increment(
                    1usize << settings.offset_table_size,
                    32 + 4 * u16::from(table),
                )?);
            }
            offsets.push(context);
        }
        Ok(Self {
            top: AdaptiveModel::new(514)?,
            offsets,
        })
    }

    pub(crate) fn from_static_prefix(prefix: &[u8], settings: RangeSettings) -> Result<Self> {
        let offset_symbols = 1usize << settings.offset_table_size;
        let expected = settings.combuf_static_prefix_size();
        if prefix.len() < expected {
            return Err(DzipError::InvalidDz(format!(
                "COMBUF static table is {} bytes, expected {}",
                prefix.len(),
                expected
            )));
        }
        let top = AdaptiveModel::from_normalized(&prefix[..514])?;
        let mut cursor = 514usize;
        let mut offsets = Vec::with_capacity(usize::from(settings.offset_contexts));
        for _ in 0..settings.offset_contexts {
            let mut context = Vec::with_capacity(usize::from(settings.offset_tables));
            for table in 0..settings.offset_tables {
                let end = cursor + offset_symbols;
                context.push(AdaptiveModel::from_normalized_with_increment(
                    &prefix[cursor..end],
                    32 + 4 * u16::from(table),
                )?);
                cursor = end;
            }
            offsets.push(context);
        }
        Ok(Self { top, offsets })
    }
}

pub(crate) struct CommonFrequencies {
    pub(crate) top: Vec<u64>,
    pub(crate) offsets: Vec<Vec<Vec<u64>>>,
}

impl CommonFrequencies {
    pub(crate) fn new(settings: RangeSettings) -> Self {
        let symbols = 1usize << settings.offset_table_size;
        Self {
            top: vec![0; 514],
            offsets: (0..settings.offset_contexts)
                .map(|_| {
                    (0..settings.offset_tables)
                        .map(|_| vec![0; symbols])
                        .collect()
                })
                .collect(),
        }
    }

    pub(crate) fn record_top(&mut self, symbol: usize) -> Result<()> {
        let frequency = self.top.get_mut(symbol).ok_or_else(|| {
            DzipError::InvalidDz(format!("invalid COMBUF analysis token {}", symbol))
        })?;
        *frequency = frequency.saturating_add(1);
        Ok(())
    }

    pub(crate) fn record_grouped(
        &mut self,
        context: usize,
        bits: u8,
        mut value: usize,
    ) -> Result<()> {
        let continuation = 1usize << (bits - 1);
        let payload_mask = continuation - 1;
        let mut table = 0usize;
        loop {
            let payload = value & payload_mask;
            value >>= bits - 1;
            let group = payload | if value != 0 { continuation } else { 0 };
            let frequency = self
                .offsets
                .get_mut(context)
                .and_then(|models| models.get_mut(table))
                .and_then(|frequencies| frequencies.get_mut(group))
                .ok_or_else(|| DzipError::InvalidDz("invalid COMBUF analysis group".to_string()))?;
            *frequency = frequency.saturating_add(1);
            if value == 0 {
                return Ok(());
            }
            table = usize::min(table + 1, self.offsets[context].len() - 1);
        }
    }

    pub(crate) fn normalized_prefix(&self) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(
            self.top.len()
                + self
                    .offsets
                    .iter()
                    .flat_map(|context| context.iter())
                    .map(Vec::len)
                    .sum::<usize>(),
        );
        append_normalized_with_pseudocount(&mut prefix, &self.top);
        for context in &self.offsets {
            for table in context {
                append_normalized_with_pseudocount(&mut prefix, table);
            }
        }
        prefix
    }

    pub(crate) fn fixed_costs(&self, settings: RangeSettings) -> DzFixedCosts {
        DzFixedCosts {
            top: fixed_symbol_costs(&self.top),
            offsets: self
                .offsets
                .iter()
                .map(|context| {
                    context
                        .iter()
                        .map(|frequencies| fixed_symbol_costs(frequencies))
                        .collect()
                })
                .collect(),
            offset_bits: settings.offset_table_size,
        }
    }
}

fn append_normalized_with_pseudocount(output: &mut Vec<u8>, frequencies: &[u64]) {
    // sub_48F5C0 builds costs from the raw first-pass counts, then adds one
    // to every symbol before sub_48EC40 writes the static tables.
    let maximum = frequencies
        .iter()
        .map(|&frequency| frequency.saturating_add(1))
        .max()
        .unwrap_or(1);
    output.extend(frequencies.iter().map(|&frequency| {
        (((u128::from(frequency.saturating_add(1)) * 255) / u128::from(maximum)) as u8).max(1)
    }));
}

pub(crate) struct DzFrequencyCounts {
    top: Vec<u64>,
    pub(crate) offsets: Vec<Vec<Vec<u64>>>,
    offset_bits: u8,
}

impl DzFrequencyCounts {
    pub(crate) fn new(settings: RangeSettings, has_combuf: bool) -> Self {
        let top_symbols = if has_combuf {
            514usize + (1usize << settings.ref_length_table_size)
        } else {
            514
        };
        let offset_symbols = 1usize << settings.offset_table_size;
        Self {
            top: vec![0; top_symbols],
            offsets: (0..settings.offset_contexts)
                .map(|_| {
                    (0..settings.offset_tables)
                        .map(|_| vec![0; offset_symbols])
                        .collect()
                })
                .collect(),
            offset_bits: settings.offset_table_size,
        }
    }

    pub(crate) fn record_top(&mut self, symbol: usize) -> Result<()> {
        let frequency = self
            .top
            .get_mut(symbol)
            .ok_or_else(|| DzipError::InvalidDz(format!("invalid DZ analysis token {}", symbol)))?;
        *frequency = frequency.saturating_add(1);
        Ok(())
    }

    pub(crate) fn record_grouped(
        &mut self,
        context: usize,
        bits: u8,
        mut value: usize,
    ) -> Result<()> {
        let continuation = 1usize << (bits - 1);
        let payload_mask = continuation - 1;
        let mut table = 0usize;
        loop {
            let payload = value & payload_mask;
            value >>= bits - 1;
            let group = payload | if value != 0 { continuation } else { 0 };
            let frequency = self
                .offsets
                .get_mut(context)
                .and_then(|models| models.get_mut(table))
                .and_then(|frequencies| frequencies.get_mut(group))
                .ok_or_else(|| {
                    DzipError::InvalidDz("invalid DZ analysis offset group".to_string())
                })?;
            *frequency = frequency.saturating_add(1);
            if value == 0 {
                return Ok(());
            }
            table = usize::min(table + 1, self.offsets[context].len() - 1);
        }
    }

    pub(crate) fn into_costs(self) -> DzFixedCosts {
        let offset_costs = self
            .offsets
            .iter()
            .map(|context| {
                context
                    .iter()
                    .map(|frequencies| fixed_symbol_costs(frequencies))
                    .collect()
            })
            .collect();
        DzFixedCosts {
            top: fixed_symbol_costs(&self.top),
            offsets: offset_costs,
            offset_bits: self.offset_bits,
        }
    }
}

pub(crate) struct DzFixedCosts {
    pub(crate) top: Vec<i32>,
    pub(crate) offsets: Vec<Vec<Vec<i32>>>,
    pub(crate) offset_bits: u8,
}

impl DzFixedCosts {
    pub(crate) fn grouped_cost(
        &self,
        dynamic_offsets: &[Vec<AdaptiveModel>],
        context: usize,
        bits: u8,
        mut value: usize,
    ) -> Result<i32> {
        let dynamic = value >= 2_000;
        let continuation = 1usize << (bits - 1);
        let payload_mask = continuation - 1;
        let mut table = 0usize;
        let mut cost = 0i32;
        loop {
            let payload = value & payload_mask;
            value >>= bits - 1;
            let group = payload | if value != 0 { continuation } else { 0 };
            let group_cost = if dynamic {
                dynamic_offsets
                    .get(context)
                    .and_then(|models| models.get(table))
                    .ok_or_else(|| {
                        DzipError::InvalidDz("invalid dynamic DZ offset cost".to_string())
                    })?
                    .estimated_cost(group)?
            } else {
                self.offsets
                    .get(context)
                    .and_then(|models| models.get(table))
                    .and_then(|costs| costs.get(group))
                    .copied()
                    .ok_or_else(|| {
                        DzipError::InvalidDz("invalid fixed DZ offset cost".to_string())
                    })?
            };
            cost = cost.saturating_add(group_cost);
            if value == 0 {
                return Ok(cost);
            }
            table = usize::min(table + 1, self.offsets[context].len() - 1);
        }
    }
}

fn fixed_symbol_costs(frequencies: &[u64]) -> Vec<i32> {
    let total = frequencies.iter().copied().sum::<u64>();
    let total_bits = bit_length_u64(total >> 1);
    frequencies
        .iter()
        .map(|&frequency| total_bits - bit_length_u64(frequency >> 1))
        .collect()
}

fn bit_length_u64(value: u64) -> i32 {
    (u64::BITS - value.leading_zeros()) as i32
}
