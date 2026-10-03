use crate::error::Error;
use crate::error::Result;
use crate::types::*;
use byteorder::{LE, ReadBytesExt};
use std::io::Read;

pub fn decode_pam<R: Read>(reader: &mut R) -> Result<PamInfo> {
    let magic = reader.read_u32::<LE>()?;
    if magic != PAM_MAGIC {
        return Err(Error::InvalidMagic(magic));
    }

    let version = reader.read_u32::<LE>()? as i32;
    if !(1..=6).contains(&version) {
        return Err(Error::VersionOutOfRange(version));
    }

    let frame_rate = reader.read_u8()? as i32;

    let mut position = [0.0; 2];
    for p in &mut position {
        *p = reader.read_i16::<LE>()? as f64 / 20.0;
    }

    let mut size = [0.0; 2];
    for s in &mut size {
        *s = reader.read_u16::<LE>()? as f64 / 20.0;
    }

    let images_count = reader.read_u16::<LE>()? as usize;
    let mut image = Vec::with_capacity(images_count);
    for _ in 0..images_count {
        image.push(read_image_info(reader, version)?);
    }

    let sprites_count = reader.read_u16::<LE>()? as usize;
    let mut sprite = Vec::with_capacity(sprites_count);
    for _ in 0..sprites_count {
        sprite.push(read_sprite_info(reader, version)?);
    }

    let has_main_sprite = if version <= 3 {
        true
    } else {
        read_bool(reader)?
    };

    let main_sprite = if has_main_sprite {
        Some(read_sprite_info(reader, version)?)
    } else {
        None
    };

    Ok(PamInfo {
        version,
        frame_rate,
        position,
        size,
        image,
        sprite,
        main_sprite,
    })
}

fn read_string_by_u16<R: Read>(reader: &mut R) -> Result<String> {
    let len = reader.read_u16::<LE>()? as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn read_bool<R: Read>(reader: &mut R) -> Result<bool> {
    Ok(reader.read_u8()? != 0)
}

fn read_image_info<R: Read>(reader: &mut R, version: i32) -> Result<ImageInfo> {
    let name = read_string_by_u16(reader)?;
    let size = if version >= 4 {
        Some([
            reader.read_i16::<LE>()? as i32,
            reader.read_i16::<LE>()? as i32,
        ])
    } else {
        None
    };

    let transform = if version == 1 {
        let mut t = vec![0.0; 3];
        t[0] = reader.read_i16::<LE>()? as f64 / 1000.0;
        t[1] = reader.read_i16::<LE>()? as f64 / 20.0;
        t[2] = reader.read_i16::<LE>()? as f64 / 20.0;
        t
    } else {
        let mut t = vec![0.0; 6];
        t[0] = reader.read_i32::<LE>()? as f64 / 1310720.0;
        t[2] = reader.read_i32::<LE>()? as f64 / 1310720.0;
        t[1] = reader.read_i32::<LE>()? as f64 / 1310720.0;
        t[3] = reader.read_i32::<LE>()? as f64 / 1310720.0;
        t[4] = reader.read_i16::<LE>()? as f64 / 20.0;
        t[5] = reader.read_i16::<LE>()? as f64 / 20.0;
        t
    };
    Ok(ImageInfo {
        name,
        size,
        transform,
    })
}

fn read_sprite_info<R: Read>(reader: &mut R, version: i32) -> Result<SpriteInfo> {
    let mut name = None;
    let mut frame_rate = None;

    if version >= 4 {
        name = Some(read_string_by_u16(reader)?);
        if version >= 6 {
            let _reserved = read_string_by_u16(reader)?;
        }
        frame_rate = Some(reader.read_i32::<LE>()? as f64 / 65536.0);
    }

    let frames_count = reader.read_u16::<LE>()? as usize;
    let work_area = if version >= 5 {
        Some([
            reader.read_i16::<LE>()? as i32,
            reader.read_i16::<LE>()? as i32,
        ])
    } else {
        None
    };

    let mut frame = Vec::with_capacity(frames_count);
    for _ in 0..frames_count {
        frame.push(read_frame_info(reader, version)?);
    }

    Ok(SpriteInfo {
        name,
        frame_rate,
        work_area,
        frame,
    })
}

fn read_frame_info<R: Read>(reader: &mut R, version: i32) -> Result<FrameInfo> {
    let flags_byte = reader.read_u8()?;
    let flags = FrameFlags::from_bits_truncate(flags_byte);

    let mut remove = Vec::new();
    if flags.contains(FrameFlags::REMOVES) {
        let mut count = reader.read_u8()? as usize;
        if count == 255 {
            count = reader.read_u16::<LE>()? as usize;
        }
        for _ in 0..count {
            remove.push(read_removes_info(reader)?);
        }
    }

    let mut append = Vec::new();
    if flags.contains(FrameFlags::ADDS) {
        let mut count = reader.read_u8()? as usize;
        if count == 255 {
            count = reader.read_u16::<LE>()? as usize;
        }
        for _ in 0..count {
            append.push(read_adds_info(reader, version)?);
        }
    }

    let mut change = Vec::new();
    if flags.contains(FrameFlags::MOVES) {
        let mut count = reader.read_u8()? as usize;
        if count == 255 {
            count = reader.read_u16::<LE>()? as usize;
        }
        for _ in 0..count {
            change.push(read_moves_info(reader, version)?);
        }
    }

    let mut label = None;
    if flags.contains(FrameFlags::FRAME_NAME) {
        label = Some(read_string_by_u16(reader)?);
    }

    let stop = flags.contains(FrameFlags::STOP);

    let mut command = Vec::new();
    if flags.contains(FrameFlags::COMMANDS) {
        let count = reader.read_u8()? as usize;
        for _ in 0..count {
            let s1 = read_string_by_u16(reader)?;
            let s2 = read_string_by_u16(reader)?;
            command.push([s1, s2]);
        }
    }

    Ok(FrameInfo {
        label,
        stop,
        command,
        remove,
        append,
        change,
    })
}

fn read_removes_info<R: Read>(reader: &mut R) -> Result<RemovesInfo> {
    let mut index = (reader.read_u16::<LE>()? & 2047) as i32;
    if index == 2047 {
        index = reader.read_i32::<LE>()?;
    }
    Ok(RemovesInfo { index })
}

fn read_adds_info<R: Read>(reader: &mut R, version: i32) -> Result<AddsInfo> {
    let num = reader.read_u16::<LE>()?;
    let mut index = (num & 2047) as i32;
    if index == 2047 {
        index = reader.read_i32::<LE>()?;
    }

    let sprite = (num & 32768) != 0;
    let additive = (num & 16384) != 0;

    let mut resource = reader.read_u8()? as u32;
    if version >= 6 && resource == 255 {
        resource = reader.read_u16::<LE>()? as u32;
    }

    let preload_frame = if (num & 8192) != 0 {
        reader.read_i16::<LE>()? as i32
    } else {
        0
    };

    let name = if (num & 4096) != 0 {
        Some(read_string_by_u16(reader)?)
    } else {
        None
    };

    let time_scale = if (num & 2048) != 0 {
        reader.read_i32::<LE>()? as f32 / 65536.0
    } else {
        1.0
    };

    Ok(AddsInfo {
        index,
        name,
        resource,
        sprite,
        additive,
        preload_frame,
        time_scale,
    })
}

fn read_moves_info<R: Read>(reader: &mut R, _version: i32) -> Result<MovesInfo> {
    let num7 = reader.read_u16::<LE>()?;
    let mut index = (num7 & 1023) as i32;
    if index == 1023 {
        index = reader.read_i32::<LE>()?;
    }

    let flags = MoveFlags::from_bits_truncate(num7);
    let mut transform = Vec::new();

    if flags.contains(MoveFlags::MATRIX) {
        transform.resize(6, 0.0);
        transform[0] = reader.read_i32::<LE>()? as f64 / 65536.0;
        transform[2] = reader.read_i32::<LE>()? as f64 / 65536.0;
        transform[1] = reader.read_i32::<LE>()? as f64 / 65536.0;
        transform[3] = reader.read_i32::<LE>()? as f64 / 65536.0;
    } else if flags.contains(MoveFlags::ROTATE) {
        transform.resize(3, 0.0);
        transform[0] = reader.read_i16::<LE>()? as f64 / 1000.0;
    } else {
        transform.resize(2, 0.0);
    }

    let (val1, val2) = if flags.contains(MoveFlags::LONG_COORDS) {
        (
            reader.read_i32::<LE>()? as f64 / 20.0,
            reader.read_i32::<LE>()? as f64 / 20.0,
        )
    } else {
        (
            reader.read_i16::<LE>()? as f64 / 20.0,
            reader.read_i16::<LE>()? as f64 / 20.0,
        )
    };

    let len = transform.len();
    transform[len - 2] = val1;
    transform[len - 1] = val2;

    let mut source_rectangle = None;
    if flags.contains(MoveFlags::SRC_RECT) {
        source_rectangle = Some(Rectangle {
            position: [
                reader.read_i16::<LE>()? as f64 / 20.0,
                reader.read_i16::<LE>()? as f64 / 20.0,
            ],
            size: [
                reader.read_i16::<LE>()? as f64 / 20.0,
                reader.read_i16::<LE>()? as f64 / 20.0,
            ],
        });
    }

    let mut color = None;
    if flags.contains(MoveFlags::COLOR) {
        let mut c = [0.0; 4];
        for v in &mut c {
            *v = reader.read_u8()? as f64 / 255.0;
        }
        color = Some(c);
    }

    let sprite_frame_number = if flags.contains(MoveFlags::ANIM_FRAME_NUM) {
        Some(reader.read_i16::<LE>()? as i32)
    } else {
        None
    };

    Ok(MovesInfo {
        index,
        transform,
        color,
        source_rectangle,
        sprite_frame_number,
    })
}
