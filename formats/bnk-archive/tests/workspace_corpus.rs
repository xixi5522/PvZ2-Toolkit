use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bnk_archive::{BankChunk, from_bytes, to_bytes};

fn collect_banks(directory: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_banks(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bnk"))
        {
            output.push(path);
        }
    }
}

/// Exhaustive local regression over every reference bank in the PvZ
/// workspace. It is ignored in normal CI because those source assets are not
/// distributed with the crate.
#[test]
#[ignore = "requires the local PvZ/Twinning reference corpus"]
fn every_workspace_bank_is_strict_and_byte_exact() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_root
        .ancestors()
        .nth(4)
        .expect("crate must be inside the PvZ workspace");
    let mut paths = Vec::new();
    collect_banks(workspace_root, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no BNK reference files found");

    let mut versions = BTreeMap::<u32, usize>::new();
    let mut hierarchy_objects = 0_usize;
    for path in &paths {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", path.display());
        });
        let bank = from_bytes(&bytes).unwrap_or_else(|error| {
            panic!("failed to decode {}: {error}", path.display());
        });
        *versions.entry(bank.header.version.number()).or_default() += 1;
        hierarchy_objects += bank
            .chunks
            .iter()
            .filter_map(|chunk| match chunk {
                BankChunk::Hierarchy(objects) => Some(objects.len()),
                _ => None,
            })
            .sum::<usize>();
        let encoded = to_bytes(&bank).unwrap_or_else(|error| {
            panic!("failed to encode {}: {error}", path.display());
        });
        assert_eq!(encoded, bytes, "{} was not byte-exact", path.display());
    }

    eprintln!(
        "validated {} BNKs, {hierarchy_objects} HIRC objects, versions {versions:?}",
        paths.len()
    );
}
