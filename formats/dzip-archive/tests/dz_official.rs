use dzip::format::CHUNK_DZ;
use dzip::reader::{DzipReader, correct_chunk_sizes};
use dzip::volume::FileSystemVolumeManager;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

#[test]
fn decodes_native_dzip_original_codec_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data/native");
    let corpus = root.join("corpus");
    let main_path = root.join("codecs.dz");

    let mut metadata_reader = DzipReader::new(File::open(&main_path).unwrap());
    let archive = metadata_reader.read_archive_settings().unwrap();
    let strings = metadata_reader
        .read_strings((archive.num_user_files + archive.num_directories - 1) as usize)
        .unwrap();
    let map = metadata_reader
        .read_file_chunk_map(archive.num_user_files as usize)
        .unwrap();
    let chunk_settings = metadata_reader.read_chunk_settings().unwrap();
    let mut chunks = metadata_reader
        .read_chunks(chunk_settings.num_chunks as usize)
        .unwrap();
    let volume_names = metadata_reader
        .read_file_list(chunk_settings.num_archive_files.saturating_sub(1) as usize)
        .unwrap();
    let settings = metadata_reader.read_global_settings().unwrap();

    let mut volume_paths = vec![main_path];
    volume_paths.extend(volume_names.iter().map(|name| root.join(name)));
    let file_sizes: HashMap<u16, u64> = volume_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (index as u16, std::fs::metadata(path).unwrap().len()))
        .collect();
    correct_chunk_sizes(&mut chunks, &file_sizes);
    let mut volume_manager = FileSystemVolumeManager::new(root.clone(), volume_names);
    let dz_context = metadata_reader
        .load_dz_context(&chunks, settings, &mut volume_manager)
        .unwrap();

    for (file_index, (directory_id, chunk_ids)) in map.iter().enumerate() {
        let mut archive_path = PathBuf::new();
        if *directory_id != 0 {
            let directory_index = archive.num_user_files as usize + usize::from(*directory_id) - 1;
            for component in strings[directory_index].split(['/', '\\']) {
                archive_path.push(component);
            }
        }
        archive_path.push(&strings[file_index]);
        let expected = std::fs::read(corpus.join(&archive_path)).unwrap();

        let mut actual = Vec::new();
        for &chunk_id in chunk_ids {
            let chunk = chunks[usize::from(chunk_id)];
            if chunk.flags & CHUNK_DZ == 0 {
                continue;
            }
            let decoded = metadata_reader
                .read_chunk_data_with_context(&chunk, &mut volume_manager, Some(&dz_context))
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to decode native DZ chunk {} from {}: {}",
                        chunk_id,
                        display_path(&archive_path),
                        error
                    )
                });
            actual.extend_from_slice(&decoded);
        }

        if chunk_ids
            .iter()
            .any(|&id| chunks[usize::from(id)].flags & CHUNK_DZ != 0)
        {
            assert_eq!(actual, expected, "{}", display_path(&archive_path));
        }
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
