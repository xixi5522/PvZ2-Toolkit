use newton_manifest::{
    ValidationProfile, encoded_len, from_bytes, raw_from_bytes_borrowed, raw_to_bytes, to_bytes,
};
use std::env;
use std::path::{Path, PathBuf};

const SAMPLE_ENV: &str = "NEWTON_MANIFEST_REAL_SAMPLE";

fn real_sample_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os(SAMPLE_ENV).map(PathBuf::from) {
        return Some(path);
    }

    let workspace_sample = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../../pvz2/test_data/12.7.1_unpacked/__MANIFESTGROUP__/PROPERTIES/RESOURCES.NEWTON",
    );
    workspace_sample.exists().then_some(workspace_sample)
}

#[test]
fn round_trips_a_real_pvz2_resource_manifest_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = real_sample_path() else {
        eprintln!("skipping real NEWTON test; set {SAMPLE_ENV} to RESOURCES.NEWTON");
        return Ok(());
    };

    let source = std::fs::read(&path)?;
    let manifest = from_bytes(&source)?;
    let encoded = to_bytes(&manifest)?;
    let raw = raw_from_bytes_borrowed(&source)?;

    assert_eq!(encoded, source);
    assert_eq!(raw_to_bytes(&raw)?, source);
    assert_eq!(encoded_len(&manifest)?, source.len());
    assert!(manifest.slot_count > 0);
    assert!(!manifest.groups.is_empty());
    let report = manifest.validate(ValidationProfile::Canonical);
    assert!(
        report.is_valid(),
        "official NEWTON should satisfy canonical validation: {:#?}",
        report.issues
    );
    eprintln!(
        "real NEWTON: {}; slots={}; groups={}; bytes={}",
        path.display(),
        manifest.slot_count,
        manifest.groups.len(),
        source.len()
    );
    Ok(())
}
