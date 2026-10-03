use crate::particles::codec::{encode_len, write_string, write_track_nodes};
use crate::particles::error::ParticlesError;
use crate::particles::types::{Particles, ParticlesField, ParticlesVersion};
use byteorder::{LE, WriteBytesExt};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

/// Alias for backward compatibility
fn write_string_by_empty<W: Write>(writer: &mut W, s: &str) -> Result<(), ParticlesError> {
    write_string(writer, s)
}

fn field_count(
    fields: &Option<Vec<ParticlesField>>,
    kind: &'static str,
) -> Result<i32, ParticlesError> {
    encode_len(kind, fields.as_ref().map_or(0, Vec::len))
}

fn write_fields<W: Write>(
    writer: &mut W,
    fields: &Option<Vec<ParticlesField>>,
    field_size: i32,
) -> Result<(), ParticlesError> {
    writer.write_i32::<LE>(field_size)?;
    if let Some(fields) = fields {
        for field in fields {
            writer.write_i32::<LE>(field.field_type.unwrap_or(0))?;

            // pad the rest of the struct with zeroes up to field_size
            let padding_ints = (field_size - 4) / 4;
            for _ in 0..padding_ints {
                writer.write_i32::<LE>(0)?;
            }
        }
        for field in fields {
            write_track_nodes(writer, &field.x)?;
            write_track_nodes(writer, &field.y)?;
        }
    }
    Ok(())
}

pub fn encode(particles: &Particles, version: ParticlesVersion) -> Result<Vec<u8>, ParticlesError> {
    let mut buf = Vec::new();
    match version {
        ParticlesVersion::PC => encode_pc(&mut buf, particles)?,
        ParticlesVersion::Phone32 => encode_phone32(&mut buf, particles)?,
        ParticlesVersion::Phone64 => encode_phone64(&mut buf, particles)?,
    }

    let mut out_buf = Vec::new();
    out_buf.write_i32::<LE>(-559022380)?; // Magic Zlib

    out_buf.write_i32::<LE>(encode_len("uncompressed payload", buf.len())?)?;

    let mut encoder = ZlibEncoder::new(out_buf, Compression::default());
    encoder.write_all(&buf)?;
    Ok(encoder.finish()?)
}

fn encode_pc<W: Write>(writer: &mut W, particles: &Particles) -> Result<(), ParticlesError> {
    writer.write_i32::<LE>(1092589901)?;
    writer.write_i32::<LE>(0)?;
    let count = encode_len("emitter", particles.emitters.len())?;
    writer.write_i32::<LE>(count)?;
    writer.write_i32::<LE>(0x164)?;

    for emitter in &particles.emitters {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(emitter.image_col.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_row.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_frames.unwrap_or(1))?;
        writer.write_i32::<LE>(emitter.animated.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.particle_flags)?;
        writer.write_i32::<LE>(emitter.emitter_type.unwrap_or(1))?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        for _ in 0..44 {
            writer.write_i32::<LE>(0)?;
        }
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.field, "field")?)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.system_field, "system field")?)?;
        for _ in 0..32 {
            writer.write_i32::<LE>(0)?;
        }
    }

    for emitter in &particles.emitters {
        write_string_by_empty(writer, emitter.image.as_deref().unwrap_or(""))?;
        write_string_by_empty(writer, emitter.name.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.system_duration)?;
        write_string_by_empty(writer, emitter.on_duration.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.cross_fade_duration)?;
        write_track_nodes(writer, &emitter.spawn_rate)?;
        write_track_nodes(writer, &emitter.spawn_min_active)?;
        write_track_nodes(writer, &emitter.spawn_max_active)?;
        write_track_nodes(writer, &emitter.spawn_max_launched)?;
        write_track_nodes(writer, &emitter.emitter_radius)?;
        write_track_nodes(writer, &emitter.emitter_offset_x)?;
        write_track_nodes(writer, &emitter.emitter_offset_y)?;
        write_track_nodes(writer, &emitter.emitter_box_x)?;
        write_track_nodes(writer, &emitter.emitter_box_y)?;
        write_track_nodes(writer, &emitter.emitter_path)?;
        write_track_nodes(writer, &emitter.emitter_skew_x)?;
        write_track_nodes(writer, &emitter.emitter_skew_y)?;
        write_track_nodes(writer, &emitter.particle_duration)?;
        write_track_nodes(writer, &emitter.system_red)?;
        write_track_nodes(writer, &emitter.system_green)?;
        write_track_nodes(writer, &emitter.system_blue)?;
        write_track_nodes(writer, &emitter.system_alpha)?;
        write_track_nodes(writer, &emitter.system_brightness)?;
        write_track_nodes(writer, &emitter.launch_speed)?;
        write_track_nodes(writer, &emitter.launch_angle)?;

        write_fields(writer, &emitter.field, 0x14)?;
        write_fields(writer, &emitter.system_field, 0x14)?;

        write_track_nodes(writer, &emitter.particle_red)?;
        write_track_nodes(writer, &emitter.particle_green)?;
        write_track_nodes(writer, &emitter.particle_blue)?;
        write_track_nodes(writer, &emitter.particle_alpha)?;
        write_track_nodes(writer, &emitter.particle_brightness)?;
        write_track_nodes(writer, &emitter.particle_spin_angle)?;
        write_track_nodes(writer, &emitter.particle_spin_speed)?;
        write_track_nodes(writer, &emitter.particle_scale)?;
        write_track_nodes(writer, &emitter.particle_stretch)?;
        write_track_nodes(writer, &emitter.collision_reflect)?;
        write_track_nodes(writer, &emitter.collision_spin)?;
        write_track_nodes(writer, &emitter.clip_top)?;
        write_track_nodes(writer, &emitter.clip_bottom)?;
        write_track_nodes(writer, &emitter.clip_left)?;
        write_track_nodes(writer, &emitter.clip_right)?;
        write_track_nodes(writer, &emitter.animation_rate)?;
    }

    Ok(())
}

fn encode_phone32<W: Write>(writer: &mut W, particles: &Particles) -> Result<(), ParticlesError> {
    writer.write_i32::<LE>(1092589901)?;
    writer.write_i32::<LE>(0)?;
    let count = encode_len("emitter", particles.emitters.len())?;
    writer.write_i32::<LE>(count)?;
    writer.write_i32::<LE>(0x164)?;

    for emitter in &particles.emitters {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(emitter.image_col.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_row.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_frames.unwrap_or(1))?;
        writer.write_i32::<LE>(emitter.animated.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.particle_flags)?;
        writer.write_i32::<LE>(emitter.emitter_type.unwrap_or(1))?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        for _ in 0..44 {
            writer.write_i32::<LE>(0)?;
        }
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.field, "field")?)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.system_field, "system field")?)?;
        for _ in 0..32 {
            writer.write_i32::<LE>(0)?;
        }
    }

    for emitter in &particles.emitters {
        let img_val: i32 = emitter
            .image
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1);
        writer.write_i32::<LE>(img_val)?;
        write_string_by_empty(writer, emitter.name.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.system_duration)?;
        write_string_by_empty(writer, emitter.on_duration.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.cross_fade_duration)?;
        write_track_nodes(writer, &emitter.spawn_rate)?;
        write_track_nodes(writer, &emitter.spawn_min_active)?;
        write_track_nodes(writer, &emitter.spawn_max_active)?;
        write_track_nodes(writer, &emitter.spawn_max_launched)?;
        write_track_nodes(writer, &emitter.emitter_radius)?;
        write_track_nodes(writer, &emitter.emitter_offset_x)?;
        write_track_nodes(writer, &emitter.emitter_offset_y)?;
        write_track_nodes(writer, &emitter.emitter_box_x)?;
        write_track_nodes(writer, &emitter.emitter_box_y)?;
        write_track_nodes(writer, &emitter.emitter_path)?;
        write_track_nodes(writer, &emitter.emitter_skew_x)?;
        write_track_nodes(writer, &emitter.emitter_skew_y)?;
        write_track_nodes(writer, &emitter.particle_duration)?;
        write_track_nodes(writer, &emitter.system_red)?;
        write_track_nodes(writer, &emitter.system_green)?;
        write_track_nodes(writer, &emitter.system_blue)?;
        write_track_nodes(writer, &emitter.system_alpha)?;
        write_track_nodes(writer, &emitter.system_brightness)?;
        write_track_nodes(writer, &emitter.launch_speed)?;
        write_track_nodes(writer, &emitter.launch_angle)?;

        write_fields(writer, &emitter.field, 0x14)?;
        write_fields(writer, &emitter.system_field, 0x14)?;

        write_track_nodes(writer, &emitter.particle_red)?;
        write_track_nodes(writer, &emitter.particle_green)?;
        write_track_nodes(writer, &emitter.particle_blue)?;
        write_track_nodes(writer, &emitter.particle_alpha)?;
        write_track_nodes(writer, &emitter.particle_brightness)?;
        write_track_nodes(writer, &emitter.particle_spin_angle)?;
        write_track_nodes(writer, &emitter.particle_spin_speed)?;
        write_track_nodes(writer, &emitter.particle_scale)?;
        write_track_nodes(writer, &emitter.particle_stretch)?;
        write_track_nodes(writer, &emitter.collision_reflect)?;
        write_track_nodes(writer, &emitter.collision_spin)?;
        write_track_nodes(writer, &emitter.clip_top)?;
        write_track_nodes(writer, &emitter.clip_bottom)?;
        write_track_nodes(writer, &emitter.clip_left)?;
        write_track_nodes(writer, &emitter.clip_right)?;
        write_track_nodes(writer, &emitter.animation_rate)?;
    }

    Ok(())
}

fn encode_phone64<W: Write>(writer: &mut W, particles: &Particles) -> Result<(), ParticlesError> {
    writer.write_i32::<LE>(-527264279)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0)?;
    let count = encode_len("emitter", particles.emitters.len())?;
    writer.write_i32::<LE>(count)?;
    writer.write_i32::<LE>(0)?;
    writer.write_i32::<LE>(0x2B0)?;

    for emitter in &particles.emitters {
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(emitter.image_col.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_row.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.image_frames.unwrap_or(1))?;
        writer.write_i32::<LE>(emitter.animated.unwrap_or(0))?;
        writer.write_i32::<LE>(emitter.particle_flags)?;
        writer.write_i32::<LE>(emitter.emitter_type.unwrap_or(1))?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        for _ in 0..88 {
            writer.write_i32::<LE>(0)?;
        }
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.field, "field")?)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(0)?;
        writer.write_i32::<LE>(field_count(&emitter.system_field, "system field")?)?;
        for _ in 0..65 {
            writer.write_i32::<LE>(0)?;
        }
    }

    for emitter in &particles.emitters {
        let img_val: i32 = emitter
            .image
            .as_ref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1);
        writer.write_i32::<LE>(img_val)?;
        write_string_by_empty(writer, emitter.name.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.system_duration)?;
        write_string_by_empty(writer, emitter.on_duration.as_deref().unwrap_or(""))?;
        write_track_nodes(writer, &emitter.cross_fade_duration)?;
        write_track_nodes(writer, &emitter.spawn_rate)?;
        write_track_nodes(writer, &emitter.spawn_min_active)?;
        write_track_nodes(writer, &emitter.spawn_max_active)?;
        write_track_nodes(writer, &emitter.spawn_max_launched)?;
        write_track_nodes(writer, &emitter.emitter_radius)?;
        write_track_nodes(writer, &emitter.emitter_offset_x)?;
        write_track_nodes(writer, &emitter.emitter_offset_y)?;
        write_track_nodes(writer, &emitter.emitter_box_x)?;
        write_track_nodes(writer, &emitter.emitter_box_y)?;
        write_track_nodes(writer, &emitter.emitter_path)?;
        write_track_nodes(writer, &emitter.emitter_skew_x)?;
        write_track_nodes(writer, &emitter.emitter_skew_y)?;
        write_track_nodes(writer, &emitter.particle_duration)?;
        write_track_nodes(writer, &emitter.system_red)?;
        write_track_nodes(writer, &emitter.system_green)?;
        write_track_nodes(writer, &emitter.system_blue)?;
        write_track_nodes(writer, &emitter.system_alpha)?;
        write_track_nodes(writer, &emitter.system_brightness)?;
        write_track_nodes(writer, &emitter.launch_speed)?;
        write_track_nodes(writer, &emitter.launch_angle)?;

        write_fields(writer, &emitter.field, 0x18)?;
        write_fields(writer, &emitter.system_field, 0x18)?;

        write_track_nodes(writer, &emitter.particle_red)?;
        write_track_nodes(writer, &emitter.particle_green)?;
        write_track_nodes(writer, &emitter.particle_blue)?;
        write_track_nodes(writer, &emitter.particle_alpha)?;
        write_track_nodes(writer, &emitter.particle_brightness)?;
        write_track_nodes(writer, &emitter.particle_spin_angle)?;
        write_track_nodes(writer, &emitter.particle_spin_speed)?;
        write_track_nodes(writer, &emitter.particle_scale)?;
        write_track_nodes(writer, &emitter.particle_stretch)?;
        write_track_nodes(writer, &emitter.collision_reflect)?;
        write_track_nodes(writer, &emitter.collision_spin)?;
        write_track_nodes(writer, &emitter.clip_top)?;
        write_track_nodes(writer, &emitter.clip_bottom)?;
        write_track_nodes(writer, &emitter.clip_left)?;
        write_track_nodes(writer, &emitter.clip_right)?;
        write_track_nodes(writer, &emitter.animation_rate)?;
    }

    Ok(())
}
