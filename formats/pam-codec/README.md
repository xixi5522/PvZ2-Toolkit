# pam-codec

`pam-codec` is a Rust library for reading and writing PopCap/PvZ2 PAM animation files.

PAM (PopCap Animation) is the binary animation format used in Plants vs. Zombies 2
for character animations, UI effects, and other sprite-based visuals.

## Features

- **Decode** PAM binary files into strongly-typed Rust structs (`decode_pam`)
- **Encode** Rust structs back to PAM binary format (`encode_pam`)
- Supports all PAM versions (v1 through v6)
- Serde `Serialize` / `Deserialize` on all types for JSON round-trip
- Custom error type (`Error`) with `thiserror`, no `anyhow`
- Minimal dependencies: `serde`, `byteorder`, `thiserror`, `bitflags`

## Installation

```toml
[dependencies]
pam-codec = { git = "https://github.com/LambdaEd1th/pam-codec" }
```

## Usage

### Decode

```rust
use std::fs;
use pam_codec::decode_pam;

let mut file = fs::File::open("animation.pam")?;
let pam = decode_pam(&mut file)?;
println!("Version: {}, sprites: {}", pam.version, pam.sprite.len());
```

### Encode

```rust
use std::fs;
use pam_codec::encode_pam;

let mut file = fs::File::create("output.pam")?;
encode_pam(&pam, &mut file)?;
```

### JSON round-trip

All types implement `serde::Serialize` and `serde::Deserialize`:

```rust
let json = serde_json::to_string_pretty(&pam)?;
let pam: PamInfo = serde_json::from_str(&json)?;
```

## Data Model

| Type | Description |
|------|-------------|
| `PamInfo` | Top-level file: version, frame_rate, position, size, images, sprites |
| `ImageInfo` | Referenced image: name, optional size, v1 angle+translation or v2+ affine transform |
| `SpriteInfo` | Animation layer: optional v4+ name/frame_rate/work_area, frames |
| `FrameInfo` | Single frame: label, stop flag, commands, add/remove/move operations |
| `RemovesInfo` | Remove an image/sprite from the display list |
| `AddsInfo` | Add an image/sprite with transform, blending, timing |
| `MovesInfo` | Modify an existing element: matrix/rotate, color, source rect |

## Version History

| Version | Key differences |
|---------|----------------|
| 1 | Image transforms stored as angle + translation (i16 angle / 1000) |
| 2–3 | Image transforms stored as 4×i32 matrix (÷ 1310720) + i16 translation |
| 4 | Per-sprite frame_rate, string names, size fields on images, optional main sprite |
| 5 | Explicit work_area fields |
| 6 | Extended resource index (u16), reserved empty string after sprite name |

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
