use std::fs;
use std::path::Path;

use pak_archive::{FlatProfile, PakCompression, PakFormat, from_bytes, to_bytes};

#[test]
fn real_pvz2_main_pak_is_byte_exact() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../pvz2-toolkit/test_data/main.pak");
    if !path.exists() {
        eprintln!("skipping real PAK test: {} is unavailable", path.display());
        return;
    }
    let bytes = fs::read(&path).unwrap();
    let archive = from_bytes(&bytes).unwrap_or_else(|error| {
        panic!("failed to decode {}: {error}", path.display());
    });
    assert_eq!(
        archive.options().format,
        PakFormat::Flat {
            profile: FlatProfile::PcXor,
            compression: PakCompression::None,
        }
    );
    assert!(!archive.entries().is_empty());
    assert_eq!(to_bytes(&archive).unwrap(), bytes);
}
