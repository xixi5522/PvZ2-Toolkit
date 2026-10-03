# smf-container

`smf-container` is an independent Rust reader and writer for PopCap SMF files.
SMF wraps one payload in zlib and prefixes it with a magic value and the
uncompressed size.

The crate supports both known layouts:

- a compact 8-byte header with 32-bit fields;
- an extended 16-byte header with zero-extended magic and a 64-bit size.

It automatically detects the layout while decoding, validates the declared
payload size, exposes streaming `Read + Seek` / `Write` APIs, and can generate
the uppercase MD5 contents used by `.tag.smf` sidecars. It has no dependency on
the Toolkit GUI, a CLI, or an aggregate SDK.

## Installation

```toml
[dependencies]
smf-container = { git = "https://github.com/LambdaEd1th/smf-container" }
```

The Cargo package uses a hyphen, while Rust imports it with an underscore:

```rust
use std::io::Cursor;

use smf_container::{EncodeOptions, decode, encode};

fn main() -> smf_container::Result<()> {
    let source = b"PopCap payload";
    let encoded = encode(source, EncodeOptions::compact())?;
    let decoded = decode(Cursor::new(encoded))?;
    assert_eq!(decoded, source);
    Ok(())
}
```

## Header selection

Use `EncodeOptions::compact()` for the standard 8-byte header or
`EncodeOptions::extended()` for the 16-byte layout. Custom zlib levels are
available through `EncodeOptions::new`:

```rust
use smf_container::{EncodeOptions, SmfVariant, encode};

let encoded = encode(
    b"payload",
    EncodeOptions::new(SmfVariant::Extended64, 6),
)?;
# Ok::<(), smf_container::SmfError>(())
```

## Streaming APIs

- `inspect` reads metadata without retaining the decoded payload.
- `decode_to` streams the decompressed payload into a writer.
- `encode_to` writes a new SMF container directly into a writer.
- `tag_contents` creates the matching uppercase MD5 sidecar text with CRLF.

## License

Licensed under the GNU Affero General Public License v3.0 or later
(`AGPL-3.0-or-later`).
