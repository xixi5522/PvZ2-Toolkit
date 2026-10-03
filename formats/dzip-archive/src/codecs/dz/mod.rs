//! Native Dzip range/LZ and archive-wide common-buffer codec.

mod archive;
mod chunk;
mod codec;
mod error;
mod matchfinder;
mod model;
mod options;
mod range;
mod settings;

pub use archive::common::DzCommonBuffer;
#[allow(unused_imports)]
pub use archive::{DzEncoderOptions, EncodedDzArchive, compress_archive, compress_archive_slices};
#[allow(unused_imports)]
pub use chunk::{compress_chunk, decompress_chunk, decompress_chunk_with_common_buffer};
#[allow(unused_imports)]
pub use codec::{Decoder, Encoder, decode, encode};
#[allow(unused_imports)]
pub use error::{DzError, ErrorKind, Result};
#[allow(unused_imports)]
pub use options::{DecoderOptions, EncoderOptions, ResourceLimits, model_workspace_size};
pub use settings::RangeSettings;

pub(crate) use DzError as DzipError;

pub(crate) const END_SYMBOL: usize = 513;
pub(crate) const MIN_MATCH: usize = 2;
pub(crate) const MAX_MATCH: usize = 258;

#[cfg(test)]
mod tests;
