#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

use pak_archive::{
    CompatibilityMode, DecodeLimits, DecodeOptions, FlatProfile, PakCompression, PakFormat,
    PakFormatHint, PakReader, ValidationMode, from_bytes_with_options, to_bytes,
};

fuzz_target!(|data: &[u8]| {
    let (selector, payload) = data.split_first().map_or((0, data), |(head, tail)| (*head, tail));
    let format_hint = match selector % 7 {
        0 => PakFormatHint::Auto,
        1 => PakFormatHint::Exact(PakFormat::pc(PakCompression::None)),
        2 => PakFormatHint::Exact(PakFormat::pc(PakCompression::Zlib)),
        3 => PakFormatHint::Exact(PakFormat::plain(PakCompression::None)),
        4 => PakFormatHint::Exact(PakFormat::plain(PakCompression::Zlib)),
        5 => PakFormatHint::Exact(PakFormat::xbox360(PakCompression::None)),
        _ => PakFormatHint::Exact(PakFormat::Flat {
            profile: FlatProfile::Xbox360,
            compression: PakCompression::Zlib,
        }),
    };
    let options = DecodeOptions {
        limits: DecodeLimits {
            max_archive_bytes: 1024 * 1024,
            max_entries: 4096,
            max_directory_bytes: 256 * 1024,
            max_total_path_bytes: 128 * 1024,
            max_entry_stored_bytes: 256 * 1024,
            max_entry_uncompressed_bytes: 512 * 1024,
            max_total_uncompressed_bytes: 1024 * 1024,
        },
        format_hint,
        compatibility: if selector & 0x40 == 0 {
            CompatibilityMode::CanonicalOnly
        } else {
            CompatibilityMode::LegacyToolkit
        },
        validation: if selector & 0x80 == 0 {
            ValidationMode::Strict
        } else {
            ValidationMode::Permissive
        },
    };
    if let Ok(mut reader) = PakReader::with_options(Cursor::new(payload), options) {
        if reader.entries().len() != 0 {
            let _ = reader.read_entry(0);
        }
        let snapshot = reader.index_snapshot();
        let _ = snapshot.attach(Cursor::new(payload));
    }
    if let Ok(archive) = from_bytes_with_options(payload, options) {
        let _ = to_bytes(&archive);
    }
});
