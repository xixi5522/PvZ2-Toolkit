use std::collections::HashMap;
use std::sync::LazyLock;

use crate::vcdiff::VcdiffFormat;
use crate::vcdiff::address_cache::AddressEncoder;
use crate::vcdiff::code_table::{InstructionKind, default_instructions};
use crate::vcdiff::io::write_varint;

use super::block_hash::Operation;

pub(super) fn encode_operations(
    operations: &[Operation<'_>],
    format: VcdiffFormat,
    source_size: usize,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut data = Vec::new();
    let mut addresses = Vec::new();
    let mut instructions = InstructionWriter::default();
    let mut address_cache = AddressEncoder::default();
    let mut target_position = 0_usize;
    for operation in operations {
        match *operation {
            Operation::Add(bytes) => {
                instructions.write(InstructionKind::Add, bytes.len());
                match format {
                    VcdiffFormat::Standard => data.extend_from_slice(bytes),
                    VcdiffFormat::Interleaved => instructions.bytes.extend_from_slice(bytes),
                }
                target_position += bytes.len();
            }
            Operation::Copy { address, size } => {
                let here = source_size + target_position;
                let (mode, encoded) = address_cache.encode(address, here);
                instructions.write(InstructionKind::Copy(mode), size);
                match format {
                    VcdiffFormat::Standard if mode >= 6 => addresses.push(encoded as u8),
                    VcdiffFormat::Standard => write_varint(encoded, &mut addresses),
                    VcdiffFormat::Interleaved if mode >= 6 => {
                        instructions.bytes.push(encoded as u8);
                    }
                    VcdiffFormat::Interleaved => {
                        write_varint(encoded, &mut instructions.bytes);
                    }
                }
                target_position += size;
            }
        }
    }
    (data, instructions.bytes, addresses)
}

#[derive(Default)]
struct InstructionWriter {
    bytes: Vec<u8>,
    last_opcode: Option<usize>,
}

impl InstructionWriter {
    fn write(&mut self, kind: InstructionKind, size: usize) {
        if let Some(index) = self.last_opcode
            && let Some((opcode, explicit_size)) =
                INSTRUCTION_MAP.lookup_second(self.bytes[index], kind, size)
        {
            self.bytes[index] = opcode;
            if explicit_size {
                write_varint(size, &mut self.bytes);
            }
            self.last_opcode = None;
            return;
        }

        let (opcode, explicit_size) = INSTRUCTION_MAP
            .lookup_first(kind, size)
            .expect("the RFC default code table contains every instruction kind and mode");
        let index = self.bytes.len();
        self.bytes.push(opcode);
        if explicit_size {
            write_varint(size, &mut self.bytes);
        }
        self.last_opcode = Some(index);
    }
}

static INSTRUCTION_MAP: LazyLock<InstructionMap> = LazyLock::new(InstructionMap::new);

struct InstructionMap {
    first: HashMap<(u8, u8, usize), u8>,
    variable_first: HashMap<(u8, u8), u8>,
    second: HashMap<(u8, u8, u8, usize), u8>,
    variable_second: HashMap<(u8, u8, u8), u8>,
}

impl InstructionMap {
    fn new() -> Self {
        let mut result = Self {
            first: HashMap::new(),
            variable_first: HashMap::new(),
            second: HashMap::new(),
            variable_second: HashMap::new(),
        };
        for opcode in u8::MIN..=u8::MAX {
            let [first, second] = default_instructions(opcode);
            if second.is_none()
                && let Some((kind, size)) = first
            {
                let (kind, mode) = instruction_key(kind);
                result.first.entry((kind, mode, size)).or_insert(opcode);
                if size == 0 {
                    result.variable_first.entry((kind, mode)).or_insert(opcode);
                }
            }
        }
        for opcode in u8::MIN..=u8::MAX {
            let [
                Some((first_kind, first_size)),
                Some((second_kind, second_size)),
            ] = default_instructions(opcode)
            else {
                continue;
            };
            let (first_kind, first_mode) = instruction_key(first_kind);
            let Some(&single_opcode) = result.first.get(&(first_kind, first_mode, first_size))
            else {
                continue;
            };
            let (second_kind, second_mode) = instruction_key(second_kind);
            result
                .second
                .entry((single_opcode, second_kind, second_mode, second_size))
                .or_insert(opcode);
            if second_size == 0 {
                result
                    .variable_second
                    .entry((single_opcode, second_kind, second_mode))
                    .or_insert(opcode);
            }
        }
        result
    }

    fn lookup_first(&self, kind: InstructionKind, size: usize) -> Option<(u8, bool)> {
        let (kind, mode) = instruction_key(kind);
        self.first
            .get(&(kind, mode, size))
            .copied()
            .map(|opcode| (opcode, false))
            .or_else(|| {
                self.variable_first
                    .get(&(kind, mode))
                    .copied()
                    .map(|opcode| (opcode, true))
            })
    }

    fn lookup_second(
        &self,
        first_opcode: u8,
        kind: InstructionKind,
        size: usize,
    ) -> Option<(u8, bool)> {
        let (kind, mode) = instruction_key(kind);
        self.second
            .get(&(first_opcode, kind, mode, size))
            .copied()
            .map(|opcode| (opcode, false))
            .or_else(|| {
                self.variable_second
                    .get(&(first_opcode, kind, mode))
                    .copied()
                    .map(|opcode| (opcode, true))
            })
    }
}

fn instruction_key(kind: InstructionKind) -> (u8, u8) {
    match kind {
        InstructionKind::Add => (1, 0),
        InstructionKind::Run => (2, 0),
        InstructionKind::Copy(mode) => (3, mode),
    }
}
