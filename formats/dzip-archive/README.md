# dzip

[![Core CI](https://github.com/LambdaEd1th/dzip-archive/actions/workflows/ci.yml/badge.svg)](https://github.com/LambdaEd1th/dzip-archive/actions/workflows/ci.yml)

A pure-Rust library for reading, extracting, creating, and inspecting Dzip archives. The DZ, Zlib, Bzip, and LZMA engines are integrated into this crate, so consumers only need one dependency.

The command-line and graphical applications live in [dzip-tools](https://github.com/LambdaEd1th/dzip-tools).

## Add the library

The crate is not published on crates.io. Pin a release tag from GitHub:

```toml
[dependencies]
dzip = { git = "https://github.com/LambdaEd1th/dzip-archive.git", tag = "v0.5.1" }
```

## Read an archive

```rust,no_run
use dzip::Archive;

let mut archive = Archive::open_path("game.dz")?;
for entry in archive.entries() {
    println!("{} ({} bytes)", entry.path().display(), entry.decompressed_size());
}

let data = archive.read_entry_by_path("Data/config.bin")?;
# Ok::<(), dzip::DzipError>(())
```

## Build an archive

```rust,no_run
use dzip::{ArchiveBuilder, Compression, EntryOptions};

let mut builder = ArchiveBuilder::new();
builder.add_path(
    "Data/config.bin",
    "input/config.bin",
    EntryOptions::new().compression(Compression::Dz),
)?;
builder.write_to_path("game.dz")?;
# Ok::<(), dzip::DzipError>(())
```

Archives may assign a compression method and volume to each file. The library also supports split volumes, safe path handling, extraction limits, deterministic archive creation, and preservation of existing archive images.

## Features

Default features enable reading, writing, and every integrated codec.

- `encode` — archive creation and compression APIs
- `decode` — archive reading and extraction APIs
- `all-codecs` — enables `bzip`, `dz`, `lzma`, and `zlib`
- `parallel` — Rayon-backed parallel work where supported
- `serde` — serialization support for public option types

The codec engines are private implementation details. Their core algorithms remain allocation-oriented and do not rely on C libraries or FFI, while the public archive API uses the Rust standard library for files, paths, and I/O.

## Development

```sh
cargo fmt --all --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

## License

The project is licensed under AGPL-3.0-or-later. The legacy Bzip randomization table retains its original notice in [`LICENSES/BZIP2.txt`](LICENSES/BZIP2.txt).
