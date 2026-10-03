# serde-rton

`serde-rton` is a Serde-based reader and writer for PopCap/PvZ2 RTON files.

The crate focuses on binary RTON. For JSON, use `serde_json` directly with
Serde. There is no separate PvZ2 JSON bridge API in this crate.

## Features

- Deserialize RTON into ordinary Rust structs with `serde::Deserialize`.
- Serialize Rust values back to standard RTON with `serde::Serialize`.
- Use `serde_rton::Value` as a dynamic AST when the schema is unknown.
- Preserve object entry order and duplicate keys when using `Value`.
- Read standard RTON files and PvZ2 compact runtime RTON files.
- Write standard RTON with `to_bytes` / `to_writer`.
- Write compact runtime RTON with `to_compact_bytes` / `to_compact_writer`.
- Support RTID values, BinaryBlob values, string interning, direct strings, and
  explicit VarInt wrappers.
- Provide optional Rijndael-192-CBC helpers for encrypted PvZ2 RTON payloads
  behind the `crypto` feature.

## Installation

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0" # only needed if you want JSON output/input
serde-rton = { git = "https://github.com/LambdaEd1th/serde-rton" }
```

For local development inside this repository:

```toml
serde-rton = { path = "." }
```

Enable encrypted-RTON helpers with:

```toml
serde-rton = { path = ".", default-features = false, features = ["crypto"] }
```

## RTON to JSON with serde_json

Use `serde_rton::Value` if you want a generic RTON tree, then pass it directly
to `serde_json`.

```rust
use serde_rton::{from_bytes, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rton = std::fs::read("config.rton")?;
    let value: Value = from_bytes(&rton)?;

    let json = serde_json::to_string_pretty(&value)?;
    std::fs::write("config.json", json)?;

    Ok(())
}
```

`Value::Object` stores entries as `Vec<(String, Value)>`, so duplicate object
keys and entry order are preserved when serializing through `serde_json`.
Avoid converting through `serde_json::Value` or a `HashMap` if duplicate keys or
ordering matter.

## JSON to RTON with serde_json

Deserialize JSON directly into `serde_rton::Value`, then write RTON.

```rust
use serde_rton::{to_bytes, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read("config.json")?;
    let value: Value = serde_json::from_slice(&json)?;

    let rton = to_bytes(&value)?;
    std::fs::write("config.rton", rton)?;

    Ok(())
}
```

For known schemas, deserialize JSON into your own struct and serialize that
struct to RTON:

```rust
use serde::{Deserialize, Serialize};
use serde_rton::to_bytes;

#[derive(Debug, Deserialize, Serialize)]
struct LevelConfig {
    objclass: String,
    flags: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read("level.json")?;
    let config: LevelConfig = serde_json::from_slice(&json)?;

    let rton = to_bytes(&config)?;
    std::fs::write("level.rton", rton)?;

    Ok(())
}
```

## Important JSON Semantics

JSON is a semantic text format here. It is not an exact RTON tag-preserving
format.

- Integer width is not preserved. For example, `Value::Int32(1)` serializes as
  JSON number `1`; reading that JSON back into `Value` may produce
  `Value::UInt8(1)` or another smallest fitting numeric variant.
- VarInt tags are not preserved. `Value::VarIntI32(VarInt(1))` becomes a JSON
  number and is read back as a normal integer value.
- RTID values serialize as strings such as `"RTID(0)"` or
  `"RTID(name@parent)"`. Valid RTID strings are read back as `Value::Rtid`.
- Binary blobs serialize as strings like `"$BINARY(\"0A0B\", 2)"` in
  human-readable formats. Reading that JSON string back into `Value` produces a
  normal `Value::String`, not `Value::Binary`.
- `Value::Null` serializes as JSON `null`, but JSON `null` is not currently
  accepted by `Value` deserialization. For PvZ2 RTID-null semantics, use the
  string `"RTID(0)"`.
- `serde_json` does not accept `NaN`, `Infinity`, or `-Infinity` as input.
  Serializing non-finite Rust floats with `serde_json` produces JSON `null`,
  which is not a reversible `Value` representation.
- Large `u64` values serialize as JSON numbers. Some JSON consumers, especially
  JavaScript, may lose precision above `2^53 - 1`.

If you need exact binary semantics, keep the data in RTON and use
`serde_rton::Value` plus `to_bytes` or `to_compact_bytes`.

## Dynamic Value Editing

Use `Value` when you need to edit unknown files without defining Rust structs.

```rust
use serde_rton::{from_bytes, to_bytes, Rtid, Value};

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("config.rton")?;
    let mut value: Value = from_bytes(&data)?;

    if let Value::Object(entries) = &mut value {
        entries.push(("enabled".to_string(), Value::Bool(true)));
        entries.push(("score".to_string(), Value::Int16(1024)));
        entries.push((
            "resource".to_string(),
            Value::Rtid(Rtid::Raw {
                name: "Plant".to_string(),
                parent: "Images".to_string(),
            }),
        ));
    }

    std::fs::write("config_edited.rton", to_bytes(&value)?)?;
    Ok(())
}
```

## Strongly Typed RTON

For stable schemas, normal Serde structs are more ergonomic than `Value`.

```rust
use serde::{Deserialize, Serialize};
use serde_rton::{from_bytes, to_bytes};

#[derive(Debug, Deserialize, Serialize)]
struct LevelData {
    version: u32,
    #[serde(rename = "objclass")]
    class_name: String,
    props: LevelProps,
}

#[derive(Debug, Deserialize, Serialize)]
struct LevelProps {
    zombies: Vec<String>,
    waves: i32,
}

fn main() -> serde_rton::Result<()> {
    let data = std::fs::read("level.rton")?;
    let mut level: LevelData = from_bytes(&data)?;

    level.props.waves += 1;

    std::fs::write("level_edited.rton", to_bytes(&level)?)?;
    Ok(())
}
```

## Controlling RTON Encoding

Most users can rely on the default serializer. When you need specific RTON
encoding behavior, use the wrapper types.

```rust
use serde::Serialize;
use serde_rton::{DirectStr, VarInt};

#[derive(Serialize)]
struct EncodedFields {
    // Forces signed varint tag selection instead of the adaptive default.
    compact_i32: VarInt<i32>,

    // Emits a direct string tag (0x81/0x82) instead of an interned string
    // definition/reference pair (0x90-0x93).
    direct_name: DirectStr<String>,
}
```

## Compact Runtime RTON

PvZ2 also has a compact runtime RTON form using tags such as `0xB0`-`0xBC`.
This crate can read compact files and can write semantic `Value` trees to the
compact runtime form.

```rust
use serde_rton::{to_compact_bytes, Value};

fn main() -> serde_rton::Result<()> {
    let value = Value::Object(vec![("flag".to_string(), Value::Bool(true))]);
    let bytes = to_compact_bytes(&value)?;
    std::fs::write("runtime.rton", bytes)?;
    Ok(())
}
```

## Encrypted RTON Payloads

PvZ2 encrypted RTON payloads use a `[0x10, 0x00]` prefix and
Rijndael-192-CBC. The crypto helpers operate on bytes; after decrypting, pass
the plaintext to `from_bytes`.

```rust
use serde_rton::crypto::{decrypt_data, encrypt_data};
use serde_rton::{from_bytes, to_bytes, Value};

fn main() -> serde_rton::Result<()> {
    let encrypted = std::fs::read("encrypted.rton")?;
    let plain = decrypt_data(&encrypted)?;

    let value: Value = from_bytes(&plain)?;
    let edited = to_bytes(&value)?;
    let encrypted_edited = encrypt_data(&edited)?;

    std::fs::write("encrypted_edited.rton", encrypted_edited)?;
    Ok(())
}
```

## RTON Mapping

| RTON concept | Rust representation | Notes |
| --- | --- | --- |
| Booleans | `bool` / `Value::Bool` | Uses dedicated true/false tags |
| Fixed integers | `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64` | Zero tags are used when appropriate |
| VarInts | `VarInt<T>` / `Value::VarInt*` | Exact VarInt tags are not preserved through JSON |
| Floats | `f32`, `f64` / `Value::Float`, `Value::Double` | Non-finite values are not JSON reversible |
| Strings | `String` / `Value::String` | Standard writer interns strings by default |
| Direct strings | `DirectStr<T>` | Forces direct string tags |
| BinaryBlob | `BinaryBlob` / `Value::Binary` | Human-readable JSON emits `$BINARY(...)` text |
| RTID | `Rtid` / `Value::Rtid` | Serialized SexyAppFramework `RtId`; human-readable JSON emits RTID strings |
| Array | `Vec<T>` / `Value::Array` | Standard arrays may end before declared capacity |
| Object | structs/maps / `Value::Object` | `Value` preserves order and duplicate keys |

## Project Layout

- `src/lib.rs`: crate entry point and public re-exports.
- `src/de.rs`: RTON deserializer.
- `src/ser.rs`: standard and compact RTON serializers.
- `src/value.rs`: dynamic `Value` AST and Serde implementation.
- `src/rtid.rs`: SexyAppFramework `RtId` value parsing and serialization.
- `src/tags.rs`: file markers and tag identifiers.
- `src/binary.rs`: `BinaryBlob` support.
- `src/varint.rs`: `VarInt` and `DirectStr` wrappers.
- `src/crypto.rs`: encrypted RTON helpers.
- `samples/rton/`: curated real-world RTON fixtures.

## Testing

```bash
cargo fmt
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The curated fixtures under `samples/rton/` are included in `cargo test` and are
checked with:

- `RTON -> Value`
- `RTON -> Value -> standard RTON -> Value`
- `RTON -> Value -> compact RTON -> Value`
- `RTON -> Value -> serde_json -> Value -> RTON`

The larger local corpus under `sample/`, when present, has also been checked
with the same flow.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
