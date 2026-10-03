use crate::error::ReanimError;
use crate::types::{Reanim, ReanimTrack, ReanimTransform, ReanimVersion};
use byteorder::{LE, ReadBytesExt};
use flate2::read::ZlibDecoder;
use std::borrow::Cow;
use std::io::{Cursor, Read};

const POPCAP_ZLIB_MAGIC: u32 = 0xDEAD_FED4;
const MAX_TRACKS: usize = 1_000_000;
const MAX_TRANSFORMS_PER_TRACK: usize = 10_000_000;
const MAX_STRING_LENGTH: usize = 64 * 1024 * 1024;
const MAX_UNCOMPRESSED_SIZE: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy)]
enum ImageField {
    String,
    Integer,
}

#[derive(Clone, Copy)]
struct Layout {
    version: ReanimVersion,
    phone64_wrapper: bool,
    payload_prefix: usize,
    header_reserved: usize,
    layout_magic: u32,
    track_prefix: usize,
    track_suffix: usize,
    track_magic: u32,
    transform_reserved: usize,
    image_field: ImageField,
}

const PC_LAYOUT: Layout = Layout {
    version: ReanimVersion::PC,
    phone64_wrapper: false,
    payload_prefix: 8,
    header_reserved: 4,
    layout_magic: 0x0C,
    track_prefix: 8,
    track_suffix: 0,
    track_magic: 0x2C,
    transform_reserved: 12,
    image_field: ImageField::String,
};

const PHONE32_LAYOUT: Layout = Layout {
    version: ReanimVersion::Phone32,
    phone64_wrapper: false,
    payload_prefix: 8,
    header_reserved: 4,
    layout_magic: 0x10,
    track_prefix: 12,
    track_suffix: 0,
    track_magic: 0x2C,
    transform_reserved: 12,
    image_field: ImageField::Integer,
};

const PHONE64_LAYOUT: Layout = Layout {
    version: ReanimVersion::Phone64,
    phone64_wrapper: true,
    payload_prefix: 12,
    header_reserved: 8,
    layout_magic: 0x20,
    track_prefix: 24,
    track_suffix: 4,
    track_magic: 0x38,
    transform_reserved: 24,
    image_field: ImageField::Integer,
};

fn checked_len(kind: &'static str, value: i32, max: usize) -> Result<usize, ReanimError> {
    let value = usize::try_from(value).map_err(|_| ReanimError::InvalidLength {
        kind,
        value: i64::from(value),
    })?;
    if value > max {
        return Err(ReanimError::LimitExceeded { kind, value, max });
    }
    Ok(value)
}

fn read_string<R: Read>(reader: &mut R) -> Result<String, ReanimError> {
    let len = checked_len("string", reader.read_i32::<LE>()?, MAX_STRING_LENGTH)?;
    if len == 0 {
        return Ok(String::new());
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8(bytes).map_err(|_| ReanimError::StringDecodeError)
}

fn skip<R: Read>(reader: &mut R, count: usize) -> Result<(), ReanimError> {
    debug_assert!(count <= 24);
    let mut reserved = [0_u8; 24];
    reader.read_exact(&mut reserved[..count])?;
    Ok(())
}

fn unwrap_payload(data: &[u8], phone64: bool) -> Result<Cow<'_, [u8]>, ReanimError> {
    if data.len() < 4 || u32::from_le_bytes(data[..4].try_into().unwrap()) != POPCAP_ZLIB_MAGIC {
        return Ok(Cow::Borrowed(data));
    }

    let header_size = if phone64 { 16 } else { 8 };
    if data.len() < header_size {
        return Err(ReanimError::InvalidContainer);
    }

    let declared = if phone64 {
        if data[4..8] != [0; 4] || data[12..16] != [0; 4] {
            return Err(ReanimError::InvalidContainer);
        }
        u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize
    } else {
        u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize
    };
    if declared > MAX_UNCOMPRESSED_SIZE {
        return Err(ReanimError::LimitExceeded {
            kind: "uncompressed payload",
            value: declared,
            max: MAX_UNCOMPRESSED_SIZE,
        });
    }

    let mut payload = Vec::with_capacity(declared.min(16 * 1024 * 1024));
    ZlibDecoder::new(&data[header_size..])
        .take(declared as u64 + 1)
        .read_to_end(&mut payload)?;
    if payload.len() != declared {
        return Err(ReanimError::SizeMismatch {
            expected: declared,
            actual: payload.len(),
        });
    }
    Ok(Cow::Owned(payload))
}

fn read_optional_f32<R: Read>(reader: &mut R) -> Result<Option<f32>, ReanimError> {
    let value = reader.read_f32::<LE>()?;
    Ok((value != -10_000.0).then_some(value))
}

fn decode_layout(data: &[u8], layout: Layout) -> Result<Reanim, ReanimError> {
    let payload = unwrap_payload(data, layout.phone64_wrapper)?;
    let mut reader = Cursor::new(payload.as_ref());

    skip(&mut reader, layout.payload_prefix)?;
    let track_count = checked_len("track", reader.read_i32::<LE>()?, MAX_TRACKS)?;
    let fps = reader.read_f32::<LE>()?;
    skip(&mut reader, layout.header_reserved)?;

    let actual_magic = reader.read_u32::<LE>()?;
    if actual_magic != layout.layout_magic {
        return Err(ReanimError::InvalidMagic(layout.layout_magic, actual_magic));
    }

    let mut tracks = Vec::with_capacity(track_count);
    for _ in 0..track_count {
        skip(&mut reader, layout.track_prefix)?;
        let transform_count = checked_len(
            "transform",
            reader.read_i32::<LE>()?,
            MAX_TRANSFORMS_PER_TRACK,
        )?;
        skip(&mut reader, layout.track_suffix)?;
        tracks.push(ReanimTrack {
            name: String::new(),
            transforms: vec![ReanimTransform::default(); transform_count],
        });
    }

    for track in &mut tracks {
        track.name = read_string(&mut reader)?;
        if reader.read_u32::<LE>()? != layout.track_magic {
            return Err(ReanimError::InvalidTrack);
        }

        for transform in &mut track.transforms {
            *transform = ReanimTransform {
                x: read_optional_f32(&mut reader)?,
                y: read_optional_f32(&mut reader)?,
                kx: read_optional_f32(&mut reader)?,
                ky: read_optional_f32(&mut reader)?,
                sx: read_optional_f32(&mut reader)?,
                sy: read_optional_f32(&mut reader)?,
                f: read_optional_f32(&mut reader)?,
                a: read_optional_f32(&mut reader)?,
                ..Default::default()
            };
            skip(&mut reader, layout.transform_reserved)?;
        }

        for transform in &mut track.transforms {
            transform.i = match layout.image_field {
                ImageField::String => {
                    let value = read_string(&mut reader)?;
                    (!value.is_empty()).then_some(value)
                }
                ImageField::Integer => {
                    let value = reader.read_i32::<LE>()?;
                    (value != -1).then(|| value.to_string())
                }
            };

            let font = read_string(&mut reader)?;
            transform.font = (!font.is_empty()).then_some(font);
            let text = read_string(&mut reader)?;
            transform.text = (!text.is_empty()).then_some(text);
        }
    }

    Ok(Reanim {
        do_scale: None,
        fps,
        tracks,
    })
}

/// Decodes a PC REANIM payload or PopCap zlib container.
pub fn decode_pc(data: &[u8]) -> Result<Reanim, ReanimError> {
    decode_layout(data, PC_LAYOUT)
}

/// Decodes a 32-bit mobile REANIM payload or PopCap zlib container.
pub fn decode_phone32(data: &[u8]) -> Result<Reanim, ReanimError> {
    decode_layout(data, PHONE32_LAYOUT)
}

/// Decodes a 64-bit mobile REANIM payload or PopCap zlib container.
pub fn decode_phone64(data: &[u8]) -> Result<Reanim, ReanimError> {
    decode_layout(data, PHONE64_LAYOUT)
}

/// Decodes REANIM data and returns the detected compiled layout.
pub fn decode_with_version(data: &[u8]) -> Result<(Reanim, ReanimVersion), ReanimError> {
    for layout in [PC_LAYOUT, PHONE32_LAYOUT, PHONE64_LAYOUT] {
        if let Ok(reanim) = decode_layout(data, layout) {
            return Ok((reanim, layout.version));
        }
    }
    Err(ReanimError::InvalidVariant)
}

/// Decodes REANIM data and automatically detects its compiled layout.
pub fn decode(data: &[u8]) -> Result<Reanim, ReanimError> {
    decode_with_version(data).map(|(reanim, _)| reanim)
}
