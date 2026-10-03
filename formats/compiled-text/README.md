# compiled-text

`compiled-text` is a reusable Rust reader and writer for PopCap encrypted
Compiled Text files. It is independent from the Toolkit GUI and does not
provide a CLI or aggregate SDK.

The format pipeline is:

1. an 8-byte or 16-byte SMF/zlib container;
2. Rijndael-192-CBC encryption with a key derived from an application seed;
3. standard Base64 encoding.

The decoder detects the SMF header layout automatically, accepts wrapped
Base64 by default, validates Rijndael block alignment, and enforces a decoded
size limit before inflating untrusted data.

```rust
use compiled_text::{DecodeOptions, EncodeOptions, decode, encode_with_options};

let seed = "application-specific seed";
let encoded = encode_with_options(b"hello", seed, EncodeOptions::compact())?;
let decoded = decode(&encoded, seed)?;
assert_eq!(decoded, b"hello");

# Ok::<(), compiled_text::CompiledTextError>(())
```

The project is licensed under `AGPL-3.0-or-later`.
