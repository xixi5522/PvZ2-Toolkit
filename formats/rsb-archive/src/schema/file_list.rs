use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Read, Result, Seek, SeekFrom, Write};

pub trait FileListPayload: Sized {
    fn read(reader: &mut impl Read) -> Result<Self>;
    fn write(&self, writer: &mut impl Write) -> Result<()>;
}

impl FileListPayload for i32 {
    fn read(reader: &mut impl Read) -> Result<Self> {
        reader.read_i32::<LE>()
    }

    fn write(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_i32::<LE>(*self)?;
        Ok(())
    }
}

pub fn read_file_list<P: FileListPayload, R: Read + Seek>(
    reader: &mut R,
    start_offset: u64,
    length: u64,
) -> Result<Vec<(String, P)>> {
    reader.seek(SeekFrom::Start(start_offset))?;
    let limit = start_offset + length;

    let mut file_list = Vec::new();
    let mut name_dict: HashMap<u64, String> = HashMap::new();
    let mut current_path = String::new();

    while reader.stream_position()? < limit {
        let char_byte = reader.read_u8()?;
        let offset_val = reader.read_u24::<LE>()? as u64 * 4;

        // The offset points to a "Sibling" node which shares the same prefix as the current node.
        // Therefore, we must register the *current* path (before appending char) for that offset.
        if offset_val != 0 {
            name_dict.insert(offset_val + start_offset, current_path.clone());
        }

        if char_byte == 0 {
            let payload = P::read(reader)?;
            file_list.push((current_path.clone(), payload));

            // Logic check: When we finish a file (char=0), we might "jump" if this node was a target.

            let current_pos = reader.stream_position()?;
            if let Some(path) = name_dict.remove(&current_pos) {
                current_path = path;
            }
        } else {
            current_path.push(char_byte as char);
            // Note: We ALREADY handled insertion for offset_val above using the prefix.
        }
    }

    Ok(file_list)
}

pub fn write_file_list<P: FileListPayload + Clone, W: Write + Seek>(
    writer: &mut W,
    start_offset: u64,
    items: &[(String, P)],
) -> Result<()> {
    // Robust Packer with Prefix Compression

    let mut sorted_items = items.to_vec();

    sorted_items.sort_by_key(|item| item.0.to_uppercase());

    // Track active prefix states: (Path, FilePositionOfOffsetField)
    // The FilePositionOfOffsetField is where we wrote the 0 that we need to update later.
    let mut active_prefixes: HashMap<String, u64> = HashMap::new();

    // We also need to track the "current path" state of the artificial parser
    let mut current_parser_path = String::new();

    for (path, payload) in sorted_items {
        // Compute LCP between `current_parser_path` and `path`

        // Find split point
        let mut split_idx = 0;
        let char_indices: Vec<(usize, char)> = path.char_indices().collect();

        for i in (0..=char_indices.len()).rev() {
            let prefix = if i == char_indices.len() {
                path.clone()
            } else {
                path[..char_indices[i].0].to_string()
            };

            if let Some(&instr_pos) = active_prefixes.get(&prefix) {
                // Found a reusable state!
                // Update the instruction at `instr_pos` to point to HERE.
                // Calculate Target Offset: (CurrentPos - start_offset) / 4.

                let current_pos = writer.stream_position()?;
                let target_val = (current_pos - start_offset) / 4;

                // Update the previous jump instruction
                let saved_pos = writer.stream_position()?;
                writer.seek(SeekFrom::Start(instr_pos))?;
                writer.write_u24::<LE>(target_val as u32)?; // Update the offset
                writer.seek(SeekFrom::Start(saved_pos))?;

                // Remove from active_prefixes (consumed)
                active_prefixes.remove(&prefix);

                current_parser_path = prefix;
                if i < char_indices.len() {
                    split_idx = i;
                } else {
                    split_idx = char_indices.len();
                }
                break;
            }
        }

        // Write Suffix
        // Suffix starts after `best_prefix`.

        let suffix_chars = &char_indices[split_idx..];

        for (_, c) in suffix_chars.iter() {
            // Write Char
            writer.write_u8(*c as u8)?;

            // Write Placeholder Offset
            let offset_pos = writer.stream_position()?;
            writer.write_u24::<LE>(0)?;

            // Record this new state as available for future jumps
            // State: current_parser_path (before c) is the context for this sibling pointer
            active_prefixes.insert(current_parser_path.clone(), offset_pos);
            current_parser_path.push(*c);
        }

        // Finish string with \0
        writer.write_u8(0)?;
        let offset_pos = writer.stream_position()?;
        writer.write_u24::<LE>(0)?;

        // State after \0 is valid for next item
        active_prefixes.insert(current_parser_path.clone(), offset_pos);

        // Write Payload
        payload.write(writer)?;
    }

    Ok(())
}
