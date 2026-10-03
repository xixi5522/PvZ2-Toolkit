# wem-audio

`wem-audio` is the reusable Wwise WEM audio library used by Ed1th's PvZ
Toolkit. It does not depend on the Toolkit UI or a command-line program.

## Installation

```toml
[dependencies]
wem-audio = { git = "https://github.com/LambdaEd1th/wem-audio" }
```

Supported conversion paths:

- Wwise Vorbis WEM to standard Ogg Vorbis
- Ogg Vorbis to WEM
- PCM WAV to WEM and WEM to WAV
- Wwise IMA ADPCM WAV to WEM and WEM to WAV
- AAC/M4A to WEM and WEM AAC extraction/decoding
- Automatic WEM format inspection and WEM to WAV dispatch

```rust,no_run
use std::fs::File;
use wem_audio::{CodebookLibrary, VorbisOptions, VorbisWemDecoder};

let input = File::open("audio.wem")?;
let output = File::create("audio.ogg")?;
let options = VorbisOptions::new()
    .with_codebooks(CodebookLibrary::aotuv_603());
VorbisWemDecoder::with_options(input, options)?.decode_to_ogg(output)?;
# Ok::<(), wem_audio::WemError>(())
```

The public API is organized around `WemReader` plus codec-specific types:

- `VorbisWemDecoder` and `VorbisWemEncoder`
- `PcmWemEncoder` and `AdpcmWemEncoder`
- `AacWemEncoder`, `AacMetadata`, and byte/reader-based AAC probing
- `decode_wem_to_wav` for high-level decoding

Encoders stream their payloads instead of buffering the complete input. On
native targets, high-level Vorbis decoding also uses a temporary Ogg stream to
avoid retaining the complete intermediate file in memory.

Feature selection:

- no features: WEM container inspection
- `wav`: PCM and ADPCM encoding
- `vorbis`: Wwise Vorbis/Ogg conversion
- `aac`: AAC wrapping, extraction, and probing
- `transcode` (default): all codecs plus high-level WEM-to-WAV decoding

Plants vs. Zombies 2 WEM files commonly use the aoTuV 6.03 packed codebooks.
Use `CodebookLibrary::aotuv_603()` for those files. Embedded codebook sets are
borrowed static data, and cloning a library does not duplicate the codebooks.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for ww2ogg and
embedded-codebook attribution.
