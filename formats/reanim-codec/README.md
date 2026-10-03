# reanim-codec

`reanim-codec` is a reusable Rust library for PopCap REANIM resources. It is
independent from the Toolkit GUI and does not provide a CLI or aggregate SDK.

Supported data includes:

- PC, 32-bit mobile and 64-bit mobile compiled REANIM layouts;
- PopCap zlib-wrapped and uncompressed payloads;
- automatic compiled-layout detection;
- Adobe XFL directory import and export;
- tracks, transforms, images, text and font properties.

```rust
use reanim_codec::{ReanimVersion, decode, encode};

let reanim = decode(&compiled_bytes)?;
let rebuilt = encode(&reanim, ReanimVersion::PC)?;

# Ok::<(), reanim_codec::ReanimError>(())
```

The project is licensed under `AGPL-3.0-or-later`.
