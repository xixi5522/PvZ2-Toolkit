//! BNK container and chunk reader/writer.

mod reader;
mod writer;

pub(crate) use reader::{
    from_bytes_with_options, from_reader_with_options, parse_chunk, parse_header,
    validate_media_pairs, validate_twinning_chunks,
};
pub(crate) use writer::validate_sound_bank;
pub(crate) use writer::{to_bytes, to_writer};
