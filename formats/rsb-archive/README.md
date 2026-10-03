# rsb-archive

`rsb-archive` is a Rust library for reading and writing PopCap/PvZ2 RSB bundles,
packing and unpacking their RSG packets, and decoding or encoding embedded PTX
textures.

It is an independent format crate: applications can depend on it without the
Toolkit UI, a CLI, or an aggregate SDK.

## Features

- Read RSB headers, file lists, packet metadata, composites, auto pools, PTX
  metadata, and resource descriptions.
- Represent the RSB/RSG Magic values as the semantic `rsb1`/`rsgp` FourCCs and
  encode them as little-endian `u32` values (`1bsr`/`pgsr` on disk).
- Write RSB metadata sections and extract embedded RSG packets.
- Rebuild an existing RSB after adding, replacing, deleting, renaming, or
  recompressing files in its RSG packets while preserving unknown metadata and
  rewriting the resource/RSG indexes.
- Add, remove, or replace complete Part-0 RSG packets while rebuilding packet,
  resource-path, group, and auto-pool indices.
- Pack and unpack RSG packet data with PopCap-compatible zlib compression.
- Decode and encode common PTX pixel formats, including ETC1, PVRTC, and ASTC.
- Serialize public metadata types with Serde.
- Use generic `Read + Seek` and `Write + Seek` streams instead of filesystem-only
  APIs.

## Installation

```toml
[dependencies]
rsb-archive = { git = "https://github.com/LambdaEd1th/rsb-archive" }
```

ASTC and PVRTC decoding and encoding are implemented in pure Rust. They do not
require a C/C++ compiler or architecture-specific native libraries, and are
available on WebAssembly.

### Optional WGPU acceleration

UI applications can enable the optional GPU backend without depending on a
separate renderer crate:

```toml
[dependencies]
rsb-archive = {
    git = "https://github.com/LambdaEd1th/rsb-archive",
    features = ["gpu"],
}
```

The `rsb_archive::ptx::gpu` module provides:

- Direct ASTC and ETC1 sampling when the device was created with the matching
  WGPU compressed-texture feature.
- Portable WGSL ETC1 and PVRTC decoding to render-ready RGBA textures,
  including PVRTC+A8, ETC1+A8, ETC1 with compressed alpha, and ETC1 palette
  alpha payloads.
- Fast block-parallel ASTC, ETC1, and PVRTC encoders for interactive previews,
  with GPU-side A8, compressed-alpha, and palette-index generation.
- RGBA readback and an aspect-fit `PtxPreviewRenderer` for surfaces or
  offscreen render targets.
- Automatic CPU fallback on WebGL2 or devices without the required compute or
  compressed-texture capabilities.

Use `supported_optional_features(&adapter)` when requesting the WGPU device.
The compute encoders prioritize interactive latency: ASTC uses a void-extent
color per block and ETC1 uses an average-color individual-mode fit. Continue
to use `PtxEncoder` for final, quality-oriented exports.

CPU and GPU APIs use the same resolved `PtxFormat`. `Etc1A8` represents an
uncompressed A8 plane, while `Etc1CompressedAlpha` represents a second ETC1
grayscale plane. The compatibility fields `alpha_size` and `alpha_format`
correspond to PvZ2 China's `additional_byte_count` and `scale`; they are not
an alpha-codec enum.

All CPU codec boundaries use RGBA8: `Rgba8Surface<'_>` borrows row-strided
input bytes without copying, and decoders return `image::RgbaImage`. `Rgba8`
is the four-byte block-working pixel, while widened `RgbaI32` or floating-point
values are used only for interpolation and encoder search. Codec entry points
accept `Rgba8Surface<'_>` or `image::RgbaImage`; no dynamic-image compatibility
layer is exposed.

## Decode a PTX payload

Resolve ambiguous RSB format codes once, validate and split the payload, then
pass the same descriptor or payload to CPU and GPU consumers:

```rust,no_run
use rsb_archive::{
    ChannelOrder, PtxDecoder, PtxDescriptor, PtxFormatCode, PtxPayload,
    PtxRsbMetadata,
};

fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, rsb_archive::PtxError> {
    let descriptor = PtxDescriptor::from_rsb_payload(
        width,
        height,
        PtxRsbMetadata {
            format_code: PtxFormatCode(147),
            alpha_size: None,
            alpha_format: Some(100),
            row_pitch: None,
            channel_order: ChannelOrder::Rgba,
        },
        bytes,
    )?;
    let payload = PtxPayload::parse(bytes, descriptor)?;
    PtxDecoder::decode_payload(&payload)
}
```

All payload length, block-grid, row-pitch, color-plane and alpha-plane
validation lives in `PtxPayloadLayout`; backends do not independently guess
plane offsets.

## Read an RSB

```rust,no_run
use rsb_archive::Rsb;
use std::fs::File;

fn main() -> rsb_archive::Result<()> {
    let file = File::open("main.rsb")?;
    let mut rsb = Rsb::open(file)?;

    println!("RSB version: {}", rsb.header.version);
    for packet in rsb.read_rsg_info()? {
        println!("{}: {} bytes", packet.name, packet.rsg_length);
    }

    Ok(())
}
```

## Pack and unpack an RSG

```rust,no_run
use rsb_archive::{Part1Extra, UnpackedFile, pack_rsg, unpack_rsg};
use std::io::Cursor;

fn main() -> rsb_archive::Result<()> {
    let files = vec![UnpackedFile {
        path: "IMAGES/EXAMPLE.PTX".into(),
        data: vec![0; 16],
        is_part1: true,
        part1_info: Some(Part1Extra {
            id: 0,
            width: 2,
            height: 2,
        }),
    }];

    let mut packed = Cursor::new(Vec::new());
    pack_rsg(&mut packed, &files, 4, 3)?;

    packed.set_position(0);
    let unpacked = unpack_rsg(&mut packed)?;
    assert_eq!(unpacked[0].path, "IMAGES/EXAMPLE.PTX");

    Ok(())
}
```

The `flags` argument selects which RSG data sections use zlib:

| Flag | Part 0 | Part 1 |
| --- | --- | --- |
| `0` | raw | raw |
| `1` | raw | zlib |
| `2` | zlib | raw |
| `3` | zlib | zlib |

## Edit an existing RSB

`rebuild_rsb` copies unedited packets byte-for-byte and rebuilds only the
packets supplied through `RsbArchiveEdit`. It also writes new resource and RSG
path dictionaries, packet offsets, auto-pool sizes, and optional fixed-index
PTX metadata:

```rust,no_run
use rsb_archive::{
    Rsb, RsbArchiveEdit, RsgHeader, RsgPacketEdit, rebuild_rsb, unpack_rsg,
};
use std::io::Cursor;

fn replace_first_file(source: &[u8]) -> rsb_archive::Result<Vec<u8>> {
    let mut archive = Rsb::open(Cursor::new(source))?;
    let info = archive.read_rsg_info()?.remove(0);
    let raw = archive.extract_packet(&info)?;
    let header = RsgHeader::read_from(&mut Cursor::new(&raw))?;
    let mut files = unpack_rsg(&mut Cursor::new(raw))?;
    let original_paths = files.iter().map(|file| file.path.clone()).collect();
    files[0].data = b"replacement data".to_vec();

    rebuild_rsb(
        source,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index: 0,
                original_paths,
                name: info.name,
                version: header.version,
                compression_flags: header.flags,
                files,
            }],
            ptx_infos: None,
            ..Default::default()
        },
    )
}
```

Part-1 textures can be added or removed. Local IDs in each edited packet must
remain contiguous from zero, and `RsbArchiveEdit::ptx_infos` must contain the
complete resized global metadata table. The rebuild recalculates every packet's
PTX count and global begin index. Dimensions, format, pitch, additional byte
count, and scale are stored in the same table.

Complete RSG additions use `RsgPacketAddition`, optionally with an
`RsgPacketGroup` placement, while `RsbArchiveEdit::removed_packets` contains
original packet indices to delete. New RSG packets may contain Part-1 textures
when their metadata is included at the end of `ptx_infos`. Removing a complete
original RSG also removes its global Part-1 metadata range automatically and
reindexes the remaining packet, texture, resource, autopool, and composite
records. A custom `ptx_infos` table is only required when the same rebuild also
adds textures or otherwise changes retained packet texture counts.

The crate also exposes the same compression independently:

```rust
use rsb_archive::{
    compress_rsg_zlib, compress_zlib_with_level, decompress_zlib_exact,
};

let source = b"PopCap resource data";

// A regular zlib-wrapped DEFLATE stream.
let stream = compress_zlib_with_level(source, 9)?;
assert_eq!(decompress_zlib_exact(&stream, source.len())?, source);

// The RSG representation, zero-padded to a 4096-byte boundary.
let stored_section = compress_rsg_zlib(source)?;
assert_eq!(decompress_zlib_exact(&stored_section, source.len())?, source);

# Ok::<(), rsb_archive::RsbError>(())
```

The real-file integration test accepts any RSB through an environment variable
and automatically skips when no sample is available:

```sh
RSB_ARCHIVE_REAL_SAMPLE=/path/to/main.rsb \
    cargo test -p rsb-archive --all-features --test real_rsb -- --nocapture
```

## Encode an ASTC PTX payload

```rust,no_run
use image::RgbaImage;
use rsb_archive::{AstcQuality, PtxEncoder, Rgba8Surface};

fn encode(image: &RgbaImage) -> rsb_archive::Result<Vec<u8>> {
    PtxEncoder::encode_astc_rgba8(
        Rgba8Surface::from_image(image),
        4,
        4,
        AstcQuality::MEDIUM,
    )
}
```

`AstcQuality` controls the search effort of the pure Rust encoder. It retains
the `0..=100` range used by Twinning, but does not promise byte-identical output
or identical preset behavior to Arm `astcenc`.

## Encode a PVRTC PTX payload

```rust,no_run
use image::RgbaImage;
use rsb_archive::{Rgba8Surface, encode_pvrtc_4bpp_rgba8};

fn encode(image: &RgbaImage) -> rsb_archive::Result<Vec<u8>> {
    encode_pvrtc_4bpp_rgba8(Rgba8Surface::from_image(image), true)
}
```

The second argument controls whether alpha participates in the PVRTC endpoint
and modulation search. `PtxEncoder` disables it automatically for the
PVRTC+A8 format because that format stores an independent uncompressed alpha
plane. Width and height must each be powers of two and at least four pixels.

The PvZ2 China PTX format codes follow Twinning's conversion map:

| PTX code | ASTC footprint |
| --- | --- |
| `160` | `4x4` |
| `161` | `5x5` |
| `162` | `6x6` |
| `163` | `8x8` |

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).
