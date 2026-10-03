pub mod codec;
pub mod error;
pub mod reader;
pub mod trail;
pub mod types;
pub mod writer;
pub mod xml;

pub use codec::PopCapCodec;
pub use error::ParticlesError;
pub use reader::{decode, decode_pc, decode_phone32, decode_phone64};
pub use types::{
    Particles, ParticlesEmitter, ParticlesField, ParticlesTrackNode, ParticlesVersion,
};
pub use writer::encode;

// Implement PopCapCodec for Particles
impl PopCapCodec for Particles {
    fn decode(data: &[u8]) -> Result<Self, ParticlesError> {
        reader::decode(data)
    }

    fn encode(&self) -> Result<Vec<u8>, ParticlesError> {
        writer::encode(self, ParticlesVersion::PC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_particles() -> Particles {
        let mut particles = Particles::default();

        let mut emitter = ParticlesEmitter {
            name: Some("TestEmitter".to_string()),
            image: Some("star".to_string()),
            image_frames: Some(4),
            particle_flags: 15,
            ..Default::default()
        };

        let tp1 = ParticlesTrackNode {
            time: 0.0,
            low_value: Some(1.0),
            high_value: Some(2.0),
            curve_type: None,
            distribution: None,
        };
        emitter.spawn_rate = Some(vec![tp1]);

        let field = ParticlesField {
            field_type: Some(2),
            x: Some(vec![ParticlesTrackNode {
                time: 0.5,
                low_value: Some(10.0),
                high_value: None,
                curve_type: None,
                distribution: None,
            }]),
            ..Default::default()
        };
        emitter.field = Some(vec![field]);

        particles.emitters.push(emitter);
        particles
    }

    #[test]
    fn test_particles_roundtrip_pc() {
        let original = create_test_particles();
        let encoded = encode(&original, ParticlesVersion::PC).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.emitters.len(), decoded.emitters.len());
        assert_eq!(original.emitters[0].name, decoded.emitters[0].name);
        assert_eq!(
            original.emitters[0].spawn_rate,
            decoded.emitters[0].spawn_rate
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap().len(),
            decoded.emitters[0].field.as_ref().unwrap().len()
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap()[0].x,
            decoded.emitters[0].field.as_ref().unwrap()[0].x
        );
    }

    #[test]
    fn test_particles_roundtrip_phone32() {
        let original = create_test_particles();
        let encoded = encode(&original, ParticlesVersion::Phone32).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.emitters.len(), decoded.emitters.len());
        assert_eq!(original.emitters[0].name, decoded.emitters[0].name);
        assert_eq!(
            original.emitters[0].spawn_rate,
            decoded.emitters[0].spawn_rate
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap().len(),
            decoded.emitters[0].field.as_ref().unwrap().len()
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap()[0].x,
            decoded.emitters[0].field.as_ref().unwrap()[0].x
        );
    }

    #[test]
    fn test_particles_roundtrip_phone64() {
        let original = create_test_particles();
        let encoded = encode(&original, ParticlesVersion::Phone64).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.emitters.len(), decoded.emitters.len());
        assert_eq!(original.emitters[0].name, decoded.emitters[0].name);
        assert_eq!(
            original.emitters[0].spawn_rate,
            decoded.emitters[0].spawn_rate
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap().len(),
            decoded.emitters[0].field.as_ref().unwrap().len()
        );
        assert_eq!(
            original.emitters[0].field.as_ref().unwrap()[0].x,
            decoded.emitters[0].field.as_ref().unwrap()[0].x
        );
    }
}
