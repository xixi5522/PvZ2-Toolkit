use crate::error::ReanimError;
use crate::types::{Reanim, ReanimVersion};
use byteorder::{LE, WriteBytesExt};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

const POPCAP_ZLIB_MAGIC: u32 = 0xDEAD_FED4;

fn encode_len(kind: &'static str, value: usize) -> Result<i32, ReanimError> {
    i32::try_from(value).map_err(|_| ReanimError::LengthOverflow { kind, value })
}

fn write_string_by_int32<W: Write>(writer: &mut W, s: &str) -> Result<(), ReanimError> {
    writer.write_i32::<LE>(encode_len("string", s.len())?)?;
    writer.write_all(s.as_bytes())?;
    Ok(())
}

fn write_opt_f32<W: Write>(writer: &mut W, val: Option<f32>) -> Result<(), ReanimError> {
    writer.write_f32::<LE>(val.unwrap_or(-10000.0))?;
    Ok(())
}

pub fn encode(reanim: &Reanim, version: ReanimVersion) -> Result<Vec<u8>, ReanimError> {
    let mut buf = Vec::new();
    match version {
        ReanimVersion::PC => encode_pc(&mut buf, reanim)?,
        ReanimVersion::Phone32 => encode_phone32(&mut buf, reanim)?,
        ReanimVersion::Phone64 => encode_phone64(&mut buf, reanim)?,
    }

    // Now PopCapZlib compress it, as the C# code does for all variants
    let mut out_buf = Vec::new();
    out_buf.write_u32::<LE>(POPCAP_ZLIB_MAGIC)?;

    // For Phone64, there is a weird extra 0 before the length
    if version == ReanimVersion::Phone64 {
        out_buf.write_i32::<LE>(0)?;
    }

    out_buf.write_i32::<LE>(encode_len("uncompressed payload", buf.len())?)?;

    // For Phone64, there is a weird extra 0 after the length
    if version == ReanimVersion::Phone64 {
        out_buf.write_i32::<LE>(0)?;
    }

    let mut encoder = ZlibEncoder::new(out_buf, Compression::default());
    encoder.write_all(&buf)?;
    Ok(encoder.finish()?)
}

fn encode_pc<W: Write>(writer: &mut W, reanim: &Reanim) -> Result<(), ReanimError> {
    writer.write_i32::<LE>(-1282165568)?;
    writer.write_i32::<LE>(0)?;
    let tracks_number = encode_len("track", reanim.tracks.len())?;
    writer.write_i32::<LE>(tracks_number)?;
    writer.write_f32::<LE>(reanim.fps)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0x0C)?;

    for track in &reanim.tracks {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(encode_len("transform", track.transforms.len())?)?;
    }

    for track in &reanim.tracks {
        write_string_by_int32(writer, &track.name)?;
        writer.write_i32::<LE>(0x2C)?;

        for ts in &track.transforms {
            write_opt_f32(writer, ts.x)?;
            write_opt_f32(writer, ts.y)?;
            write_opt_f32(writer, ts.kx)?;
            write_opt_f32(writer, ts.ky)?;
            write_opt_f32(writer, ts.sx)?;
            write_opt_f32(writer, ts.sy)?;
            write_opt_f32(writer, ts.f)?;
            write_opt_f32(writer, ts.a)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
        }

        for ts in &track.transforms {
            write_string_by_int32(writer, ts.i.as_deref().unwrap_or(""))?;
            write_string_by_int32(writer, ts.font.as_deref().unwrap_or(""))?;
            write_string_by_int32(writer, ts.text.as_deref().unwrap_or(""))?;
        }
    }

    Ok(())
}

fn encode_phone32<W: Write>(writer: &mut W, reanim: &Reanim) -> Result<(), ReanimError> {
    writer.write_i32::<LE>(-14326347)?;
    writer.write_i32::<LE>(0)?;
    let tracks_number = encode_len("track", reanim.tracks.len())?;
    writer.write_i32::<LE>(tracks_number)?;
    writer.write_f32::<LE>(reanim.fps)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0x10)?;

    for track in &reanim.tracks {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(encode_len("transform", track.transforms.len())?)?;
    }

    for track in &reanim.tracks {
        write_string_by_int32(writer, &track.name)?;
        writer.write_i32::<LE>(0x2C)?;

        for ts in &track.transforms {
            write_opt_f32(writer, ts.x)?;
            write_opt_f32(writer, ts.y)?;
            write_opt_f32(writer, ts.kx)?;
            write_opt_f32(writer, ts.ky)?;
            write_opt_f32(writer, ts.sx)?;
            write_opt_f32(writer, ts.sy)?;
            write_opt_f32(writer, ts.f)?;
            write_opt_f32(writer, ts.a)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
        }

        for ts in &track.transforms {
            let i_val: i32 = ts.i.as_ref().and_then(|s| s.parse().ok()).unwrap_or(-1);
            writer.write_i32::<LE>(i_val)?;
            write_string_by_int32(writer, ts.font.as_deref().unwrap_or(""))?;
            write_string_by_int32(writer, ts.text.as_deref().unwrap_or(""))?;
        }
    }

    Ok(())
}

fn encode_phone64<W: Write>(writer: &mut W, reanim: &Reanim) -> Result<(), ReanimError> {
    writer.write_i32::<LE>(-1069095568)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0)?;
    let tracks_number = encode_len("track", reanim.tracks.len())?;
    writer.write_i32::<LE>(tracks_number)?;
    writer.write_f32::<LE>(reanim.fps)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0x20)?;

    for track in &reanim.tracks {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(encode_len("transform", track.transforms.len())?)?;
        writer.write_i32::<LE>(0)?;
    }

    for track in &reanim.tracks {
        write_string_by_int32(writer, &track.name)?;
        writer.write_i32::<LE>(0x38)?;

        for ts in &track.transforms {
            write_opt_f32(writer, ts.x)?;
            write_opt_f32(writer, ts.y)?;
            write_opt_f32(writer, ts.kx)?;
            write_opt_f32(writer, ts.ky)?;
            write_opt_f32(writer, ts.sx)?;
            write_opt_f32(writer, ts.sy)?;
            write_opt_f32(writer, ts.f)?;
            write_opt_f32(writer, ts.a)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
            writer.write_i32::<LE>(0)?;
        }

        for ts in &track.transforms {
            let i_val: i32 = ts.i.as_ref().and_then(|s| s.parse().ok()).unwrap_or(-1);
            writer.write_i32::<LE>(i_val)?;
            write_string_by_int32(writer, ts.font.as_deref().unwrap_or(""))?;
            write_string_by_int32(writer, ts.text.as_deref().unwrap_or(""))?;
        }
    }

    Ok(())
}
