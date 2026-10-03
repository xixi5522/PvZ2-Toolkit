use crate::error::{Error, Result};
use crate::types::*;
use byteorder::{LE, WriteBytesExt};
use std::io::Write;

pub fn encode_pam<W: Write>(pam: &PamInfo, writer: &mut W) -> Result<()> {
    if !(1..=6).contains(&pam.version) {
        return Err(Error::VersionOutOfRange(pam.version));
    }

    writer.write_u32::<LE>(PAM_MAGIC)?;
    writer.write_u32::<LE>(pam.version as u32)?;
    writer.write_u8(pam.frame_rate as u8)?;

    for p in &pam.position {
        writer.write_i16::<LE>(round_i16(*p * 20.0))?;
    }

    for s in &pam.size {
        writer.write_u16::<LE>(round_u16(*s * 20.0))?;
    }

    write_u16_len(pam.image.len(), "pam.image", writer)?;
    for img in &pam.image {
        write_image_info(img, writer, pam.version)?;
    }

    write_u16_len(pam.sprite.len(), "pam.sprite", writer)?;
    for sprite in &pam.sprite {
        write_sprite_info(sprite, writer, pam.version)?;
    }

    if pam.version > 3 {
        let has_main = pam.main_sprite.is_some();
        writer.write_u8(if has_main { 1 } else { 0 })?;
        if let Some(main_sprite) = &pam.main_sprite {
            write_sprite_info(main_sprite, writer, pam.version)?;
        }
    } else {
        let main_sprite = pam
            .main_sprite
            .as_ref()
            .ok_or(Error::MissingRequiredField {
                field: "main_sprite",
            })?;
        write_sprite_info(main_sprite, writer, pam.version)?;
    }

    Ok(())
}

fn round_i16(value: f64) -> i16 {
    value.round() as i16
}

fn round_u16(value: f64) -> u16 {
    value.round() as u16
}

fn round_i32(value: f64) -> i32 {
    value.round() as i32
}

fn write_u8_count<W: Write>(count: usize, field: &'static str, writer: &mut W) -> Result<()> {
    if count > u8::MAX as usize {
        return Err(Error::ValueOutOfRange {
            field,
            value: count as i64,
        });
    }
    writer.write_u8(count as u8)?;
    Ok(())
}

fn write_u16_len<W: Write>(count: usize, field: &'static str, writer: &mut W) -> Result<()> {
    if count > u16::MAX as usize {
        return Err(Error::ValueOutOfRange {
            field,
            value: count as i64,
        });
    }
    writer.write_u16::<LE>(count as u16)?;
    Ok(())
}

fn write_variant_count<W: Write>(count: usize, field: &'static str, writer: &mut W) -> Result<()> {
    if count < 255 {
        writer.write_u8(count as u8)?;
    } else {
        writer.write_u8(255)?;
        write_u16_len(count, field, writer)?;
    }
    Ok(())
}

fn write_string_by_u16<W: Write>(s: &str, writer: &mut W) -> Result<()> {
    let bytes = s.as_bytes();
    if bytes.len() > u16::MAX as usize {
        return Err(Error::ValueOutOfRange {
            field: "string.length",
            value: bytes.len() as i64,
        });
    }
    writer.write_u16::<LE>(bytes.len() as u16)?;
    writer.write_all(bytes)?;
    Ok(())
}

fn write_image_info<W: Write>(img: &ImageInfo, writer: &mut W, version: i32) -> Result<()> {
    write_string_by_u16(&img.name, writer)?;

    if version >= 4 {
        let size = img.size.ok_or(Error::MissingRequiredField {
            field: "image.size",
        })?;
        writer.write_i16::<LE>(size[0] as i16)?;
        writer.write_i16::<LE>(size[1] as i16)?;
    }

    if version == 1 {
        if img.transform.len() != 3 {
            return Err(Error::InvalidTransform {
                field: "image.transform",
                expected: "3 values for PAM v1",
                actual: img.transform.len(),
            });
        }
        let (angle, tx, ty) = (img.transform[0], img.transform[1], img.transform[2]);
        writer.write_i16::<LE>(round_i16(angle * 1000.0))?;
        writer.write_i16::<LE>(round_i16(tx * 20.0))?;
        writer.write_i16::<LE>(round_i16(ty * 20.0))?;
    } else {
        let t = &img.transform;
        if t.len() != 6 {
            return Err(Error::InvalidTransform {
                field: "image.transform",
                expected: "6 values for PAM v2+",
                actual: t.len(),
            });
        }
        let (a, b, c, d, tx, ty) = (t[0], t[1], t[2], t[3], t[4], t[5]);
        let matrix_rate = 1310720.0;

        writer.write_i32::<LE>(round_i32(a * matrix_rate))?;
        writer.write_i32::<LE>(round_i32(c * matrix_rate))?;
        writer.write_i32::<LE>(round_i32(b * matrix_rate))?;
        writer.write_i32::<LE>(round_i32(d * matrix_rate))?;
        writer.write_i16::<LE>(round_i16(tx * 20.0))?;
        writer.write_i16::<LE>(round_i16(ty * 20.0))?;
    }

    Ok(())
}

fn write_sprite_info<W: Write>(sprite: &SpriteInfo, writer: &mut W, version: i32) -> Result<()> {
    if version >= 4 {
        let name = sprite.name.as_deref().ok_or(Error::MissingRequiredField {
            field: "sprite.name",
        })?;
        write_string_by_u16(name, writer)?;
        if version >= 6 {
            write_string_by_u16("", writer)?;
        }
        let frame_rate = sprite.frame_rate.ok_or(Error::MissingRequiredField {
            field: "sprite.frame_rate",
        })?;
        writer.write_i32::<LE>(round_i32(frame_rate * 65536.0))?;
    }

    write_u16_len(sprite.frame.len(), "sprite.frame", writer)?;

    if version >= 5 {
        let work_area = sprite.work_area.ok_or(Error::MissingRequiredField {
            field: "sprite.work_area",
        })?;
        writer.write_i16::<LE>(work_area[0] as i16)?;
        writer.write_i16::<LE>(work_area[1] as i16)?;
    }

    for frame in &sprite.frame {
        write_frame_info(frame, writer, version)?;
    }

    Ok(())
}

fn write_frame_info<W: Write>(frame: &FrameInfo, writer: &mut W, version: i32) -> Result<()> {
    let mut flags = FrameFlags::empty();
    if !frame.remove.is_empty() {
        flags |= FrameFlags::REMOVES;
    }
    if !frame.append.is_empty() {
        flags |= FrameFlags::ADDS;
    }
    if !frame.change.is_empty() {
        flags |= FrameFlags::MOVES;
    }
    if frame.label.is_some() {
        flags |= FrameFlags::FRAME_NAME;
    }
    if frame.stop {
        flags |= FrameFlags::STOP;
    }
    if !frame.command.is_empty() {
        flags |= FrameFlags::COMMANDS;
    }

    writer.write_u8(flags.bits())?;

    if flags.contains(FrameFlags::REMOVES) {
        let count = frame.remove.len();
        write_variant_count(count, "frame.remove", writer)?;
        for rem in &frame.remove {
            write_removes_info(rem, writer)?;
        }
    }

    if flags.contains(FrameFlags::ADDS) {
        let count = frame.append.len();
        write_variant_count(count, "frame.append", writer)?;
        for add in &frame.append {
            write_adds_info(add, writer, version)?;
        }
    }

    if flags.contains(FrameFlags::MOVES) {
        let count = frame.change.len();
        write_variant_count(count, "frame.change", writer)?;
        for change in &frame.change {
            write_moves_info(change, writer, version)?;
        }
    }

    if let Some(label) = &frame.label {
        write_string_by_u16(label, writer)?;
    }

    if flags.contains(FrameFlags::COMMANDS) {
        let count = frame.command.len();
        write_u8_count(count, "frame.command", writer)?;
        for cmd in &frame.command {
            write_string_by_u16(&cmd[0], writer)?;
            write_string_by_u16(&cmd[1], writer)?;
        }
    }

    Ok(())
}

fn write_removes_info<W: Write>(info: &RemovesInfo, writer: &mut W) -> Result<()> {
    if info.index >= 2047 {
        writer.write_u16::<LE>(2047)?;
        writer.write_i32::<LE>(info.index)?;
    } else {
        writer.write_u16::<LE>(info.index as u16)?;
    }
    Ok(())
}

fn write_adds_info<W: Write>(info: &AddsInfo, writer: &mut W, version: i32) -> Result<()> {
    // num encoding
    // num & 2047 = index
    // num & 32768 = sprite (bool)
    // num & 16384 = additive (bool)
    // num & 8192 = has preload_frame
    // num & 4096 = has name
    // num & 2048 = has time_scale

    let mut num = 0u16;
    let large_index = info.index >= 2047;

    if large_index {
        num |= 2047;
    } else {
        num |= info.index as u16;
    }

    if info.sprite {
        num |= 32768;
    }
    if info.additive {
        num |= 16384;
    }
    if info.preload_frame != 0 {
        num |= 8192;
    }

    if info.name.is_some() {
        num |= 4096;
    }
    if (info.time_scale - 1.0).abs() > 0.0001 {
        num |= 2048;
    }

    writer.write_u16::<LE>(num)?;

    if large_index {
        writer.write_i32::<LE>(info.index)?;
    }

    // Resource
    if version >= 6 && info.resource >= 255 {
        if info.resource > u16::MAX as u32 {
            return Err(Error::ValueOutOfRange {
                field: "append.resource",
                value: info.resource as i64,
            });
        }
        writer.write_u8(255)?;
        writer.write_u16::<LE>(info.resource as u16)?;
    } else {
        if info.resource > u8::MAX as u32 {
            return Err(Error::ValueOutOfRange {
                field: "append.resource",
                value: info.resource as i64,
            });
        }
        writer.write_u8((info.resource & 0xFF) as u8)?;
    }

    if (num & 8192) != 0 {
        writer.write_i16::<LE>(info.preload_frame as i16)?;
    }

    if let Some(name) = &info.name {
        write_string_by_u16(name, writer)?;
    }

    if (num & 2048) != 0 {
        writer.write_i32::<LE>(round_i32(info.time_scale as f64 * 65536.0))?;
    }

    Ok(())
}

fn write_moves_info<W: Write>(info: &MovesInfo, writer: &mut W, _version: i32) -> Result<()> {
    // num7 encoding
    // num & 1023 = index
    // Flags

    let mut num = 0u16;
    let large_index = info.index >= 1023;
    if large_index {
        num |= 1023;
    } else {
        num |= info.index as u16;
    }

    let mut flags = MoveFlags::empty();

    if info.transform.len() >= 6 {
        flags |= MoveFlags::MATRIX;
    } else if info.transform.len() >= 3 && info.transform[0].abs() > f64::EPSILON {
        // Use f64::EPSILON instead of a hardcoded 0.00001 to avoid silently
        // dropping near-zero rotations.
        flags |= MoveFlags::ROTATE;
    }

    if info.source_rectangle.is_some() {
        flags |= MoveFlags::SRC_RECT;
    }
    if info.color.is_some() {
        flags |= MoveFlags::COLOR;
    }
    if info.sprite_frame_number.is_some() {
        flags |= MoveFlags::ANIM_FRAME_NUM;
    }
    flags |= MoveFlags::LONG_COORDS;

    let len = info.transform.len();
    let tx_raw = if len >= 2 {
        info.transform[len - 2]
    } else {
        0.0
    };
    let ty_raw = if len >= 2 {
        info.transform[len - 1]
    } else {
        0.0
    };

    num |= flags.bits();

    writer.write_u16::<LE>(num)?;

    if large_index {
        writer.write_i32::<LE>(info.index)?;
    }

    if flags.contains(MoveFlags::MATRIX) {
        writer.write_i32::<LE>(round_i32(info.transform[0] * 65536.0))?;
        writer.write_i32::<LE>(round_i32(info.transform[2] * 65536.0))?;
        writer.write_i32::<LE>(round_i32(info.transform[1] * 65536.0))?;
        writer.write_i32::<LE>(round_i32(info.transform[3] * 65536.0))?;
    } else if flags.contains(MoveFlags::ROTATE) {
        writer.write_i16::<LE>(round_i16(info.transform[0] * 1000.0))?;
    }

    writer.write_i32::<LE>(round_i32(tx_raw * 20.0))?;
    writer.write_i32::<LE>(round_i32(ty_raw * 20.0))?;

    if flags.contains(MoveFlags::SRC_RECT)
        && let Some(sr) = &info.source_rectangle
    {
        for v in [sr.position[0], sr.position[1], sr.size[0], sr.size[1]] {
            writer.write_i16::<LE>(round_i16(v * 20.0))?;
        }
    }

    if flags.contains(MoveFlags::COLOR)
        && let Some(c) = &info.color
    {
        for v in c {
            writer.write_u8(round_u16(*v * 255.0) as u8)?;
        }
    }

    if let Some(sprite_frame_number) = info.sprite_frame_number {
        writer.write_i16::<LE>(sprite_frame_number as i16)?;
    }

    Ok(())
}
