use crate::Result;

use super::invalid;

pub(super) const SERIALIZED_CODE_TABLE_SIZE: usize = 6 * 256;

#[derive(Clone, Copy, Debug)]
pub(super) enum InstructionKind {
    Add,
    Run,
    Copy(u8),
}

pub(super) type TableInstruction = Option<(InstructionKind, usize)>;

#[derive(Clone, Copy, Debug, Default)]
struct RawTableInstruction {
    kind: u8,
    size: u8,
    mode: u8,
}

pub(super) struct DecoderCodeTable {
    entries: [[RawTableInstruction; 2]; 256],
}

impl Default for DecoderCodeTable {
    fn default() -> Self {
        let mut entries = [[RawTableInstruction::default(); 2]; 256];
        for opcode in u8::MIN..=u8::MAX {
            for (index, instruction) in default_instructions(opcode).into_iter().enumerate() {
                entries[usize::from(opcode)][index] = match instruction {
                    None => RawTableInstruction::default(),
                    Some((InstructionKind::Add, size)) => RawTableInstruction {
                        kind: 1,
                        size: size as u8,
                        mode: 0,
                    },
                    Some((InstructionKind::Run, size)) => RawTableInstruction {
                        kind: 2,
                        size: size as u8,
                        mode: 0,
                    },
                    Some((InstructionKind::Copy(mode), size)) => RawTableInstruction {
                        kind: 3,
                        size: size as u8,
                        mode,
                    },
                };
            }
        }
        Self { entries }
    }
}

impl DecoderCodeTable {
    pub fn from_serialized(data: &[u8], near_cache: usize, same_cache: usize) -> Result<Self> {
        if data.len() != SERIALIZED_CODE_TABLE_SIZE {
            return Err(invalid(format!(
                "custom code table has {} bytes, expected {SERIALIZED_CODE_TABLE_SIZE}",
                data.len()
            )));
        }
        let mut entries = [[RawTableInstruction::default(); 2]; 256];
        for opcode in 0..256 {
            entries[opcode] = [
                RawTableInstruction {
                    kind: data[opcode],
                    size: data[512 + opcode],
                    mode: data[1024 + opcode],
                },
                RawTableInstruction {
                    kind: data[256 + opcode],
                    size: data[768 + opcode],
                    mode: data[1280 + opcode],
                },
            ];
        }
        let result = Self { entries };
        result.validate(near_cache + same_cache + 1)?;
        Ok(result)
    }

    pub fn instructions(&self, opcode: u8) -> Result<[TableInstruction; 2]> {
        let raw = self.entries[usize::from(opcode)];
        Ok([decode_instruction(raw[0])?, decode_instruction(raw[1])?])
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result = vec![0_u8; SERIALIZED_CODE_TABLE_SIZE];
        for (opcode, entries) in self.entries.iter().enumerate() {
            result[opcode] = entries[0].kind;
            result[256 + opcode] = entries[1].kind;
            result[512 + opcode] = entries[0].size;
            result[768 + opcode] = entries[1].size;
            result[1024 + opcode] = entries[0].mode;
            result[1280 + opcode] = entries[1].mode;
        }
        result
    }

    fn validate(&self, max_mode: usize) -> Result<()> {
        let mut required = vec![false; 4 + max_mode];
        for (opcode, entries) in self.entries.iter().enumerate() {
            for entry in entries {
                if entry.kind > 3 {
                    return Err(invalid(format!(
                        "custom code table opcode {opcode} has invalid instruction {}",
                        entry.kind
                    )));
                }
                if usize::from(entry.mode) > max_mode {
                    return Err(invalid(format!(
                        "custom code table opcode {opcode} has invalid mode {}",
                        entry.mode
                    )));
                }
                if entry.kind == 0 && entry.size != 0 {
                    return Err(invalid(format!(
                        "custom code table opcode {opcode} has a sized NOOP"
                    )));
                }
                if entry.kind != 3 && entry.mode != 0 {
                    return Err(invalid(format!(
                        "custom code table opcode {opcode} uses a COPY mode for a non-COPY instruction"
                    )));
                }
            }
            let [first, second] = entries;
            if first.size == 0 && second.kind == 0 {
                let key = usize::from(first.kind) + usize::from(first.mode);
                if key < required.len() {
                    required[key] = true;
                }
            }
        }
        for (key, exists) in required.iter().enumerate().skip(1) {
            if !exists {
                return Err(invalid(format!(
                    "custom code table has no variable-size opcode for instruction/mode {key}"
                )));
            }
        }
        Ok(())
    }
}

pub(super) fn validate_cache_sizes(near: usize, same: usize) -> Result<()> {
    if near > 254 || same > 254 || near + same > 254 {
        return Err(invalid(format!(
            "invalid VCDIFF address cache sizes: near={near}, same={same}"
        )));
    }
    Ok(())
}

pub(super) fn default_instructions(opcode: u8) -> [TableInstruction; 2] {
    match opcode {
        0 => [Some((InstructionKind::Run, 0)), None],
        1..=18 => [Some((InstructionKind::Add, usize::from(opcode - 1))), None],
        19..=162 => {
            let relative = usize::from(opcode - 19);
            let mode = (relative / 16) as u8;
            let slot = relative % 16;
            let size = if slot == 0 { 0 } else { slot + 3 };
            [Some((InstructionKind::Copy(mode), size)), None]
        }
        163..=234 => {
            let relative = usize::from(opcode - 163);
            let mode = (relative / 12) as u8;
            let pair = relative % 12;
            [
                Some((InstructionKind::Add, pair / 3 + 1)),
                Some((InstructionKind::Copy(mode), pair % 3 + 4)),
            ]
        }
        235..=246 => {
            let relative = usize::from(opcode - 235);
            [
                Some((InstructionKind::Add, relative % 4 + 1)),
                Some((InstructionKind::Copy((relative / 4 + 6) as u8), 4)),
            ]
        }
        247..=255 => [
            Some((InstructionKind::Copy(opcode - 247), 4)),
            Some((InstructionKind::Add, 1)),
        ],
    }
}

fn decode_instruction(raw: RawTableInstruction) -> Result<TableInstruction> {
    let kind = match raw.kind {
        0 => return Ok(None),
        1 => InstructionKind::Add,
        2 => InstructionKind::Run,
        3 => InstructionKind::Copy(raw.mode),
        invalid_kind => {
            return Err(invalid(format!(
                "invalid code table instruction {invalid_kind}"
            )));
        }
    };
    Ok(Some((kind, usize::from(raw.size))))
}
