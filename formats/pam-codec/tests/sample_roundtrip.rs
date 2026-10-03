use pam_codec::{PamInfo, SpriteInfo, decode_pam, encode_pam};
use std::fs::File;
use std::io::Cursor;
use std::path::Path;

#[test]
fn viewer_samples_decode_encode_decode() {
    let samples = [
        "../pam-viewer/sample/aloe/ALOE.PAM",
        "../pam-viewer/sample/sunflower/SUNFLOWER.PAM",
    ];

    for sample in samples {
        let path = Path::new(sample);
        if !path.exists() {
            continue;
        }

        let mut file = File::open(path).expect("sample PAM should open");
        let original = decode_pam(&mut file).expect("sample PAM should decode");

        let mut encoded = Vec::new();
        encode_pam(&original, &mut encoded).expect("sample PAM should encode");

        let mut cursor = Cursor::new(encoded);
        let decoded = decode_pam(&mut cursor).expect("encoded sample PAM should decode");

        assert_eq!(decoded.version, original.version);
        assert_eq!(decoded.frame_rate, original.frame_rate);
        assert_eq!(decoded.image.len(), original.image.len());
        assert_eq!(decoded.sprite.len(), original.sprite.len());
        assert_eq!(
            decoded.main_sprite.is_some(),
            original.main_sprite.is_some()
        );
    }
}

#[test]
fn version3_sprites_do_not_gain_frame_rate() {
    let pam = PamInfo {
        version: 3,
        frame_rate: 24,
        position: [0.0, 0.0],
        size: [1.0, 1.0],
        image: Vec::new(),
        sprite: vec![SpriteInfo::default()],
        main_sprite: Some(SpriteInfo::default()),
    };

    let mut encoded = Vec::new();
    encode_pam(&pam, &mut encoded).expect("version 3 PAM should encode");

    let mut cursor = Cursor::new(encoded);
    let decoded = decode_pam(&mut cursor).expect("version 3 PAM should decode");

    assert_eq!(decoded.version, 3);
    assert_eq!(decoded.sprite[0].frame_rate, None);
    assert_eq!(decoded.main_sprite.unwrap().frame_rate, None);
}
