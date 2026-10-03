# particle-codec

`particle-codec` is a reusable Rust library for PopCap particle and trail
resources. It is independent from the Toolkit GUI and does not provide a CLI
or aggregate SDK.

Supported data includes:

- PC, 32-bit mobile and 64-bit mobile compiled particle layouts;
- PopCap `0xDEADFED4` zlib containers;
- compiled trail resources;
- particle and trail XML;
- compact track nodes, fields, flags, curves and emitter properties.

```rust
use particle_codec::{ParticlesVersion, decode, encode};

let particles = decode(&compiled_bytes)?;
let rebuilt = encode(&particles, ParticlesVersion::PC)?;

# Ok::<(), particle_codec::ParticlesError>(())
```

The project is licensed under `AGPL-3.0-or-later`.
