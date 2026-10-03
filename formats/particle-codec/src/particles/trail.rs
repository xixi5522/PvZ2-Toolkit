use crate::particles::codec::{self, PopCapCodec};
use crate::particles::error::ParticlesError;
use crate::particles::types::ParticlesTrackNode;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

/// PvZ1 Trail definition (used for motion trail effects like IceTrail)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trail {
    pub max_points: i32,
    pub min_point_distance: f32,
    pub trail_flags: i32,
    pub image: Option<String>,
    pub width_over_length: Option<Vec<ParticlesTrackNode>>,
    pub width_over_life: Option<Vec<ParticlesTrackNode>>,
    pub alpha_over_length: Option<Vec<ParticlesTrackNode>>,
    pub alpha_over_life: Option<Vec<ParticlesTrackNode>>,
}

impl PopCapCodec for Trail {
    fn decode(data: &[u8]) -> Result<Self, ParticlesError> {
        decode_trail(data)
    }

    fn encode(&self) -> Result<Vec<u8>, ParticlesError> {
        encode_trail(self)
    }
}

pub fn decode_trail(data: &[u8]) -> Result<Trail, ParticlesError> {
    let decompressed = codec::popcap_decompress(data)?;
    let mut reader = Cursor::new(&decompressed);

    // Skip 2 runtime pointers
    let mut _skip = [0u8; 8];
    reader.read_exact(&mut _skip)?;

    let max_points = reader.read_i32::<LE>()?;
    let min_point_distance = reader.read_f32::<LE>()?;
    let trail_flags = reader.read_i32::<LE>()?;

    // Skip remaining header (40 bytes: runtime pointers and allocation counts)
    let mut _header_rest = [0u8; 40];
    reader.read_exact(&mut _header_rest)?;

    // Read image name
    let image = codec::read_string_opt(&mut reader)?;

    // Read 4 track node arrays
    let width_over_length = codec::read_track_nodes(&mut reader)?;
    let width_over_life = codec::read_track_nodes(&mut reader)?;
    let alpha_over_length = codec::read_track_nodes(&mut reader)?;
    let alpha_over_life = codec::read_track_nodes(&mut reader)?;

    Ok(Trail {
        max_points,
        min_point_distance,
        trail_flags,
        image,
        width_over_length,
        width_over_life,
        alpha_over_length,
        alpha_over_life,
    })
}

pub fn encode_trail(trail: &Trail) -> Result<Vec<u8>, ParticlesError> {
    let mut buf = Vec::new();

    // Write 2 zero runtime pointers
    buf.write_i32::<LE>(0)?;
    buf.write_i32::<LE>(0)?;

    // Write header fields
    buf.write_i32::<LE>(trail.max_points)?;
    buf.write_f32::<LE>(trail.min_point_distance)?;
    buf.write_i32::<LE>(trail.trail_flags)?;

    // Write 40 bytes of zeros (runtime pointers and allocation info)
    for _ in 0..10 {
        buf.write_i32::<LE>(0)?;
    }

    // Write image string
    codec::write_string(&mut buf, trail.image.as_deref().unwrap_or(""))?;

    // Write 4 track node arrays
    codec::write_track_nodes(&mut buf, &trail.width_over_length)?;
    codec::write_track_nodes(&mut buf, &trail.width_over_life)?;
    codec::write_track_nodes(&mut buf, &trail.alpha_over_length)?;
    codec::write_track_nodes(&mut buf, &trail.alpha_over_life)?;

    // Trailing 4 bytes
    buf.write_i32::<LE>(0)?;

    // Compress with PopCap wrapper
    codec::popcap_compress(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trail_roundtrip() {
        let trail = Trail {
            max_points: 20,
            min_point_distance: 3.0,
            trail_flags: 1,
            image: Some("IMAGE_ICETRAIL".to_string()),
            width_over_length: Some(vec![
                ParticlesTrackNode {
                    time: 0.0,
                    low_value: Some(12.0),
                    high_value: Some(12.0),
                    curve_type: Some(3),
                    distribution: None,
                },
                ParticlesTrackNode {
                    time: 1.0,
                    low_value: Some(5.0),
                    high_value: Some(5.0),
                    curve_type: None,
                    distribution: None,
                },
            ]),
            width_over_life: None,
            alpha_over_length: Some(vec![
                ParticlesTrackNode {
                    time: 0.0,
                    low_value: None,
                    high_value: None,
                    curve_type: None,
                    distribution: None,
                },
                ParticlesTrackNode {
                    time: 0.2,
                    low_value: Some(0.3),
                    high_value: Some(0.3),
                    curve_type: None,
                    distribution: None,
                },
            ]),
            alpha_over_life: None,
        };

        let encoded = encode_trail(&trail).unwrap();
        let decoded = decode_trail(&encoded).unwrap();

        assert_eq!(decoded.max_points, trail.max_points);
        assert_eq!(decoded.min_point_distance, trail.min_point_distance);
        assert_eq!(decoded.trail_flags, trail.trail_flags);
        assert_eq!(decoded.image, trail.image);
        assert_eq!(decoded.width_over_length, trail.width_over_length);
        assert_eq!(decoded.width_over_life, trail.width_over_life);
        assert_eq!(decoded.alpha_over_length, trail.alpha_over_length);
        assert_eq!(decoded.alpha_over_life, trail.alpha_over_life);
    }

    #[test]
    fn test_trail_via_trait() {
        let trail = Trail {
            max_points: 10,
            min_point_distance: 2.0,
            trail_flags: 0,
            image: Some("TEST_IMAGE".to_string()),
            width_over_length: None,
            width_over_life: None,
            alpha_over_length: None,
            alpha_over_life: None,
        };

        let encoded = trail.encode().unwrap();
        let decoded = Trail::decode(&encoded).unwrap();

        assert_eq!(decoded.max_points, trail.max_points);
        assert_eq!(decoded.image, trail.image);
    }
}
