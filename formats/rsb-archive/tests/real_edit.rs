use rsb_archive::{
    PtxDecoder, Rsb, RsbArchiveEdit, RsgHeader, RsgPacketAddition, RsgPacketEdit, RsgPacketGroup,
    rebuild_rsb, unpack_rsg,
};
use std::env;
use std::io::Cursor;
use std::path::{Path, PathBuf};

const SAMPLE_ENV: &str = "RSB_ARCHIVE_REAL_SAMPLE";

fn real_sample_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os(SAMPLE_ENV).map(PathBuf::from) {
        return Some(path);
    }
    let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../pvz2/com.popcap.ios.PvZ2.app/main.rsb");
    sample.exists().then_some(sample)
}

fn zombie_sample_path() -> Option<PathBuf> {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../pvz2-toolkit/test_data/zombie1.rsb");
    sample.exists().then_some(sample)
}

#[test]
fn edits_and_reopens_a_real_rsb_archive() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = real_sample_path() else {
        eprintln!("skipping real RSB edit test; set {SAMPLE_ENV}");
        return Ok(());
    };
    let source = std::fs::read(&path)?;
    let mut archive = Rsb::open(Cursor::new(&source))?;
    let infos = archive.read_rsg_info()?;
    let (packet_index, info, raw, header, mut files) = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| info.ptx_number == 0 && info.rsg_length <= 0x8000)
        .find_map(|(index, info)| {
            let raw = archive.extract_packet(info).ok()?;
            let header = RsgHeader::read_from(&mut Cursor::new(&raw)).ok()?;
            let files = unpack_rsg(&mut Cursor::new(&raw)).ok()?;
            (!files.is_empty()).then(|| (index, info.clone(), raw, header, files))
        })
        .ok_or("real archive has no small editable Part-0 packet")?;

    let changed_path = files[0].path.clone();
    let changed_data = b"rsb-archive real edit round-trip".to_vec();
    files[0].data.clone_from(&changed_data);
    let original_paths = files.iter().map(|file| file.path.clone()).collect();
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index,
                original_paths,
                name: info.name.clone(),
                version: header.version,
                compression_flags: header.flags,
                files,
            }],
            ptx_infos: None,
            ..Default::default()
        },
    )?;

    let mut reopened = Rsb::open(Cursor::new(&rebuilt))?;
    let rebuilt_infos = reopened.read_rsg_info()?;
    assert_eq!(rebuilt_infos.len(), infos.len());
    let rebuilt_info = &rebuilt_infos[packet_index];
    assert_eq!(rebuilt_info.name, info.name);
    assert_eq!(rebuilt_info.rsg_offset % 0x1000, 0);
    assert_eq!(rebuilt_info.rsg_length % 0x1000, 0);

    let rebuilt_packet = reopened.extract_packet(rebuilt_info)?;
    let rebuilt_files = unpack_rsg(&mut Cursor::new(rebuilt_packet))?;
    assert_eq!(
        rebuilt_files
            .iter()
            .find(|file| file.path == changed_path)
            .ok_or("edited file disappeared")?
            .data,
        changed_data
    );

    let untouched_index = (packet_index + 1) % infos.len();
    let original_untouched = archive.extract_packet(&infos[untouched_index])?;
    let rebuilt_untouched = reopened.extract_packet(&rebuilt_infos[untouched_index])?;
    assert_eq!(original_untouched, rebuilt_untouched);
    assert_ne!(raw, rebuilt_files[0].data);
    eprintln!(
        "real RSB edit: {}; packet={} (#{packet_index}); output={} bytes",
        path.display(),
        info.name,
        rebuilt.len()
    );
    Ok(())
}

#[test]
fn structurally_replaces_a_part0_packet_in_a_real_archive() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(path) = real_sample_path() else {
        eprintln!("skipping real RSB structural edit test; set {SAMPLE_ENV}");
        return Ok(());
    };
    let source = std::fs::read(&path)?;
    let mut archive = Rsb::open(Cursor::new(&source))?;
    let infos = archive.read_rsg_info()?;
    let (removed_index, removed_info, header, files) = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| info.ptx_number == 0 && info.rsg_length <= 0x8000)
        .find_map(|(index, info)| {
            let raw = archive.extract_packet(info).ok()?;
            let header = RsgHeader::read_from(&mut Cursor::new(&raw)).ok()?;
            let files = unpack_rsg(&mut Cursor::new(raw)).ok()?;
            (!files.is_empty()).then(|| (index, info.clone(), header, files))
        })
        .ok_or("real archive has no small Part-0 packet for structural editing")?;
    let added_name = format!("{}_ToolkitRoundTrip", removed_info.name);
    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            added_packets: vec![RsgPacketAddition {
                name: added_name.clone(),
                version: header.version,
                compression_flags: header.flags,
                files: files.clone(),
                group: Some(RsgPacketGroup {
                    name: added_name.clone(),
                    is_composite: false,
                    category: ["0".into(), String::new()],
                }),
            }],
            removed_packets: vec![removed_index],
            ..Default::default()
        },
    )?;

    let mut reopened = Rsb::open(Cursor::new(&rebuilt))?;
    let rebuilt_infos = reopened.read_rsg_info()?;
    assert_eq!(rebuilt_infos.len(), infos.len());
    assert!(
        !rebuilt_infos
            .iter()
            .any(|info| info.name == removed_info.name)
    );
    let added_info = rebuilt_infos
        .iter()
        .find(|info| info.name == added_name)
        .ok_or("added packet is missing after reopening")?;
    let added_files = unpack_rsg(&mut Cursor::new(reopened.extract_packet(added_info)?))?;
    assert_eq!(
        added_files
            .iter()
            .map(|file| (&file.path, &file.data))
            .collect::<Vec<_>>(),
        files
            .iter()
            .map(|file| (&file.path, &file.data))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "real RSB structural edit: {}; removed={} (#{removed_index}); added={added_name}",
        path.display(),
        removed_info.name,
    );
    Ok(())
}

#[test]
fn removes_a_texture_packet_and_reopens_the_real_zombie_archive()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = zombie_sample_path() else {
        eprintln!("skipping real texture RSG removal test; zombie1.rsb is unavailable");
        return Ok(());
    };
    let source = std::fs::read(&path)?;
    let mut archive = Rsb::open(Cursor::new(&source))?;
    let infos = archive.read_rsg_info()?;
    let original_ptx_infos = archive.read_ptx_info()?;
    let original_file_list = archive.read_file_list()?;
    let (removed_index, removed_info) = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| info.ptx_number > 0)
        .min_by_key(|(_, info)| info.rsg_length)
        .map(|(index, info)| (index, info.clone()))
        .ok_or("zombie1.rsb contains no texture packet")?;

    let texture_start = usize::try_from(removed_info.ptx_before_number)?;
    let texture_end = texture_start
        .checked_add(usize::try_from(removed_info.ptx_number)?)
        .ok_or("removed texture range overflow")?;
    let mut expected_ptx_infos = original_ptx_infos.clone();
    expected_ptx_infos.drain(texture_start..texture_end);
    for (index, info) in expected_ptx_infos.iter_mut().enumerate() {
        info.ptx_index = i32::try_from(index)?;
    }
    let removed_paths = original_file_list
        .iter()
        .filter(|entry| entry.pool_index == removed_info.pool_index)
        .map(|entry| entry.name_path.clone())
        .collect::<std::collections::HashSet<_>>();

    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            removed_packets: vec![removed_index],
            ..Default::default()
        },
    )?;

    let mut reopened = Rsb::open(Cursor::new(&rebuilt))?;
    let rebuilt_infos = reopened.read_rsg_info()?;
    assert_eq!(rebuilt_infos.len(), infos.len() - 1);
    assert!(
        !rebuilt_infos
            .iter()
            .any(|info| info.name == removed_info.name)
    );
    assert_eq!(
        reopened.header.ptx_number,
        archive.header.ptx_number - removed_info.ptx_number
    );
    assert_eq!(reopened.read_ptx_info()?, expected_ptx_infos);

    let mut texture_begin = 0_u32;
    for (packet_index, info) in rebuilt_infos.iter().enumerate() {
        assert_eq!(info.pool_index, i32::try_from(packet_index)?);
        assert_eq!(info.ptx_before_number, texture_begin);
        texture_begin = texture_begin
            .checked_add(info.ptx_number)
            .ok_or("rebuilt global PTX count overflow")?;
    }
    assert_eq!(texture_begin, reopened.header.ptx_number);

    let rebuilt_file_list = reopened.read_file_list()?;
    assert!(
        rebuilt_file_list
            .iter()
            .all(|entry| !removed_paths.contains(&entry.name_path))
    );
    assert!(rebuilt_file_list.iter().all(|entry| {
        usize::try_from(entry.pool_index)
            .ok()
            .is_some_and(|index| index < rebuilt_infos.len())
    }));
    assert!(
        reopened
            .read_composite_info()?
            .iter()
            .flat_map(|group| group.packet_info.iter())
            .all(|packet| usize::try_from(packet.packet_index)
                .ok()
                .is_some_and(|index| index < rebuilt_infos.len()))
    );

    let retained_old_index = if removed_index != 0 { 0 } else { 1 };
    let retained_new_index = if retained_old_index < removed_index {
        retained_old_index
    } else {
        retained_old_index - 1
    };
    assert_eq!(
        archive.extract_packet(&infos[retained_old_index])?,
        reopened.extract_packet(&rebuilt_infos[retained_new_index])?
    );

    eprintln!(
        "real texture RSG removal: {}; removed={} (#{removed_index}); textures={}; output={} bytes",
        path.display(),
        removed_info.name,
        removed_info.ptx_number,
        rebuilt.len()
    );
    Ok(())
}

#[test]
fn adds_a_texture_and_reopens_the_real_zombie_archive() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = zombie_sample_path() else {
        eprintln!("skipping real texture edit test; zombie1.rsb is unavailable");
        return Ok(());
    };
    let source = std::fs::read(&path)?;
    let mut archive = Rsb::open(Cursor::new(&source))?;
    let infos = archive.read_rsg_info()?;
    let mut ptx_infos = archive.read_ptx_info()?;
    let (packet_index, packet_info, packet_header, mut files, template_index) = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| info.ptx_number > 0)
        .filter_map(|(index, info)| {
            let raw = archive.extract_packet(info).ok()?;
            let header = RsgHeader::read_from(&mut Cursor::new(&raw)).ok()?;
            let files = unpack_rsg(&mut Cursor::new(raw)).ok()?;
            let template_index = files.iter().position(|file| file.is_part1)?;
            Some((index, info.clone(), header, files, template_index))
        })
        .min_by_key(|(_, info, _, _, _)| info.rsg_length)
        .ok_or("zombie1.rsb contains no editable texture packet")?;

    let mut added = files[template_index].clone();
    let template = added
        .part1_info
        .as_ref()
        .ok_or("template texture has no Part 1 metadata")?;
    let template_global_index = usize::try_from(packet_info.ptx_before_number)?
        .checked_add(usize::try_from(template.id)?)
        .ok_or("template global PTX index overflow")?;
    let insertion_index = usize::try_from(packet_info.ptx_before_number)?
        .checked_add(usize::try_from(packet_info.ptx_number)?)
        .ok_or("new global PTX index overflow")?;
    let mut new_metadata = ptx_infos
        .get(template_global_index)
        .cloned()
        .ok_or("template global PTX metadata is missing")?;
    let original_path = added.path.clone();
    added.path = format!("{original_path}.TOOLKIT_ADDED.PTX");
    added.part1_info.as_mut().unwrap().id = packet_info.ptx_number;
    let added_path = added.path.clone();
    let added_data = added.data.clone();
    files.push(added);
    new_metadata.ptx_index = i32::try_from(insertion_index)?;
    ptx_infos.insert(insertion_index, new_metadata);
    for (index, info) in ptx_infos.iter_mut().enumerate() {
        info.ptx_index = i32::try_from(index)?;
    }

    let rebuilt = rebuild_rsb(
        &source,
        &RsbArchiveEdit {
            packets: vec![RsgPacketEdit {
                packet_index,
                original_paths: files
                    .iter()
                    .filter(|file| file.path != added_path)
                    .map(|file| file.path.clone())
                    .collect(),
                name: packet_info.name.clone(),
                version: packet_header.version,
                compression_flags: packet_header.flags,
                files,
            }],
            ptx_infos: Some(ptx_infos),
            ..Default::default()
        },
    )?;

    let mut reopened = Rsb::open(Cursor::new(&rebuilt))?;
    assert_eq!(reopened.header.ptx_number, archive.header.ptx_number + 1);
    let rebuilt_infos = reopened.read_rsg_info()?;
    assert_eq!(
        rebuilt_infos[packet_index].ptx_number,
        packet_info.ptx_number + 1
    );
    let mut expected_begin = 0_u32;
    for info in &rebuilt_infos {
        assert_eq!(info.ptx_before_number, expected_begin);
        expected_begin = expected_begin
            .checked_add(info.ptx_number)
            .ok_or("rebuilt global PTX count overflow")?;
    }
    assert_eq!(expected_begin, reopened.header.ptx_number);

    let rebuilt_packet = reopened.extract_packet(&rebuilt_infos[packet_index])?;
    let rebuilt_files = unpack_rsg(&mut Cursor::new(rebuilt_packet))?;
    let added = rebuilt_files
        .iter()
        .find(|file| file.path == added_path)
        .ok_or("added texture disappeared after reopening")?;
    assert_eq!(added.data, added_data);
    let added_part1 = added
        .part1_info
        .as_ref()
        .ok_or("added texture lost its Part 1 metadata")?;
    let rebuilt_ptx_infos = reopened.read_ptx_info()?;
    let info = &rebuilt_ptx_infos[insertion_index];
    PtxDecoder::decode_rgba8(
        &added.data,
        added_part1.width,
        added_part1.height,
        info.format,
        info.alpha_size,
        info.alpha_format,
        Some(info.pitch),
        false,
    )?;

    eprintln!(
        "real texture edit: {}; packet={} (#{packet_index}); added={added_path}; output={} bytes",
        path.display(),
        packet_info.name,
        rebuilt.len()
    );
    Ok(())
}
