use rsb_archive::{RSG_MAGIC, Rsb, RsgInfo, decompress_zlib_exact, unpack_rsg};
use std::env;
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

const SAMPLE_ENV: &str = "RSB_ARCHIVE_REAL_SAMPLE";

fn real_sample_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os(SAMPLE_ENV).map(PathBuf::from) {
        return Some(path);
    }

    let workspace_sample = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../pvz2/com.popcap.ios.PvZ2.app/main.rsb");
    workspace_sample.exists().then_some(workspace_sample)
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
}

fn is_compressed_packet(packet: &[u8]) -> bool {
    let flags = u32_at(packet, 16).unwrap_or(u32::MAX);
    if !(1..=3).contains(&flags) {
        return false;
    }

    let part0_size = u32_at(packet, 32).unwrap_or(0);
    let part1_size = u32_at(packet, 48).unwrap_or(0);
    flags & 0b10 != 0 && part0_size > 0 || flags & 0b01 != 0 && part1_size > 0
}

fn decompress_section(
    packet: &[u8],
    offset: u32,
    stored_size: u32,
    output_size: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let start = usize::try_from(offset)?;
    let stored_size = usize::try_from(stored_size)?;
    let end = start
        .checked_add(stored_size)
        .ok_or("compressed RSG section range overflow")?;
    let section = packet
        .get(start..end)
        .ok_or("compressed RSG section points outside its packet")?;
    Ok(decompress_zlib_exact(
        section,
        usize::try_from(output_size)?,
    )?)
}

fn smallest_compressed_packet(
    rsb: &mut Rsb<File>,
    mut infos: Vec<RsgInfo>,
) -> Result<(RsgInfo, Vec<u8>), Box<dyn std::error::Error>> {
    infos.sort_by_key(|info| info.rsg_length);

    for info in infos {
        if info.rsg_length < 80 {
            continue;
        }
        let packet = rsb.extract_packet(&info)?;
        if packet.get(..4) == Some(RSG_MAGIC.to_le_bytes().as_slice())
            && is_compressed_packet(&packet)
        {
            return Ok((info, packet));
        }
    }

    Err("real RSB contains no compressed RSG packet".into())
}

#[test]
fn decompresses_zlib_sections_from_a_real_rsb() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = real_sample_path() else {
        eprintln!("skipping real RSB test; set {SAMPLE_ENV} to an .rsb file");
        return Ok(());
    };

    let mut rsb = Rsb::open(File::open(&path)?)?;
    let infos = rsb.read_rsg_info()?;
    let packet_count = infos.len();
    let (info, packet) = smallest_compressed_packet(&mut rsb, infos)?;

    let version = u32_at(&packet, 4).ok_or("missing RSG version")?;
    let flags = u32_at(&packet, 16).ok_or("missing RSG compression flags")?;
    let part0_offset = u32_at(&packet, 24).ok_or("missing RSG part 0 offset")?;
    let part0_stored = u32_at(&packet, 28).ok_or("missing RSG part 0 stored size")?;
    let part0_size = u32_at(&packet, 32).ok_or("missing RSG part 0 output size")?;
    let part1_offset = u32_at(&packet, 40).ok_or("missing RSG part 1 offset")?;
    let part1_stored = u32_at(&packet, 44).ok_or("missing RSG part 1 stored size")?;
    let part1_size = u32_at(&packet, 48).ok_or("missing RSG part 1 output size")?;

    let mut decoded_sections = 0;
    if flags & 0b10 != 0 && part0_size > 0 {
        let output = decompress_section(&packet, part0_offset, part0_stored, part0_size)?;
        assert_eq!(output.len(), usize::try_from(part0_size)?);
        decoded_sections += 1;
    }
    if flags & 0b01 != 0 && part1_size > 0 {
        let output = decompress_section(&packet, part1_offset, part1_stored, part1_size)?;
        assert_eq!(output.len(), usize::try_from(part1_size)?);
        decoded_sections += 1;
    }
    assert!(decoded_sections > 0);

    let files = unpack_rsg(&mut Cursor::new(&packet))?;
    assert!(!files.is_empty());

    eprintln!(
        "real RSB: {}; packets={packet_count}; selected={} ({} bytes, v{version}, flags={flags}); \
         part0={part0_stored}->{part0_size}; part1={part1_stored}->{part1_size}; files={}",
        path.display(),
        info.name,
        packet.len(),
        files.len()
    );
    Ok(())
}
