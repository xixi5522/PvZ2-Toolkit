# newton-manifest

`newton-manifest` reads and writes the binary `RESOURCES.NEWTON` manifests used
by Plants vs. Zombies 2.

It is an independent format crate. Applications can use it without the Toolkit
UI, a CLI, or an aggregate SDK.

## Features

- Decode and encode complete composite and simple resource groups.
- Preserve resource geometry, atlas, parent, path, and resolution metadata.
- Represent group and resource kinds with Rust enums instead of unchecked
  strings.
- Reject invalid booleans, negative unsigned fields, invalid UTF-8, malformed
  group layouts, trailing bytes, and unreasonable allocation requests.
- Preserve non-canonical flags, signed values, and arbitrary string bytes
  through a separate wire-faithful raw model.
- Decode raw strings without copying when the input is already a byte slice.
- Validate slot tables, IDs, group links, atlas links, and image geometry
  against PvZ2 runtime or official canonical profiles.
- Recognize resolution-specific children of one composite group as mutually
  exclusive, allowing the official manifests to reuse resource IDs and slots
  between variants such as `1536` and `768`.
- Pre-compute encoded lengths and validate complete documents before writing.
- Serialize the public data model with Serde for JSON, YAML, or other
  application-defined representations.
- Work with generic `Read` and `Write` streams and on WebAssembly.

NEWTON has no magic or version field. ID and path presence bits are validated
while decoding but are not exposed as application-level flags: the public model
requires those strings, and the encoder always writes their presence bits.

## Installation

```toml
[dependencies]
newton-manifest = { git = "https://github.com/LambdaEd1th/newton-manifest" }
```

## Decode and encode

```rust,no_run
use newton_manifest::{from_bytes, to_bytes};

fn main() -> newton_manifest::Result<()> {
    let source = std::fs::read("RESOURCES.NEWTON")?;
    let mut manifest = from_bytes(&source)?;

    println!("slots: {}", manifest.slot_count);
    println!("groups: {}", manifest.groups.len());

    manifest.slot_count += 1;
    std::fs::write("RESOURCES.MODIFIED.NEWTON", to_bytes(&manifest)?)?;
    Ok(())
}
```

`decode_newton`/`encode_newton` are also provided for callers migrating from
the previous `pvz2-toolkit` module.

## Raw inspection

Use the raw layer when inspecting or repairing malformed files:

```rust,no_run
use newton_manifest::{raw_from_bytes_borrowed, raw_to_bytes};

# fn main() -> newton_manifest::Result<()> {
let source = std::fs::read("RESOURCES.NEWTON")?;
let raw = raw_from_bytes_borrowed(&source)?;

// String fields borrow directly from `source`.
let encoded = raw_to_bytes(&raw)?;
assert_eq!(encoded, source);
# Ok(())
# }
```

`RawResourceManifest` preserves raw type and flag bytes, absent ID/path fields,
non-UTF-8 strings, signed values, and mixed group contents. Convert it with
`ResourceManifest::try_from(raw)` when strict semantic data is required.

## Validation

```rust,no_run
use newton_manifest::{ValidationProfile, from_bytes};

# fn main() -> newton_manifest::Result<()> {
# let source = std::fs::read("RESOURCES.NEWTON")?;
let manifest = from_bytes(&source)?;
let report = manifest.validate(ValidationProfile::Canonical);
for issue in &report.issues {
    eprintln!("{}: {}", issue.path, issue.message);
}
# Ok(())
# }
```

`Pvz2Runtime` checks known runtime hazards such as duplicate slots, broken
references, and 16-bit atlas-geometry truncation. `Canonical` additionally
checks the normalized invariants found in official generated manifests.

## Decode limits

Untrusted input is decoded with per-record, cumulative string, entry, and
estimated-allocation limits. Vector reservations are fallible, and decode
errors include the byte offset, record index, and field name.

Use `from_reader_with_limits`, `from_bytes_with_limits`, or their `raw_`
counterparts with `DecodeLimits` when a valid manifest needs different limits.

The `fuzz/` directory contains a `cargo-fuzz` target for both semantic and raw
decoders.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
