use crate::particles::codec::{
    POPCAP_COMPILED_MAGIC, checked_collection_len, popcap_decompress, read_string, read_track_nodes,
};
use crate::particles::error::ParticlesError;
use crate::particles::types::{Particles, ParticlesEmitter, ParticlesField};
use byteorder::{LE, ReadBytesExt};
use std::io::{Cursor, Read};

/// Alias for backward compatibility
fn read_string_by_int32<R: Read>(reader: &mut R) -> Result<String, ParticlesError> {
    read_string(reader)
}

fn payload_reader(data: &[u8]) -> Result<Cursor<Vec<u8>>, ParticlesError> {
    if data.len() < 4 {
        return Err(ParticlesError::InvalidVariant);
    }
    let magic = u32::from_le_bytes(data[..4].try_into().expect("four bytes checked above"));
    let payload = if magic == POPCAP_COMPILED_MAGIC {
        popcap_decompress(data)?
    } else {
        data.to_vec()
    };
    Ok(Cursor::new(payload))
}

fn expect_i32<R: Read>(reader: &mut R, expected: i32) -> Result<(), ParticlesError> {
    let actual = reader.read_i32::<LE>()?;
    if actual != expected {
        return Err(ParticlesError::UnsupportedFormat(
            expected as u32,
            actual as u32,
        ));
    }
    Ok(())
}

pub fn decode_pc(data: &[u8]) -> Result<Particles, ParticlesError> {
    let mut reader = payload_reader(data)?;

    let mut unused_8 = [0u8; 8];
    reader.read_exact(&mut unused_8)?;

    let count = checked_collection_len("emitter", reader.read_i32::<LE>()?)?;
    let mut particles = Particles {
        emitters: vec![ParticlesEmitter::default(); count],
    };

    expect_i32(&mut reader, 0x164)?;

    for i in 0..count {
        let emitter = &mut particles.emitters[i];
        let mut unused_4 = [0u8; 4];
        reader.read_exact(&mut unused_4)?;

        // ImageCol, ImageRow, ImageFrames, Animated
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_col = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_row = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.image_frames = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.animated = Some(val);
        }

        emitter.particle_flags = reader.read_i32::<LE>()?;
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.emitter_type = Some(val);
        }

        let mut unused_188 = [0u8; 188];
        reader.read_exact(&mut unused_188)?;

        let field_count = checked_collection_len("field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.field = Some(vec![ParticlesField::default(); field_count]);
        }
        reader.read_exact(&mut unused_4)?;

        let field_count = checked_collection_len("system field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.system_field = Some(vec![ParticlesField::default(); field_count]);
        }
        let mut unused_128 = [0u8; 128];
        reader.read_exact(&mut unused_128)?;
    }

    for i in 0..count {
        let emitter = &mut particles.emitters[i];

        let img = read_string_by_int32(&mut reader)?;
        if !img.is_empty() {
            emitter.image = Some(img);
        }

        let name = read_string_by_int32(&mut reader)?;
        if !name.is_empty() {
            emitter.name = Some(name);
        }

        emitter.system_duration = read_track_nodes(&mut reader)?;
        let on_dur = read_string_by_int32(&mut reader)?;
        if !on_dur.is_empty() {
            emitter.on_duration = Some(on_dur);
        }

        emitter.cross_fade_duration = read_track_nodes(&mut reader)?;
        emitter.spawn_rate = read_track_nodes(&mut reader)?;
        emitter.spawn_min_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_launched = read_track_nodes(&mut reader)?;
        emitter.emitter_radius = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_x = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_y = read_track_nodes(&mut reader)?;
        emitter.emitter_box_x = read_track_nodes(&mut reader)?;
        emitter.emitter_box_y = read_track_nodes(&mut reader)?;
        emitter.emitter_path = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_x = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_y = read_track_nodes(&mut reader)?;
        emitter.particle_duration = read_track_nodes(&mut reader)?;
        emitter.system_red = read_track_nodes(&mut reader)?;
        emitter.system_green = read_track_nodes(&mut reader)?;
        emitter.system_blue = read_track_nodes(&mut reader)?;
        emitter.system_alpha = read_track_nodes(&mut reader)?;
        emitter.system_brightness = read_track_nodes(&mut reader)?;
        emitter.launch_speed = read_track_nodes(&mut reader)?;
        emitter.launch_angle = read_track_nodes(&mut reader)?;

        // Read fields
        expect_i32(&mut reader, 0x14)?;
        if let Some(fields) = &mut emitter.field {
            for field in fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_16 = [0u8; 16];
                reader.read_exact(&mut unused_16)?;
            }
            for field in fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        expect_i32(&mut reader, 0x14)?;
        if let Some(sys_fields) = &mut emitter.system_field {
            for field in sys_fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_16 = [0u8; 16];
                reader.read_exact(&mut unused_16)?;
            }
            for field in sys_fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        emitter.particle_red = read_track_nodes(&mut reader)?;
        emitter.particle_green = read_track_nodes(&mut reader)?;
        emitter.particle_blue = read_track_nodes(&mut reader)?;
        emitter.particle_alpha = read_track_nodes(&mut reader)?;
        emitter.particle_brightness = read_track_nodes(&mut reader)?;
        emitter.particle_spin_angle = read_track_nodes(&mut reader)?;
        emitter.particle_spin_speed = read_track_nodes(&mut reader)?;
        emitter.particle_scale = read_track_nodes(&mut reader)?;
        emitter.particle_stretch = read_track_nodes(&mut reader)?;
        emitter.collision_reflect = read_track_nodes(&mut reader)?;
        emitter.collision_spin = read_track_nodes(&mut reader)?;
        emitter.clip_top = read_track_nodes(&mut reader)?;
        emitter.clip_bottom = read_track_nodes(&mut reader)?;
        emitter.clip_left = read_track_nodes(&mut reader)?;
        emitter.clip_right = read_track_nodes(&mut reader)?;
        emitter.animation_rate = read_track_nodes(&mut reader)?;
    }

    Ok(particles)
}

pub fn decode_phone32(data: &[u8]) -> Result<Particles, ParticlesError> {
    let mut reader = payload_reader(data)?;

    let mut unused_8 = [0u8; 8];
    reader.read_exact(&mut unused_8)?;

    let count = checked_collection_len("emitter", reader.read_i32::<LE>()?)?;
    let mut particles = Particles {
        emitters: vec![ParticlesEmitter::default(); count],
    };

    expect_i32(&mut reader, 0x164)?;

    for i in 0..count {
        let emitter = &mut particles.emitters[i];
        let mut unused_4 = [0u8; 4];
        reader.read_exact(&mut unused_4)?;

        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_col = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_row = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.image_frames = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.animated = Some(val);
        }

        emitter.particle_flags = reader.read_i32::<LE>()?;
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.emitter_type = Some(val);
        }

        let mut unused_188 = [0u8; 188];
        reader.read_exact(&mut unused_188)?;

        let field_count = checked_collection_len("field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.field = Some(vec![ParticlesField::default(); field_count]);
        }
        reader.read_exact(&mut unused_4)?;
        let field_count = checked_collection_len("system field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.system_field = Some(vec![ParticlesField::default(); field_count]);
        }
        let mut unused_128 = [0u8; 128];
        reader.read_exact(&mut unused_128)?;
    }

    for i in 0..count {
        let emitter = &mut particles.emitters[i];

        let img_int = reader.read_i32::<LE>()?;
        if img_int != -1 {
            emitter.image = Some(img_int.to_string());
        }

        let name = read_string_by_int32(&mut reader)?;
        if !name.is_empty() {
            emitter.name = Some(name);
        }

        emitter.system_duration = read_track_nodes(&mut reader)?;
        let on_dur = read_string_by_int32(&mut reader)?;
        if !on_dur.is_empty() {
            emitter.on_duration = Some(on_dur);
        }

        emitter.cross_fade_duration = read_track_nodes(&mut reader)?;
        emitter.spawn_rate = read_track_nodes(&mut reader)?;
        emitter.spawn_min_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_launched = read_track_nodes(&mut reader)?;
        emitter.emitter_radius = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_x = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_y = read_track_nodes(&mut reader)?;
        emitter.emitter_box_x = read_track_nodes(&mut reader)?;
        emitter.emitter_box_y = read_track_nodes(&mut reader)?;
        emitter.emitter_path = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_x = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_y = read_track_nodes(&mut reader)?;
        emitter.particle_duration = read_track_nodes(&mut reader)?;
        emitter.system_red = read_track_nodes(&mut reader)?;
        emitter.system_green = read_track_nodes(&mut reader)?;
        emitter.system_blue = read_track_nodes(&mut reader)?;
        emitter.system_alpha = read_track_nodes(&mut reader)?;
        emitter.system_brightness = read_track_nodes(&mut reader)?;
        emitter.launch_speed = read_track_nodes(&mut reader)?;
        emitter.launch_angle = read_track_nodes(&mut reader)?;

        // Fields
        expect_i32(&mut reader, 0x14)?;
        if let Some(fields) = &mut emitter.field {
            for field in fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_16 = [0u8; 16];
                reader.read_exact(&mut unused_16)?;
            }
            for field in fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        expect_i32(&mut reader, 0x14)?;
        if let Some(sys_fields) = &mut emitter.system_field {
            for field in sys_fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_16 = [0u8; 16];
                reader.read_exact(&mut unused_16)?;
            }
            for field in sys_fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        emitter.particle_red = read_track_nodes(&mut reader)?;
        emitter.particle_green = read_track_nodes(&mut reader)?;
        emitter.particle_blue = read_track_nodes(&mut reader)?;
        emitter.particle_alpha = read_track_nodes(&mut reader)?;
        emitter.particle_brightness = read_track_nodes(&mut reader)?;
        emitter.particle_spin_angle = read_track_nodes(&mut reader)?;
        emitter.particle_spin_speed = read_track_nodes(&mut reader)?;
        emitter.particle_scale = read_track_nodes(&mut reader)?;
        emitter.particle_stretch = read_track_nodes(&mut reader)?;
        emitter.collision_reflect = read_track_nodes(&mut reader)?;
        emitter.collision_spin = read_track_nodes(&mut reader)?;
        emitter.clip_top = read_track_nodes(&mut reader)?;
        emitter.clip_bottom = read_track_nodes(&mut reader)?;
        emitter.clip_left = read_track_nodes(&mut reader)?;
        emitter.clip_right = read_track_nodes(&mut reader)?;
        emitter.animation_rate = read_track_nodes(&mut reader)?;
    }

    Ok(particles)
}

pub fn decode_phone64(data: &[u8]) -> Result<Particles, ParticlesError> {
    let mut reader = payload_reader(data)?;

    let mut unused_12 = [0u8; 12];
    reader.read_exact(&mut unused_12)?;

    let count = checked_collection_len("emitter", reader.read_i32::<LE>()?)?;
    let mut particles = Particles {
        emitters: vec![ParticlesEmitter::default(); count],
    };

    let mut unused_4 = [0u8; 4];
    reader.read_exact(&mut unused_4)?;

    expect_i32(&mut reader, 0x2B0)?;

    for i in 0..count {
        let emitter = &mut particles.emitters[i];
        let mut unused_8 = [0u8; 8];
        reader.read_exact(&mut unused_8)?;

        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_col = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.image_row = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.image_frames = Some(val);
        }
        let val = reader.read_i32::<LE>()?;
        if val != 0 {
            emitter.animated = Some(val);
        }

        emitter.particle_flags = reader.read_i32::<LE>()?;
        let val = reader.read_i32::<LE>()?;
        if val != 1 {
            emitter.emitter_type = Some(val);
        }

        let mut unused_376 = [0u8; 376];
        reader.read_exact(&mut unused_376)?;

        let field_count = checked_collection_len("field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.field = Some(vec![ParticlesField::default(); field_count]);
        }
        let mut unused_12 = [0u8; 12];
        reader.read_exact(&mut unused_12)?;
        let field_count = checked_collection_len("system field", reader.read_i32::<LE>()?)?;
        if field_count != 0 {
            emitter.system_field = Some(vec![ParticlesField::default(); field_count]);
        }
        let mut unused_260 = [0u8; 260];
        reader.read_exact(&mut unused_260)?;
    }

    for i in 0..count {
        let emitter = &mut particles.emitters[i];

        let img_int = reader.read_i32::<LE>()?;
        if img_int != -1 {
            emitter.image = Some(img_int.to_string());
        }

        let name = read_string_by_int32(&mut reader)?;
        if !name.is_empty() {
            emitter.name = Some(name);
        }

        emitter.system_duration = read_track_nodes(&mut reader)?;
        let on_dur = read_string_by_int32(&mut reader)?;
        if !on_dur.is_empty() {
            emitter.on_duration = Some(on_dur);
        }

        emitter.cross_fade_duration = read_track_nodes(&mut reader)?;
        emitter.spawn_rate = read_track_nodes(&mut reader)?;
        emitter.spawn_min_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_active = read_track_nodes(&mut reader)?;
        emitter.spawn_max_launched = read_track_nodes(&mut reader)?;
        emitter.emitter_radius = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_x = read_track_nodes(&mut reader)?;
        emitter.emitter_offset_y = read_track_nodes(&mut reader)?;
        emitter.emitter_box_x = read_track_nodes(&mut reader)?;
        emitter.emitter_box_y = read_track_nodes(&mut reader)?;
        emitter.emitter_path = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_x = read_track_nodes(&mut reader)?;
        emitter.emitter_skew_y = read_track_nodes(&mut reader)?;
        emitter.particle_duration = read_track_nodes(&mut reader)?;
        emitter.system_red = read_track_nodes(&mut reader)?;
        emitter.system_green = read_track_nodes(&mut reader)?;
        emitter.system_blue = read_track_nodes(&mut reader)?;
        emitter.system_alpha = read_track_nodes(&mut reader)?;
        emitter.system_brightness = read_track_nodes(&mut reader)?;
        emitter.launch_speed = read_track_nodes(&mut reader)?;
        emitter.launch_angle = read_track_nodes(&mut reader)?;

        // Fields
        expect_i32(&mut reader, 0x18)?;
        if let Some(fields) = &mut emitter.field {
            for field in fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_20 = [0u8; 20];
                reader.read_exact(&mut unused_20)?;
            }
            for field in fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        expect_i32(&mut reader, 0x18)?;
        if let Some(sys_fields) = &mut emitter.system_field {
            for field in sys_fields.iter_mut() {
                let typ = reader.read_i32::<LE>()?;
                if typ != 0 {
                    field.field_type = Some(typ);
                }
                let mut unused_20 = [0u8; 20];
                reader.read_exact(&mut unused_20)?;
            }
            for field in sys_fields.iter_mut() {
                field.x = read_track_nodes(&mut reader)?;
                field.y = read_track_nodes(&mut reader)?;
            }
        }

        emitter.particle_red = read_track_nodes(&mut reader)?;
        emitter.particle_green = read_track_nodes(&mut reader)?;
        emitter.particle_blue = read_track_nodes(&mut reader)?;
        emitter.particle_alpha = read_track_nodes(&mut reader)?;
        emitter.particle_brightness = read_track_nodes(&mut reader)?;
        emitter.particle_spin_angle = read_track_nodes(&mut reader)?;
        emitter.particle_spin_speed = read_track_nodes(&mut reader)?;
        emitter.particle_scale = read_track_nodes(&mut reader)?;
        emitter.particle_stretch = read_track_nodes(&mut reader)?;
        emitter.collision_reflect = read_track_nodes(&mut reader)?;
        emitter.collision_spin = read_track_nodes(&mut reader)?;
        emitter.clip_top = read_track_nodes(&mut reader)?;
        emitter.clip_bottom = read_track_nodes(&mut reader)?;
        emitter.clip_left = read_track_nodes(&mut reader)?;
        emitter.clip_right = read_track_nodes(&mut reader)?;
        emitter.animation_rate = read_track_nodes(&mut reader)?;
    }

    Ok(particles)
}

pub fn decode(data: &[u8]) -> Result<Particles, ParticlesError> {
    if let Ok(p) = decode_pc(data) {
        return Ok(p);
    }
    if let Ok(p) = decode_phone32(data) {
        return Ok(p);
    }
    if let Ok(p) = decode_phone64(data) {
        return Ok(p);
    }
    Err(ParticlesError::InvalidVariant)
}
