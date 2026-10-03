# bnk-archive

`bnk-archive` is a pure Rust, version-aware reader and writer for Audiokinetic
Wwise `.bnk` sound banks.

The crate keeps chunk order and unknown data losslessly, decodes embedded WEM
media, exposes typed game-synchronization and environment settings, and
provides an editable field model for every hierarchy layout described by
Twinning across the supported Wwise generations.

## Installation

```toml
[dependencies]
bnk-archive = { git = "https://github.com/LambdaEd1th/bnk-archive" }
```

## Coverage

- Wwise bank versions 72, 88, 112, 113, 118, 120, 125, 128, 132, 134, 135,
  140, 145, and 150
- BKHD, DIDX/DATA, INIT, STMG, ENVS, HIRC, STID, and PLAT
- Complete Twinning-described HIRC field layouts for Wwise 72, 88, 112, 113,
  118, 120, 125, 128, 132, 134, 135, 140, 145, and 150: state, event/action,
  dialogue, sound, buses, effects, modulators, attenuation, actor/container,
  and music objects
- Typed, path-addressable RTPC, State, property, effect, metadata, positioning,
  auxiliary-send, playback, transition, association-path and music fields
- Exact STMG version boundaries: Wwise 120–124 and 125–139 fixed-zero tails,
  plus Twinning's neutral six-float `u1` records since Wwise 140
- Twinning-named, version-aware common-property and packed-field enumeration
  catalogs for editor UIs
- Byte-exact preservation of chunk order, unknown chunks, unknown hierarchy
  bodies, header expansion bytes, and DATA padding
- Configurable allocation limits and strict or permissive validation; writing
  structured HIRC data revalidates its counts, conditions, fixed constants,
  enumerations, and reserved packed bits
- Separate HIRC field-count and field-path allocation budgets for untrusted
  banks
- Seek-based lazy chunk access and streaming embedded-WEM extraction for large
  DATA chunks
- Streaming `to_writer` implementation that does not construct a second copy
  of the complete bank
- Rebuildable HIRC/WEM/chunk indexes for editor workloads
- Optional Serde support through the default `serde` feature

Plug-in-owned parameter blocks, unknown future hierarchy types, BKHD expansion
bytes, and Twinning's explicitly unknown fields intentionally remain lossless
byte or scalar boundaries: their schema is not selected by the BNK version and
must not be guessed by the archive layer.

## Editing hierarchy fields

```rust
use bnk_archive::{BankChunk, HierarchyBody, HierarchyFieldValue, from_bytes, to_bytes};

let mut bank = from_bytes(&std::fs::read("Music.bnk")?)?;
for object in bank.chunks.iter_mut().filter_map(|chunk| match chunk {
    BankChunk::Hierarchy(objects) => Some(objects),
    _ => None,
}).flatten() {
    if let HierarchyBody::Sound(sound) = &mut object.body {
        if let Some(HierarchyFieldValue::PropertyValue(volume)) =
            sound.settings.get_mut("node.properties.regular[0].value_for_0")
        {
            *volume = 0.0_f32.to_bits();
        }
    }
}
std::fs::write("Music-edited.bnk", to_bytes(&bank)?)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

## Reading and writing

```rust
use bnk_archive::{from_bytes, to_bytes};

let bytes = std::fs::read("Init.bnk")?;
let bank = from_bytes(&bytes)?;
assert_eq!(bank.version().number(), 140);
assert_eq!(to_bytes(&bank)?, bytes);

# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `from_bytes_lossless` for an unknown version or a non-canonical known
chunk. Strict parsing is the default and rejects inconsistent lengths,
out-of-range media entries, invalid constants, and unsupported versions.
`DecodeOptions::twinning_compatible()` additionally enforces Twinning's exact
top-level chunk order. The default remains lossless and accepts unknown chunks.

Disable serialization dependencies when only binary I/O is needed:

```toml
[dependencies]
bnk-archive = {
    git = "https://github.com/LambdaEd1th/bnk-archive",
    default-features = false,
}
```

## Large banks

`SoundBankReader` scans only the BKHD and chunk headers. It can decode selected
chunks or extract one WEM without loading the complete DATA chunk:

```rust
use std::fs::File;
use bnk_archive::SoundBankReader;

let mut bank = SoundBankReader::new(File::open("Music.bnk")?)?;
let mut wem = File::create("123.wem")?;
bank.copy_embedded_media(123, &mut wem)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

## Embedded WEM media

```rust
use bnk_archive::from_bytes;

let bytes = std::fs::read("Music.bnk")?;
let bank = from_bytes(&bytes)?;
for media in bank.embedded_media()? {
    std::fs::write(format!("{}.wem", media.id), media.data)?;
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

Call `SoundBank::build_index()` when an editor repeatedly resolves HIRC or WEM
identifiers. Rebuild the index after mutating the bank.

An editor can replace one exact embedded-media occurrence—even when identifiers
are duplicated or a bank contains multiple DIDX/DATA pairs—without changing
other chunks:

```rust
use bnk_archive::{EmbeddedMediaLocation, from_bytes, to_bytes};

let mut bank = from_bytes(&std::fs::read("Music.bnk")?)?;
bank.replace_embedded_media(
    EmbeddedMediaLocation {
        index_chunk: 0,
        data_chunk: 1,
        entry_index: 3,
    },
    std::fs::read("replacement.wem")?,
    16,
)?;
std::fs::write("Music-edited.bnk", to_bytes(&bank)?)?;

# Ok::<(), Box<dyn std::error::Error>>(())
```

The 16-byte alignment matches Twinning's SoundBank encoder. Replacing media
rebuilds only the selected DIDX/DATA pair; unknown chunks, HIRC objects, and
the remaining media stay in their original order.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
