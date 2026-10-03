# rsb-patch

`rsb-patch` is an independent pure Rust reader, writer, encoder, and decoder
for PopCap RSBPatch (`RSBP`) files. The semantic FourCC is `RSBP`
(`0x52534250`, matching Twinning); its little-endian on-disk bytes are `PBSR`.
It follows the package and packet record
layout used by Twinning and does not depend on the Toolkit GUI, a CLI, or an
aggregate SDK.

The bundled VCDIFF implementation supports:

- RFC 3284 sectioned delta windows;
- open-vcdiff's interleaved `S` extension used by PopCap tooling and Twinning;
- source-dictionary and previous-target windows;
- ADD, RUN, and overlapping COPY instructions with the RFC address caches;
- RFC custom code tables and custom NEAR/SAME cache sizes;
- optional Adler32 window verification;
- a configurable decoded-output limit for untrusted files.

New deltas default to the Twinning-compatible interleaved representation. The
encoder mirrors Twinning's bundled open-vcdiff match finder: aligned 16-byte
BlockHash entries, a 32-byte COPY threshold, target matching, address caches,
and compound opcodes.

## Installation

```toml
[dependencies]
rsb-patch = { git = "https://github.com/LambdaEd1th/rsb-patch" }
```

The Cargo package uses a hyphen, while Rust imports it with an underscore:

```rust
use rsb_patch::vcdiff;

let before = b"old resource data";
let after = b"new resource data";
let delta = vcdiff::encode(before, after)?;
assert_eq!(vcdiff::decode(before, &delta)?, after);
# Ok::<(), rsb_patch::PatchError>(())
```

## RSBPatch containers

`RsbPatch` represents the `RSBP` package header, its optional information
section delta, and the ordered packet records. `PacketPatch` provides the
packet name, original MD5, and optional VCDIFF payload. Both types verify the
original data's MD5 before applying a delta.

```rust
use rsb_patch::{RsbPatch, md5_hash};

let before = b"old information";
let patch = RsbPatch {
    all_after_size: 4096,
    before_hash: md5_hash(before),
    information_patch: None,
    packets: Vec::new(),
};

let encoded = patch.to_bytes()?;
let decoded = RsbPatch::read(encoded.as_slice())?;
assert_eq!(decoded.apply_information(before)?, before);
# Ok::<(), rsb_patch::PatchError>(())
```

For untrusted input, `RsbPatch::read_with_options` can bound the packet count,
the size of each VCDIFF payload, and the combined delta allocation using
`ContainerDecodeOptions`.

With the default `rsb` feature, the crate can also create and apply a complete
patch directly from RSB v4 bytes:

```rust
use rsb_patch::{
    ArchiveDecodeOptions, ArchiveEncodeOptions, RsbPatch,
};

let before = std::fs::read("before.rsb")?;
let after = std::fs::read("after.rsb")?;
let patch = RsbPatch::create(
    &before,
    &after,
    ArchiveEncodeOptions::stored(),
)?;
let bytes = patch.to_bytes()?;

let patch = RsbPatch::read(bytes.as_slice())?;
let rebuilt = patch.apply_to_archive(
    &before,
    ArchiveDecodeOptions::stored(),
)?;
assert_eq!(rebuilt, after);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The existing `create_archive_patch` and `apply_archive_patch` free functions
remain available for source compatibility.

`ArchiveEncodeOptions::raw()` and `ArchiveDecodeOptions::raw()` reproduce
Twinning's `use_raw_packet` path: RSG resource sections are decompressed before
differencing, then recompressed with zlib level 9 while subgroup offsets and
stored section sizes are rewritten. An RSBP file does not record this mode, so
the same mode must be selected for both creation and application.

Disable default features if only the standalone RSBP/VCDIFF layer is needed:

```toml
rsb-patch = { git = "https://github.com/LambdaEd1th/rsb-patch", default-features = false }
```

## License

Licensed under the GNU Affero General Public License v3.0 or later
(`AGPL-3.0-or-later`).
