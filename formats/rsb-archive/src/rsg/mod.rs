pub mod pack;
pub mod types;
pub mod unpack;
pub mod zlib;

pub use pack::pack_rsg;
pub use types::{
    Part0Info, Part1Extra, Part1Info, RSG_FOURCC, RSG_MAGIC, RsgHeader, RsgPayload, UnpackedFile,
};
pub use unpack::unpack_rsg;
pub use zlib::{
    DEFAULT_ZLIB_LEVEL, RSG_DATA_ALIGNMENT, compress_rsg_zlib, compress_rsg_zlib_with_level,
    compress_zlib, compress_zlib_with_level, decompress_zlib, decompress_zlib_exact,
    is_zlib_stream,
};
