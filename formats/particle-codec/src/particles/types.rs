use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Particles {
    pub emitters: Vec<ParticlesEmitter>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParticlesEmitter {
    pub name: Option<String>,
    pub image: Option<String>,
    pub image_path: Option<String>,
    pub image_col: Option<i32>,
    pub image_row: Option<i32>,
    pub image_frames: Option<i32>,
    pub animated: Option<i32>,
    pub particle_flags: i32,
    pub emitter_type: Option<i32>,

    pub on_duration: Option<String>,

    pub system_duration: Option<Vec<ParticlesTrackNode>>,
    pub cross_fade_duration: Option<Vec<ParticlesTrackNode>>,
    pub spawn_rate: Option<Vec<ParticlesTrackNode>>,
    pub spawn_min_active: Option<Vec<ParticlesTrackNode>>,
    pub spawn_max_active: Option<Vec<ParticlesTrackNode>>,
    pub spawn_max_launched: Option<Vec<ParticlesTrackNode>>,
    pub emitter_radius: Option<Vec<ParticlesTrackNode>>,
    pub emitter_offset_x: Option<Vec<ParticlesTrackNode>>,
    pub emitter_offset_y: Option<Vec<ParticlesTrackNode>>,
    pub emitter_box_x: Option<Vec<ParticlesTrackNode>>,
    pub emitter_box_y: Option<Vec<ParticlesTrackNode>>,
    pub emitter_path: Option<Vec<ParticlesTrackNode>>,
    pub emitter_skew_x: Option<Vec<ParticlesTrackNode>>,
    pub emitter_skew_y: Option<Vec<ParticlesTrackNode>>,
    pub particle_duration: Option<Vec<ParticlesTrackNode>>,
    pub system_red: Option<Vec<ParticlesTrackNode>>,
    pub system_green: Option<Vec<ParticlesTrackNode>>,
    pub system_blue: Option<Vec<ParticlesTrackNode>>,
    pub system_alpha: Option<Vec<ParticlesTrackNode>>,
    pub system_brightness: Option<Vec<ParticlesTrackNode>>,
    pub launch_speed: Option<Vec<ParticlesTrackNode>>,
    pub launch_angle: Option<Vec<ParticlesTrackNode>>,

    pub field: Option<Vec<ParticlesField>>,
    pub system_field: Option<Vec<ParticlesField>>,

    pub particle_red: Option<Vec<ParticlesTrackNode>>,
    pub particle_green: Option<Vec<ParticlesTrackNode>>,
    pub particle_blue: Option<Vec<ParticlesTrackNode>>,
    pub particle_alpha: Option<Vec<ParticlesTrackNode>>,
    pub particle_brightness: Option<Vec<ParticlesTrackNode>>,
    pub particle_spin_angle: Option<Vec<ParticlesTrackNode>>,
    pub particle_spin_speed: Option<Vec<ParticlesTrackNode>>,
    pub particle_scale: Option<Vec<ParticlesTrackNode>>,
    pub particle_stretch: Option<Vec<ParticlesTrackNode>>,
    pub collision_reflect: Option<Vec<ParticlesTrackNode>>,
    pub collision_spin: Option<Vec<ParticlesTrackNode>>,
    pub clip_top: Option<Vec<ParticlesTrackNode>>,
    pub clip_bottom: Option<Vec<ParticlesTrackNode>>,
    pub clip_left: Option<Vec<ParticlesTrackNode>>,
    pub clip_right: Option<Vec<ParticlesTrackNode>>,
    pub animation_rate: Option<Vec<ParticlesTrackNode>>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParticlesField {
    pub field_type: Option<i32>,
    pub x: Option<Vec<ParticlesTrackNode>>,
    pub y: Option<Vec<ParticlesTrackNode>>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParticlesTrackNode {
    pub time: f32,
    pub low_value: Option<f32>,
    pub high_value: Option<f32>,
    pub curve_type: Option<i32>,
    pub distribution: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticlesVersion {
    #[default]
    PC = 1,
    Phone32 = 2,
    Phone64 = 3,
}
