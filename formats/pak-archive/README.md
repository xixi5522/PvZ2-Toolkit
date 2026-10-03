# pak-archive

`pak-archive` is a pure Rust, byte-preserving reader and canonical writer for
PopCap `.pak` resource archives. It is independent from the Toolkit GUI and is
designed for both small conversions and indexed access to large archives.

## Installation

```toml
[dependencies]
pak-archive = { git = "https://github.com/LambdaEd1th/pak-archive" }
```

## Supported containers

- PC flat archives XOR-obfuscated byte-for-byte with `0xF7`;
- canonical unobfuscated PopCap/Twinning flat archives;
- Xbox 360 flat archives with length-prefixed eight-byte payload alignment;
- TV ZIP containers through the default `tv` feature;
- uncompressed and zlib-compressed flat payloads;
- Twinning's canonical `stored_size`, `original_size`, `time` directory order;
- read compatibility with the former Toolkit writer's reversed zlib sizes.

Paths are stored as [`PakPath`](src/path.rs) bytes rather than forced UTF-8, so
unusual archives remain round-trippable. `PakPath::to_safe_relative_path`
performs the validation required before extracting to the host filesystem.

## Owned convenience API

```rust
use pak_archive::{from_bytes, to_bytes};

let source = std::fs::read("main.pak")?;
let mut archive = from_bytes(&source)?;
if let Some(index) = archive.find_entry("properties\\example.rton") {
    archive.replace_entry_data(index, b"replacement".to_vec())?;
}
std::fs::write("main-edited.pak", to_bytes(&archive)?)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

Create a new archive with a format that cannot represent invalid combinations:

```rust
use pak_archive::{
    EncodeOptions, PakArchive, PakCompression, PakEntry, PakFormat,
    PathSeparator, to_bytes,
};

let archive = PakArchive::new(
    EncodeOptions {
        format: PakFormat::pc(PakCompression::Zlib),
        path_separator: PathSeparator::Backslash,
        ..EncodeOptions::default()
    },
    vec![PakEntry::new("properties/example.rton", b"example")],
)?;
let bytes = to_bytes(&archive)?;

# Ok::<(), pak_archive::PakError>(())
```

## Indexed and streaming access

`PakReader` requires `Read + Seek`, parses only the directory, and leaves entry
payloads in the source until requested:

```rust
use std::fs::File;
use std::io::Read;
use pak_archive::PakReader;

let mut pak = PakReader::new(File::open("main.pak")?)?;
let index = pak.find_entry("images\\icon.ptx").expect("entry exists");
let info = pak.entry_info(index).unwrap();
println!("{}: {} bytes", info.path(), info.original_size());

let mut stream = pak.open_entry(index)?;
let mut data = Vec::new();
stream.read_to_end(&mut data)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

For a fully streaming input and output path, build a `PakWriter` from independent
readers. Declared source lengths are checked exactly, and progress observers can
cancel between chunks:

```rust
use std::fs::File;
use pak_archive::{EncodeOptions, PakWriteEntry, PakWriter};

let input = File::open("large.bin")?;
let length = input.metadata()?.len();
let mut pak = PakWriter::new(EncodeOptions::default());
pak.push_entry(PakWriteEntry::new("large.bin", length, input))?;
let mut output = File::create("output.pak")?;
let report = pak.write_seekable(&mut output)?;
println!("wrote {} bytes", report.bytes_written);

# Ok::<(), pak_archive::PakError>(())
```

Owned archives can use `to_seekable_writer`; it avoids a second complete output
copy and returns a write report:

```rust
use std::fs::File;
use pak_archive::{PakArchive, to_seekable_writer};

# fn write(archive: &PakArchive) -> Result<(), pak_archive::PakError> {
let mut output = File::create("output.pak")?;
let report = to_seekable_writer(archive, &mut output)?;
# assert_eq!(report.entries_written, archive.len());
# Ok(())
# }
```

## Detection and compatibility

PAK version 0 does not store a compression flag. Automatic mode validates the
directory, predicted payload boundaries, zlib headers, and platform alignment.
Applications that already know the profile should provide an exact hint:

```rust
use pak_archive::{
    DecodeOptions, PakCompression, PakFormat, PakFormatHint, PakReader,
};

# let file = std::fs::File::open("main.pak")?;
let options = DecodeOptions {
    format_hint: PakFormatHint::Exact(PakFormat::pc(PakCompression::Zlib)),
    ..DecodeOptions::default()
};
let reader = PakReader::with_options(file, options)?;

# Ok::<(), pak_archive::PakError>(())
```

Use `CompatibilityMode::CanonicalOnly` to reject historical reversed-size
archives. Compatible decoding records the observed layout through
`PakArchive::source_info()`; every flat writer still emits canonical Twinning
order and reports that normalization in `WriteReport::warnings`.

## Editing, ranges, and TV metadata

`PakArchive` keeps its encoding invariants behind validated insert, remove,
rename, payload replacement, timestamp, and ZIP metadata methods. Changing
between flat and TV containers requires an explicit `MetadataConversion`
policy, so timestamps or directory entries are never discarded silently.

`PakReader::from_range` reads a PAK embedded inside a larger seekable source.
`PakReader::index_snapshot` produces a detached index that can be attached to
independent handles for parallel extraction without reparsing the directory.
`PakEntryReader::finish` drains and validates a partially consumed payload.

TV ZIP writing preserves raw non-UTF-8 names, explicit directory entries,
archive comments, supported per-entry metadata, and extra fields. ZIP64 records
are emitted automatically when sizes, offsets, or entry counts require them.
`to_path_atomic` writes through a sibling temporary file so failure or
cancellation does not replace the destination.

## Resource limits and features

All decoders enforce archive, directory, aggregate path, entry, and total
decompression limits.
`DecodeLimits::web()` provides a conservative browser profile.

Disable optional Serde and TV/ZIP support when only flat PAK I/O is needed:

```toml
[dependencies]
pak-archive = {
    git = "https://github.com/LambdaEd1th/pak-archive",
    default-features = false,
}
```

The `fuzz/` package contains a `cargo-fuzz` decode/round-trip target. The test
suite also covers fixed canonical bytes, legacy fields, malformed bounds,
non-UTF-8 paths, TV metadata, strict alignment, lazy reads, and a byte-exact
real PvZ `main.pak` when the local corpus is available.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
