//! PopCap PvZ1 XML format parser/serializer for Particles and Trail.
//!
//! Handles the compact TrackNode format: `value,time% [low high] CurveName`

use crate::particles::error::ParticlesError;
use crate::particles::trail::Trail;
use crate::particles::types::{Particles, ParticlesEmitter, ParticlesField, ParticlesTrackNode};
use quick_xml::Reader;
use quick_xml::events::Event;

// ─── Curve Type Mapping ──────────────────────────────────────────────────────

fn curve_name_to_id(name: &str) -> Option<i32> {
    match name {
        "Linear" => Some(1),
        "EaseIn" => Some(2),
        "EaseOut" => Some(3),
        "EaseSinWave" => Some(4),
        "EaseInOutWeak" => Some(5),
        "Bounce" => Some(6),
        _ => None,
    }
}

fn curve_id_to_name(id: i32) -> Option<&'static str> {
    match id {
        1 => None, // Linear is default, omit
        2 => Some("EaseIn"),
        3 => Some("EaseOut"),
        4 => Some("EaseSinWave"),
        5 => Some("EaseInOutWeak"),
        6 => Some("Bounce"),
        _ => None,
    }
}

// ─── Field Type Mapping ──────────────────────────────────────────────────────

fn field_type_name_to_id(name: &str) -> i32 {
    match name {
        "Position" => 0,
        "Velocity" => 1,
        "Acceleration" => 2,
        "Friction" => 3,
        "Shake" => 4,
        "Circle" => 5,
        "Away" => 6,
        "GroundConstraint" => 7,
        "SystemPosition" => 8,
        _ => 0,
    }
}

fn field_type_id_to_name(id: i32) -> &'static str {
    match id {
        0 => "Position",
        1 => "Velocity",
        2 => "Acceleration",
        3 => "Friction",
        4 => "Shake",
        5 => "Circle",
        6 => "Away",
        7 => "GroundConstraint",
        8 => "SystemPosition",
        _ => "Position",
    }
}

// ─── Emitter Type Mapping ────────────────────────────────────────────────────

fn emitter_type_name_to_id(name: &str) -> i32 {
    match name {
        "Circle" => 1,
        "Box" => 2,
        "CircleEvenSpacing" => 3,
        _ => 0,
    }
}

fn emitter_type_id_to_name(id: i32) -> Option<&'static str> {
    match id {
        0 => None, // default, omit
        1 => Some("Circle"),
        2 => Some("Box"),
        3 => Some("CircleEvenSpacing"),
        _ => None,
    }
}

// ─── Particle Flags ──────────────────────────────────────────────────────────

const FLAG_RANDOM_LAUNCH_SPIN: i32 = 1 << 0;
const FLAG_PARTICLE_LOOPS: i32 = 1 << 3;
const FLAG_SYSTEM_LOOPS: i32 = 1 << 4;
const FLAG_DIE_IF_OVERLOADED: i32 = 1 << 5;
const FLAG_FULL_SCREEN: i32 = 1 << 7;
const FLAG_ADDITIVE: i32 = 1 << 8;
const FLAG_HARDWARE_ONLY: i32 = 1 << 9;

// ─── TrackNode Parsing ───────────────────────────────────────────────────────

/// Parse the compact PopCap TrackNode text format.
///
/// Format examples:
/// - `12` → single node {time:0, low:12, high:12}
/// - `.3,20` → {time:0.2, low:0.3, high:0.3}
/// - `[.4 1.5]` → {time:0, low:0.4, high:1.5}
/// - `[.4 1.5],10` → {time:0.1, low:0.4, high:1.5}
/// - `EaseOut` → sets previous node's curve_type
/// - `12 EaseOut 5` → two nodes with curve on first
pub fn parse_track_nodes(text: &str) -> Vec<ParticlesTrackNode> {
    let text = text.trim();
    if text.is_empty() {
        return vec![];
    }

    let mut nodes: Vec<ParticlesTrackNode> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();

    // Tokenize: split by spaces, but keep [...] as single tokens
    let mut current = String::new();
    let mut in_bracket = false;
    for ch in text.chars() {
        match ch {
            '[' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
                in_bracket = true;
                current.push(ch);
            }
            ']' => {
                current.push(ch);
                in_bracket = false;
                // Check if followed by ,time
            }
            ' ' | '\t' if !in_bracket => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }

    // Parse tokens
    for token in &tokens {
        // Check if it's a curve name
        if let Some(curve_id) = curve_name_to_id(token) {
            // Apply curve to previous node
            if let Some(last) = nodes.last_mut() {
                last.curve_type = Some(curve_id);
            }
            continue;
        }

        // Parse the token as a value/range with optional time
        let (value_part, time_part) = if token.starts_with('[') {
            // Range token: [low high] or [low high],time
            if let Some(bracket_end) = token.find(']') {
                let range_str = &token[1..bracket_end];
                let after = &token[bracket_end + 1..];
                let time = if let Some(after) = after.strip_prefix(',') {
                    after.parse::<f32>().ok()
                } else {
                    None
                };
                // Parse range
                let parts: Vec<&str> = range_str.split_whitespace().collect();
                if parts.len() >= 2 {
                    let low = parts[0].parse::<f32>().unwrap_or(0.0);
                    let high = parts[1].parse::<f32>().unwrap_or(0.0);
                    nodes.push(ParticlesTrackNode {
                        time: time.map(|t| t / 100.0).unwrap_or(0.0),
                        low_value: Some(low),
                        high_value: Some(high),
                        curve_type: None,
                        distribution: None,
                    });
                } else if parts.len() == 1 {
                    let val = parts[0].parse::<f32>().unwrap_or(0.0);
                    nodes.push(ParticlesTrackNode {
                        time: time.map(|t| t / 100.0).unwrap_or(0.0),
                        low_value: Some(val),
                        high_value: Some(val),
                        curve_type: None,
                        distribution: None,
                    });
                }
                continue;
            }
            continue;
        } else {
            // Simple value or value,time
            if let Some(comma_pos) = token.find(',') {
                let val = &token[..comma_pos];
                let time = &token[comma_pos + 1..];
                (val.to_string(), time.parse::<f32>().ok())
            } else {
                (token.clone(), None)
            }
        };

        let val = value_part.parse::<f32>().unwrap_or(0.0);
        nodes.push(ParticlesTrackNode {
            time: time_part.map(|t| t / 100.0).unwrap_or(0.0),
            low_value: if val != 0.0 { Some(val) } else { None },
            high_value: if val != 0.0 { Some(val) } else { None },
            curve_type: None,
            distribution: None,
        });
    }

    // Auto-assign times for nodes without explicit time (evenly distributed)
    // Only if there's more than 1 node and all times are 0
    let needs_auto_time = nodes.len() > 1
        && nodes.iter().all(|node| node.time == 0.0)
        && tokens.iter().all(|t| !t.contains(','));

    if needs_auto_time && nodes.len() > 1 {
        // Check if ALL tokens had no explicit time (no commas)
        let has_any_time = tokens.iter().any(|t| {
            if t.starts_with('[') {
                t.contains("],")
            } else {
                t.contains(',') && curve_name_to_id(t).is_none()
            }
        });
        if !has_any_time {
            // Don't auto-assign — keep time=0 for all. The binary format
            // stores the actual time values; the XML shorthand just omits them
            // when they increment evenly from 0.
        }
    }

    nodes
}

/// Format TrackNodes to PopCap compact text format.
pub fn format_track_nodes(nodes: &[ParticlesTrackNode]) -> String {
    let mut parts: Vec<String> = Vec::new();

    for node in nodes {
        let low = node.low_value.unwrap_or(0.0);
        let high = node.high_value.unwrap_or(0.0);
        let time_pct = node.time * 100.0;

        let value_str = if low != high {
            // Range
            let range = format!("[{} {}]", format_float(low), format_float(high));
            if time_pct != 0.0 {
                format!("{},{}", range, format_float(time_pct))
            } else {
                range
            }
        } else {
            // Single value
            if time_pct != 0.0 {
                format!("{},{}", format_float(low), format_float(time_pct))
            } else {
                format_float(low)
            }
        };
        parts.push(value_str);

        // Add curve name if non-default
        if let Some(curve_id) = node.curve_type
            && let Some(name) = curve_id_to_name(curve_id)
        {
            parts.push(name.to_string());
        }
    }

    parts.join(" ")
}

/// Format a float, rounding to avoid f32 precision artifacts like 79.99999
fn format_float(v: f32) -> String {
    // Round to 6 significant digits to eliminate f32 noise
    let rounded = (v * 1000000.0).round() / 1000000.0;
    // Check if close to an integer
    let nearest_int = rounded.round();
    if (rounded - nearest_int).abs() < 0.0001 && nearest_int.abs() < 1e7 {
        format!("{}", nearest_int as i64)
    } else {
        // Format with enough precision but strip trailing zeros
        let s = format!("{rounded:.6}");
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

// ─── Particles XML Serializer ────────────────────────────────────────────────

pub fn format_particles_xml(particles: &Particles) -> Result<String, ParticlesError> {
    let mut output = String::new();

    for emitter in &particles.emitters {
        output.push_str("<Emitter>\n");
        write_emitter_xml(&mut output, emitter, "  ");
        output.push_str("</Emitter>\n");
    }

    Ok(output)
}

fn write_emitter_xml(out: &mut String, e: &ParticlesEmitter, indent: &str) {
    // Name
    if let Some(ref name) = e.name {
        write_tag(out, indent, "Name", name);
    }

    // Flags as individual tags
    let flags = e.particle_flags;
    if flags & FLAG_RANDOM_LAUNCH_SPIN != 0 {
        write_tag(out, indent, "RandomLaunchSpin", "1");
    }
    if flags & FLAG_DIE_IF_OVERLOADED != 0 {
        write_tag(out, indent, "DieIfOverloaded", "1");
    }
    if flags & FLAG_PARTICLE_LOOPS != 0 {
        write_tag(out, indent, "ParticleLoops", "1");
    }
    if flags & FLAG_SYSTEM_LOOPS != 0 {
        write_tag(out, indent, "SystemLoops", "1");
    }
    if flags & FLAG_FULL_SCREEN != 0 {
        write_tag(out, indent, "FullScreen", "1");
    }
    if flags & FLAG_ADDITIVE != 0 {
        write_tag(out, indent, "Additive", "1");
    }
    if flags & FLAG_HARDWARE_ONLY != 0 {
        write_tag(out, indent, "HardwareOnly", "1");
    }

    // Image
    if let Some(ref img) = e.image {
        write_tag(out, indent, "Image", img);
    }
    if let Some(ref path) = e.image_path {
        write_tag(out, indent, "ImagePath", path);
    }
    if let Some(v) = e.image_col
        && v != 0
    {
        write_tag(out, indent, "ImageCol", &v.to_string());
    }
    if let Some(v) = e.image_row
        && v != 0
    {
        write_tag(out, indent, "ImageRow", &v.to_string());
    }
    if let Some(v) = e.image_frames
        && v != 0
    {
        write_tag(out, indent, "ImageFrames", &v.to_string());
    }
    if let Some(v) = e.animated
        && v != 0
    {
        write_tag(out, indent, "Animated", &v.to_string());
    }

    // Emitter type
    if let Some(v) = e.emitter_type
        && let Some(name) = emitter_type_id_to_name(v)
    {
        write_tag(out, indent, "EmitterType", name);
    }

    // Track nodes
    write_track_tag(out, indent, "SystemDuration", &e.system_duration);
    if let Some(on_duration) = &e.on_duration {
        write_tag(out, indent, "OnDuration", on_duration);
    }
    write_track_tag(out, indent, "CrossFadeDuration", &e.cross_fade_duration);
    write_track_tag(out, indent, "SpawnRate", &e.spawn_rate);
    write_track_tag(out, indent, "SpawnMinActive", &e.spawn_min_active);
    write_track_tag(out, indent, "SpawnMaxActive", &e.spawn_max_active);
    write_track_tag(out, indent, "SpawnMaxLaunched", &e.spawn_max_launched);
    write_track_tag(out, indent, "EmitterRadius", &e.emitter_radius);
    write_track_tag(out, indent, "EmitterOffsetX", &e.emitter_offset_x);
    write_track_tag(out, indent, "EmitterOffsetY", &e.emitter_offset_y);
    write_track_tag(out, indent, "EmitterBoxX", &e.emitter_box_x);
    write_track_tag(out, indent, "EmitterBoxY", &e.emitter_box_y);
    write_track_tag(out, indent, "EmitterPath", &e.emitter_path);
    write_track_tag(out, indent, "EmitterSkewX", &e.emitter_skew_x);
    write_track_tag(out, indent, "EmitterSkewY", &e.emitter_skew_y);
    write_track_tag(out, indent, "ParticleDuration", &e.particle_duration);
    write_track_tag(out, indent, "SystemRed", &e.system_red);
    write_track_tag(out, indent, "SystemGreen", &e.system_green);
    write_track_tag(out, indent, "SystemBlue", &e.system_blue);
    write_track_tag(out, indent, "SystemAlpha", &e.system_alpha);
    write_track_tag(out, indent, "SystemBrightness", &e.system_brightness);
    write_track_tag(out, indent, "LaunchSpeed", &e.launch_speed);
    write_track_tag(out, indent, "LaunchAngle", &e.launch_angle);
    write_track_tag(out, indent, "ParticleRed", &e.particle_red);
    write_track_tag(out, indent, "ParticleGreen", &e.particle_green);
    write_track_tag(out, indent, "ParticleBlue", &e.particle_blue);
    write_track_tag(out, indent, "ParticleAlpha", &e.particle_alpha);
    write_track_tag(out, indent, "ParticleBrightness", &e.particle_brightness);
    write_track_tag(out, indent, "ParticleSpinAngle", &e.particle_spin_angle);
    write_track_tag(out, indent, "ParticleSpinSpeed", &e.particle_spin_speed);
    write_track_tag(out, indent, "ParticleScale", &e.particle_scale);
    write_track_tag(out, indent, "ParticleStretch", &e.particle_stretch);
    write_track_tag(out, indent, "CollisionReflect", &e.collision_reflect);
    write_track_tag(out, indent, "CollisionSpin", &e.collision_spin);
    write_track_tag(out, indent, "ClipTop", &e.clip_top);
    write_track_tag(out, indent, "ClipBottom", &e.clip_bottom);
    write_track_tag(out, indent, "ClipLeft", &e.clip_left);
    write_track_tag(out, indent, "ClipRight", &e.clip_right);
    write_track_tag(out, indent, "AnimationRate", &e.animation_rate);

    // Fields
    write_fields_xml(out, indent, "Field", &e.field);
    write_fields_xml(out, indent, "SystemField", &e.system_field);
}

fn write_tag(out: &mut String, indent: &str, tag: &str, value: &str) {
    out.push_str(indent);
    out.push('<');
    out.push_str(tag);
    out.push('>');
    out.push_str(value);
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn write_track_tag(
    out: &mut String,
    indent: &str,
    tag: &str,
    nodes: &Option<Vec<ParticlesTrackNode>>,
) {
    if let Some(nodes) = nodes
        && !nodes.is_empty()
    {
        let text = format_track_nodes(nodes);
        write_tag(out, indent, tag, &text);
    }
}

fn write_fields_xml(
    out: &mut String,
    indent: &str,
    tag: &str,
    fields: &Option<Vec<ParticlesField>>,
) {
    if let Some(fields) = fields {
        for field in fields {
            out.push_str(indent);
            out.push('<');
            out.push_str(tag);
            out.push_str(">\n");

            let inner = format!("{indent}  ");
            let ft = field.field_type.unwrap_or(0);
            write_tag(out, &inner, "FieldType", field_type_id_to_name(ft));
            write_track_tag(out, &inner, "X", &field.x);
            write_track_tag(out, &inner, "Y", &field.y);

            out.push_str(indent);
            out.push_str("</");
            out.push_str(tag);
            out.push_str(">\n");
        }
    }
}

// ─── Trail XML Serializer ────────────────────────────────────────────────────

pub fn format_trail_xml(trail: &Trail) -> Result<String, ParticlesError> {
    let mut out = String::new();
    if let Some(ref img) = trail.image {
        write_tag(&mut out, "", "Image", img);
    }
    write_tag(&mut out, "", "MaxPoints", &trail.max_points.to_string());
    if trail.trail_flags != 0 {
        write_tag(&mut out, "", "Loops", &trail.trail_flags.to_string());
    }
    write_tag(
        &mut out,
        "",
        "MinPointDistance",
        &format_float(trail.min_point_distance),
    );
    write_track_tag(&mut out, "", "WidthOverLength", &trail.width_over_length);
    write_track_tag(&mut out, "", "WidthOverLife", &trail.width_over_life);
    write_track_tag(&mut out, "", "AlphaOverLength", &trail.alpha_over_length);
    write_track_tag(&mut out, "", "AlphaOverLife", &trail.alpha_over_life);
    Ok(out)
}

// ─── Particles XML Parser ────────────────────────────────────────────────────

pub fn parse_particles_xml(xml_str: &str) -> Result<Particles, ParticlesError> {
    let mut particles = Particles::default();
    let mut reader = Reader::from_str(xml_str);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == "Emitter" => {
                let emitter = parse_emitter_xml(&mut reader)?;
                particles.emitters.push(emitter);
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ParticlesError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("XML parse error: {e}"),
                )));
            }
        }
        buf.clear();
    }
    Ok(particles)
}

fn parse_emitter_xml(reader: &mut Reader<&[u8]>) -> Result<ParticlesEmitter, ParticlesError> {
    let mut emitter = ParticlesEmitter::default();
    let mut buf = Vec::new();
    let mut _current_field_tag: Option<String> = None; // "Field" or "SystemField"
    let mut current_field = ParticlesField::default();
    let mut in_field = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_string();
                if tag == "Field" || tag == "SystemField" {
                    _current_field_tag = Some(tag);
                    current_field = ParticlesField::default();
                    in_field = true;
                } else if in_field {
                    let text = read_text_content(reader, &tag)?;
                    match tag.as_str() {
                        "FieldType" => {
                            current_field.field_type = Some(field_type_name_to_id(&text));
                        }
                        "X" => {
                            current_field.x = Some(parse_track_nodes(&text));
                        }
                        "Y" => {
                            current_field.y = Some(parse_track_nodes(&text));
                        }
                        _ => {}
                    }
                } else {
                    let text = read_text_content(reader, &tag)?;
                    apply_emitter_tag(&mut emitter, &tag, &text);
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_string();
                if tag == "Emitter" {
                    break;
                }
                if (tag == "Field" || tag == "SystemField") && in_field {
                    if tag == "Field" {
                        emitter
                            .field
                            .get_or_insert_with(Vec::new)
                            .push(current_field.clone());
                    } else {
                        emitter
                            .system_field
                            .get_or_insert_with(Vec::new)
                            .push(current_field.clone());
                    }
                    in_field = false;
                    _current_field_tag = None;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ParticlesError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("XML parse error: {e}"),
                )));
            }
        }
        buf.clear();
    }
    Ok(emitter)
}

fn read_text_content(reader: &mut Reader<&[u8]>, tag_name: &str) -> Result<String, ParticlesError> {
    let mut buf = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                text = e.as_ref().to_string();
            }
            Ok(Event::End(ref e)) => {
                let name = e.name().as_ref().to_string();
                if name == tag_name {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buf.clear();
    }
    Ok(text)
}

fn apply_emitter_tag(emitter: &mut ParticlesEmitter, tag: &str, text: &str) {
    match tag {
        "Name" => emitter.name = Some(text.to_string()),
        "Image" => emitter.image = Some(text.to_string()),
        "ImagePath" => emitter.image_path = Some(text.to_string()),
        "ImageCol" => emitter.image_col = text.parse().ok(),
        "ImageRow" => emitter.image_row = text.parse().ok(),
        "ImageFrames" => emitter.image_frames = text.parse().ok(),
        "Animated" => emitter.animated = text.parse().ok(),
        "EmitterType" => emitter.emitter_type = Some(emitter_type_name_to_id(text)),
        "OnDuration" => emitter.on_duration = Some(text.to_string()),

        // Flags
        "RandomLaunchSpin" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_RANDOM_LAUNCH_SPIN;
            }
        }
        "ParticleLoops" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_PARTICLE_LOOPS;
            }
        }
        "SystemLoops" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_SYSTEM_LOOPS;
            }
        }
        "DieIfOverloaded" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_DIE_IF_OVERLOADED;
            }
        }
        "FullScreen" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_FULL_SCREEN;
            }
        }
        "Additive" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_ADDITIVE;
            }
        }
        "HardwareOnly" => {
            if text == "1" {
                emitter.particle_flags |= FLAG_HARDWARE_ONLY;
            }
        }

        // Track nodes
        "SystemDuration" => emitter.system_duration = Some(parse_track_nodes(text)),
        "CrossFadeDuration" => emitter.cross_fade_duration = Some(parse_track_nodes(text)),
        "SpawnRate" => emitter.spawn_rate = Some(parse_track_nodes(text)),
        "SpawnMinActive" => emitter.spawn_min_active = Some(parse_track_nodes(text)),
        "SpawnMaxActive" => emitter.spawn_max_active = Some(parse_track_nodes(text)),
        "SpawnMaxLaunched" => emitter.spawn_max_launched = Some(parse_track_nodes(text)),
        "EmitterRadius" => emitter.emitter_radius = Some(parse_track_nodes(text)),
        "EmitterOffsetX" => emitter.emitter_offset_x = Some(parse_track_nodes(text)),
        "EmitterOffsetY" => emitter.emitter_offset_y = Some(parse_track_nodes(text)),
        "EmitterBoxX" => emitter.emitter_box_x = Some(parse_track_nodes(text)),
        "EmitterBoxY" => emitter.emitter_box_y = Some(parse_track_nodes(text)),
        "EmitterPath" => emitter.emitter_path = Some(parse_track_nodes(text)),
        "EmitterSkewX" => emitter.emitter_skew_x = Some(parse_track_nodes(text)),
        "EmitterSkewY" => emitter.emitter_skew_y = Some(parse_track_nodes(text)),
        "ParticleDuration" => emitter.particle_duration = Some(parse_track_nodes(text)),
        "SystemRed" => emitter.system_red = Some(parse_track_nodes(text)),
        "SystemGreen" => emitter.system_green = Some(parse_track_nodes(text)),
        "SystemBlue" => emitter.system_blue = Some(parse_track_nodes(text)),
        "SystemAlpha" => emitter.system_alpha = Some(parse_track_nodes(text)),
        "SystemBrightness" => emitter.system_brightness = Some(parse_track_nodes(text)),
        "LaunchSpeed" => emitter.launch_speed = Some(parse_track_nodes(text)),
        "LaunchAngle" => emitter.launch_angle = Some(parse_track_nodes(text)),
        "ParticleRed" => emitter.particle_red = Some(parse_track_nodes(text)),
        "ParticleGreen" => emitter.particle_green = Some(parse_track_nodes(text)),
        "ParticleBlue" => emitter.particle_blue = Some(parse_track_nodes(text)),
        "ParticleAlpha" => emitter.particle_alpha = Some(parse_track_nodes(text)),
        "ParticleBrightness" => emitter.particle_brightness = Some(parse_track_nodes(text)),
        "ParticleSpinAngle" => emitter.particle_spin_angle = Some(parse_track_nodes(text)),
        "ParticleSpinSpeed" => emitter.particle_spin_speed = Some(parse_track_nodes(text)),
        "ParticleScale" => emitter.particle_scale = Some(parse_track_nodes(text)),
        "ParticleStretch" => emitter.particle_stretch = Some(parse_track_nodes(text)),
        "CollisionReflect" => emitter.collision_reflect = Some(parse_track_nodes(text)),
        "CollisionSpin" => emitter.collision_spin = Some(parse_track_nodes(text)),
        "ClipTop" => emitter.clip_top = Some(parse_track_nodes(text)),
        "ClipBottom" => emitter.clip_bottom = Some(parse_track_nodes(text)),
        "ClipLeft" => emitter.clip_left = Some(parse_track_nodes(text)),
        "ClipRight" => emitter.clip_right = Some(parse_track_nodes(text)),
        "AnimationRate" => emitter.animation_rate = Some(parse_track_nodes(text)),
        _ => {} // Ignore unknown tags
    }
}

// ─── Trail XML Parser ────────────────────────────────────────────────────────

pub fn parse_trail_xml(xml_str: &str) -> Result<Trail, ParticlesError> {
    let mut trail = Trail {
        max_points: 0,
        min_point_distance: 0.0,
        trail_flags: 0,
        image: None,
        width_over_length: None,
        width_over_life: None,
        alpha_over_length: None,
        alpha_over_life: None,
    };

    // Wrap in a root element for parsing
    let wrapped = format!("<Trail>{xml_str}</Trail>");
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_trail = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_string();
                if tag == "Trail" {
                    in_trail = true;
                } else if in_trail {
                    let text = read_text_content(&mut reader, &tag)?;
                    match tag.as_str() {
                        "Image" => trail.image = Some(text),
                        "MaxPoints" => trail.max_points = text.parse().unwrap_or(0),
                        "Loops" => trail.trail_flags = text.parse().unwrap_or(0),
                        "MinPointDistance" => {
                            trail.min_point_distance = text.parse().unwrap_or(0.0)
                        }
                        "WidthOverLength" => {
                            trail.width_over_length = Some(parse_track_nodes(&text))
                        }
                        "WidthOverLife" => trail.width_over_life = Some(parse_track_nodes(&text)),
                        "AlphaOverLength" => {
                            trail.alpha_over_length = Some(parse_track_nodes(&text))
                        }
                        "AlphaOverLife" => trail.alpha_over_life = Some(parse_track_nodes(&text)),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == "Trail" {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => {
                return Err(ParticlesError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("XML parse error: {e}"),
                )));
            }
        }
        buf.clear();
    }
    Ok(trail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_track_single_value() {
        let nodes = parse_track_nodes("12");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].low_value, Some(12.0));
        assert_eq!(nodes[0].high_value, Some(12.0));
        assert_eq!(nodes[0].time, 0.0);
    }

    #[test]
    fn test_parse_track_value_with_time() {
        let nodes = parse_track_nodes(".3,20");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].low_value, Some(0.3));
        assert!((nodes[0].time - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_parse_track_range() {
        let nodes = parse_track_nodes("[.4 1.5]");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].low_value, Some(0.4));
        assert_eq!(nodes[0].high_value, Some(1.5));
    }

    #[test]
    fn test_parse_track_range_with_time() {
        let nodes = parse_track_nodes("[.4 1.5],10");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].low_value, Some(0.4));
        assert_eq!(nodes[0].high_value, Some(1.5));
        assert!((nodes[0].time - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_parse_track_with_curve() {
        let nodes = parse_track_nodes("12 EaseOut 5");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].low_value, Some(12.0));
        assert_eq!(nodes[0].curve_type, Some(3)); // EaseOut
        assert_eq!(nodes[1].low_value, Some(5.0));
    }

    #[test]
    fn test_parse_track_alpha_pattern() {
        let nodes = parse_track_nodes("1,80 0");
        assert_eq!(nodes.len(), 2);
        assert!((nodes[0].time - 0.8).abs() < 0.001);
        assert_eq!(nodes[0].low_value, Some(1.0));
        assert_eq!(nodes[1].low_value, None); // 0.0 becomes None
    }

    #[test]
    fn test_format_track_nodes_simple() {
        let nodes = vec![ParticlesTrackNode {
            time: 0.0,
            low_value: Some(12.0),
            high_value: Some(12.0),
            curve_type: None,
            distribution: None,
        }];
        assert_eq!(format_track_nodes(&nodes), "12");
    }

    #[test]
    fn test_format_track_nodes_range() {
        let nodes = vec![ParticlesTrackNode {
            time: 0.0,
            low_value: Some(0.4),
            high_value: Some(1.5),
            curve_type: None,
            distribution: None,
        }];
        assert_eq!(format_track_nodes(&nodes), "[0.4 1.5]");
    }

    #[test]
    fn test_format_track_nodes_with_curve() {
        let nodes = vec![
            ParticlesTrackNode {
                time: 0.0,
                low_value: Some(12.0),
                high_value: Some(12.0),
                curve_type: Some(3),
                distribution: None,
            },
            ParticlesTrackNode {
                time: 0.0,
                low_value: Some(5.0),
                high_value: Some(5.0),
                curve_type: None,
                distribution: None,
            },
        ];
        assert_eq!(format_track_nodes(&nodes), "12 EaseOut 5");
    }

    #[test]
    fn test_parse_particles_xml() {
        let xml = r#"
<Emitter>
  <Name>Test</Name>
  <SpawnMinActive>1</SpawnMinActive>
  <Image>IMAGE_TEST</Image>
  <ParticleDuration>20</ParticleDuration>
  <SystemDuration>20</SystemDuration>
  <RandomLaunchSpin>1</RandomLaunchSpin>
  <Field>
    <FieldType>Position</FieldType>
    <X>0 15</X>
  </Field>
</Emitter>
"#;
        let p = parse_particles_xml(xml).unwrap();
        assert_eq!(p.emitters.len(), 1);
        assert_eq!(p.emitters[0].name.as_deref(), Some("Test"));
        assert_eq!(p.emitters[0].image.as_deref(), Some("IMAGE_TEST"));
        assert_eq!(p.emitters[0].particle_flags & FLAG_RANDOM_LAUNCH_SPIN, 1);
        assert!(p.emitters[0].field.is_some());
        let fields = p.emitters[0].field.as_ref().unwrap();
        assert_eq!(fields[0].field_type, Some(0)); // Position
    }

    #[test]
    fn test_particles_xml_preserves_on_duration() {
        let particles = Particles {
            emitters: vec![ParticlesEmitter {
                name: Some("TimedEmitter".to_string()),
                on_duration: Some("12.5".to_string()),
                ..Default::default()
            }],
        };
        let formatted = format_particles_xml(&particles).unwrap();
        assert!(formatted.contains("<OnDuration>12.5</OnDuration>"));
        let decoded = parse_particles_xml(&formatted).unwrap();
        assert_eq!(decoded.emitters[0].on_duration.as_deref(), Some("12.5"));
    }

    #[test]
    fn test_parse_trail_xml() {
        let xml = r#"
<Image>IMAGE_ICETRAIL</Image>
<MaxPoints>20</MaxPoints>
<Loops>1</Loops>
<MinPointDistance>3</MinPointDistance>
<WidthOverLength>12 EaseOut 5</WidthOverLength>
<AlphaOverLength>0,0 .3,20</AlphaOverLength>
"#;
        let trail = parse_trail_xml(xml).unwrap();
        assert_eq!(trail.image.as_deref(), Some("IMAGE_ICETRAIL"));
        assert_eq!(trail.max_points, 20);
        assert_eq!(trail.trail_flags, 1);
        assert_eq!(trail.min_point_distance, 3.0);

        let wol = trail.width_over_length.unwrap();
        assert_eq!(wol.len(), 2);
        assert_eq!(wol[0].low_value, Some(12.0));
        assert_eq!(wol[0].curve_type, Some(3));
        assert_eq!(wol[1].low_value, Some(5.0));
    }
}
