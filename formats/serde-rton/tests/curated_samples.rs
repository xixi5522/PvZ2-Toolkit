use serde_rton::{Value, from_bytes, to_bytes, to_compact_bytes};
use std::path::{Path, PathBuf};

fn collect_rton_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read sample directory") {
        let entry = entry.expect("read sample directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rton_files(&path, files);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rton"))
        {
            files.push(path);
        }
    }
}

#[test]
fn curated_samples_decode_and_round_trip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/rton");
    let mut files = Vec::new();
    collect_rton_files(&root, &mut files);
    files.sort();

    assert_eq!(files.len(), 10, "unexpected curated sample count");

    let mut failures = Vec::new();
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                failures.push(format!("{name}: read failed: {err}"));
                continue;
            }
        };

        let value: Value = match from_bytes(&bytes) {
            Ok(value) => value,
            Err(err) => {
                failures.push(format!("{name}: RTON -> Value failed: {err}"));
                continue;
            }
        };

        match to_bytes(&value).and_then(|bytes| from_bytes::<Value>(&bytes)) {
            Ok(decoded) if decoded == value => {}
            Ok(_) => failures.push(format!("{name}: standard RTON semantic mismatch")),
            Err(err) => failures.push(format!("{name}: standard RTON round-trip failed: {err}")),
        }

        match to_compact_bytes(&value).and_then(|bytes| from_bytes::<Value>(&bytes)) {
            Ok(decoded) if decoded == value => {}
            Ok(_) => failures.push(format!("{name}: compact RTON semantic mismatch")),
            Err(err) => failures.push(format!("{name}: compact RTON round-trip failed: {err}")),
        }

        match serde_json::to_string(&value)
            .map_err(|err| err.to_string())
            .and_then(|json| serde_json::from_str::<Value>(&json).map_err(|err| err.to_string()))
            .and_then(|json_value| {
                to_bytes(&json_value)
                    .map_err(|err| err.to_string())
                    .and_then(|bytes| from_bytes::<Value>(&bytes).map_err(|err| err.to_string()))
                    .map(|decoded| (json_value, decoded))
            }) {
            Ok((json_value, decoded)) if decoded == json_value => {}
            Ok(_) => failures.push(format!("{name}: JSON bridge semantic mismatch")),
            Err(err) => failures.push(format!("{name}: JSON bridge round-trip failed: {err}")),
        }
    }

    assert!(
        failures.is_empty(),
        "curated RTON sample failures:\n{}",
        failures.join("\n")
    );
}
