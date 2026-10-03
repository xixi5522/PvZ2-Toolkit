//! PopCap REANIM binary and Adobe XFL encoder/decoder.

#![forbid(unsafe_code)]

pub mod error;
pub mod io;
pub mod types;
pub mod xfl;

pub use error::ReanimError;
pub use io::{decode, decode_pc, decode_phone32, decode_phone64, decode_with_version, encode};
pub use types::{Reanim, ReanimTrack, ReanimTransform, ReanimVersion};
pub use xfl::{decode_xfl, encode_xfl};

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_reanim() -> Reanim {
        let mut reanim = Reanim {
            fps: 60.0,
            ..Default::default()
        };

        let mut track = ReanimTrack {
            name: "TestTrack".to_string(),
            transforms: vec![],
        };

        let t1 = ReanimTransform {
            x: Some(10.0),
            y: Some(20.0),
            font: Some("Arial".to_string()),
            text: Some("Hello".to_string()),
            ..Default::default()
        };

        let t2 = ReanimTransform {
            sx: Some(1.5),
            sy: Some(1.5),
            i: Some("123".to_string()),
            ..Default::default()
        };

        track.transforms.push(t1);
        track.transforms.push(t2);

        reanim.tracks.push(track);
        reanim
    }

    #[test]
    fn test_reanim_roundtrip_pc() {
        let original = create_test_reanim();
        let encoded = encode(&original, ReanimVersion::PC).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.fps, decoded.fps);
        assert_eq!(original.tracks.len(), decoded.tracks.len());
        assert_eq!(original.tracks[0].name, decoded.tracks[0].name);
        assert_eq!(
            original.tracks[0].transforms[0].text,
            decoded.tracks[0].transforms[0].text
        );

        // `i` parameter in PC doesn't serialize as integer exclusively,
        // string roundtrips fine.
        assert_eq!(
            original.tracks[0].transforms[1].i,
            decoded.tracks[0].transforms[1].i
        );
    }

    #[test]
    fn test_reanim_roundtrip_phone32() {
        let original = create_test_reanim();
        let encoded = encode(&original, ReanimVersion::Phone32).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.fps, decoded.fps);
        assert_eq!(original.tracks.len(), decoded.tracks.len());
        assert_eq!(original.tracks[0].name, decoded.tracks[0].name);
        assert_eq!(
            original.tracks[0].transforms[0].text,
            decoded.tracks[0].transforms[0].text
        );
        assert_eq!(
            original.tracks[0].transforms[1].i,
            decoded.tracks[0].transforms[1].i
        );
    }

    #[test]
    fn test_reanim_roundtrip_phone64() {
        let original = create_test_reanim();
        let encoded = encode(&original, ReanimVersion::Phone64).unwrap();
        let decoded = decode(&encoded).unwrap();

        assert_eq!(original.fps, decoded.fps);
        assert_eq!(original.tracks.len(), decoded.tracks.len());
        assert_eq!(original.tracks[0].name, decoded.tracks[0].name);
        assert_eq!(
            original.tracks[0].transforms[0].text,
            decoded.tracks[0].transforms[0].text
        );
        assert_eq!(
            original.tracks[0].transforms[1].i,
            decoded.tracks[0].transforms[1].i
        );
    }
}
