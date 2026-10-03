# crypt-data

`crypt-data` is a reusable Rust library for PopCap `CRYPT_RES` resources. It is
independent from the Toolkit GUI and does not provide a CLI or aggregate SDK.

The format stores the original byte length in a little-endian `u64` header and
XORs a configurable prefix of the payload with a repeating key. The default
prefix limit is 256 bytes. Inputs smaller than the limit remain unwrapped, as
required by the PopCap format.

```rust
use crypt_data::{decrypt, encrypt};

let raw = vec![0x2a; 512];
let encrypted = encrypt(&raw, b"resource-key")?;
let decoded = decrypt(&encrypted, b"resource-key")?;
assert_eq!(decoded, raw);

# Ok::<(), crypt_data::CryptDataError>(())
```

Use `encrypt_with_limit` and `decrypt_with_limit` when a resource uses a prefix
limit other than 256 bytes. `inspect` validates the container without decrypting
it, and `decrypt_wrapped` can be used when a `CRYPT_RES` header is mandatory.
The conventional resource key is available as `DEFAULT_KEY` but callers can
provide any byte key required by a specific game build.

The project is licensed under `AGPL-3.0-or-later`.
