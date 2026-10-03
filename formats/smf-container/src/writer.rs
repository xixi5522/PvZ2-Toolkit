use std::io::Write;

use byteorder::{LittleEndian, WriteBytesExt};
use flate2::{Compression, write::ZlibEncoder};

use crate::{EncodeOptions, Result, SMF_MAGIC, SmfError, SmfMetadata, SmfVariant};

/// Encode a payload into a new SMF byte vector.
pub fn encode(data: &[u8], options: EncodeOptions) -> Result<Vec<u8>> {
    let header_len = usize::try_from(options.variant.header_len()).unwrap_or(16);
    let mut output = Vec::with_capacity(data.len().saturating_add(header_len));
    encode_to(&mut output, data, options)?;
    Ok(output)
}

/// Encode a payload directly into a writer.
pub fn encode_to<W: Write>(
    mut writer: W,
    data: &[u8],
    options: EncodeOptions,
) -> Result<SmfMetadata> {
    if options.compression_level > 9 {
        return Err(SmfError::InvalidCompressionLevel {
            level: options.compression_level,
        });
    }
    let size = data.len() as u64;
    match options.variant {
        SmfVariant::Compact32 => {
            let size = u32::try_from(size).map_err(|_| SmfError::CompactSizeOverflow {
                size: data.len() as u64,
            })?;
            writer.write_u32::<LittleEndian>(SMF_MAGIC)?;
            writer.write_u32::<LittleEndian>(size)?;
        }
        SmfVariant::Extended64 => {
            writer.write_u64::<LittleEndian>(u64::from(SMF_MAGIC))?;
            writer.write_u64::<LittleEndian>(size)?;
        }
    }

    let mut encoder = ZlibEncoder::new(writer, Compression::new(options.compression_level));
    encoder.write_all(data)?;
    encoder.try_finish()?;
    let compressed_size = encoder.total_out();
    encoder.finish()?;

    Ok(SmfMetadata {
        variant: options.variant,
        uncompressed_size: size,
        compressed_size,
        total_size: options.variant.header_len() + compressed_size,
    })
}
