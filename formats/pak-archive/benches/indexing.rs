use std::hint::black_box;
use std::io::Cursor;
use std::time::{Duration, Instant};

use pak_archive::{
    EncodeOptions, PakArchive, PakCompression, PakEntry, PakFormat, PakReader, to_bytes,
};

const ENTRIES: usize = 10_000;
const ITERATIONS: usize = 20;

fn main() {
    let entries = (0..ENTRIES)
        .map(|index| {
            PakEntry::new(
                format!("resources/{index:05}.bin"),
                (index as u64).to_le_bytes(),
            )
        })
        .collect();
    let archive = PakArchive::new(
        EncodeOptions {
            format: PakFormat::plain(PakCompression::None),
            ..EncodeOptions::default()
        },
        entries,
    )
    .expect("benchmark archive is valid");
    let bytes = to_bytes(&archive).expect("benchmark archive encodes");

    let mut elapsed = Duration::ZERO;
    for _ in 0..ITERATIONS {
        let started = Instant::now();
        let reader = PakReader::new(Cursor::new(black_box(bytes.as_slice())))
            .expect("benchmark archive indexes");
        black_box(reader.find_entry(b"resources/09999.bin"));
        elapsed += started.elapsed();
    }
    let average = elapsed / ITERATIONS as u32;
    println!(
        "indexed {ENTRIES} entries ({} bytes) in {average:?} average over {ITERATIONS} iterations",
        bytes.len()
    );
}
