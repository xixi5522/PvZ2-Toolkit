use std::io::Cursor;

use bnk_archive::{
    BankChunk, BankHeader, BankVersion, ChunkId, DecodeLimits, DecodeOptions, HierarchyBody,
    HierarchyKind, OwnedEmbeddedMedia, RawChunk, SoundBank, SoundBankReader, ValidationMode,
    from_bytes_with_options, to_bytes, to_writer,
};

fn bank_with_media() -> SoundBank {
    let mut bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V140,
            id: 7,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: Vec::new(),
    };
    bank.set_embedded_media(
        [
            OwnedEmbeddedMedia {
                id: 10,
                data: vec![1, 2, 3],
            },
            OwnedEmbeddedMedia {
                id: 20,
                data: vec![4, 5, 6, 7],
            },
        ],
        16,
    )
    .unwrap();
    bank
}

#[test]
fn streaming_writer_and_lazy_reader_preserve_the_owned_model() {
    let bank = bank_with_media();
    let expected = to_bytes(&bank).unwrap();
    let mut streamed = Vec::new();
    to_writer(&bank, &mut streamed).unwrap();
    assert_eq!(streamed, expected);

    let mut reader = SoundBankReader::new(Cursor::new(streamed)).unwrap();
    assert_eq!(reader.header(), &bank.header);
    assert_eq!(reader.chunk_locations().len(), 2);
    assert_eq!(
        reader.read_embedded_media(20).unwrap().unwrap(),
        [4, 5, 6, 7]
    );

    let mut copied = Vec::new();
    assert!(reader.copy_embedded_media(10, &mut copied).unwrap());
    assert_eq!(copied, [1, 2, 3]);
    assert!(!reader.copy_embedded_media(999, Vec::new()).unwrap());
    assert_eq!(reader.read_sound_bank().unwrap(), bank);
}

#[test]
fn rebuildable_indexes_resolve_chunks_hierarchy_and_media() {
    let mut bank = bank_with_media();
    bank.chunks
        .push(BankChunk::Hierarchy(vec![bnk_archive::HierarchyObject {
            kind: HierarchyKind::Unknown,
            type_code: 255,
            id: 42,
            body: HierarchyBody::Raw(vec![9]),
        }]));
    let index = bank.build_index();
    assert_eq!(index.chunk_positions(ChunkId::DIDX), [0]);
    assert_eq!(index.hierarchy_object(&bank, 42).unwrap().id, 42);
    assert_eq!(
        index.embedded_media(&bank, 20).unwrap().unwrap().data,
        [4, 5, 6, 7]
    );
}

#[test]
fn twinning_compatible_mode_is_stricter_than_lossless_strict_mode() {
    let bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V140,
            id: 7,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: vec![BankChunk::Unknown(RawChunk {
            id: ChunkId(*b"ABCD"),
            data: vec![1],
        })],
    };
    let bytes = to_bytes(&bank).unwrap();
    assert!(bnk_archive::from_bytes(&bytes).is_ok());
    assert!(from_bytes_with_options(&bytes, DecodeOptions::twinning_compatible()).is_err());
    assert!(bank.validate().is_ok());
    assert!(bank.validate_twinning_compatibility().is_err());
}

#[test]
fn hirc_field_materialization_obeys_its_own_budget() {
    let mut dialogue = Vec::new();
    dialogue.extend_from_slice(&1_u32.to_le_bytes());
    dialogue.push(100);
    dialogue.extend_from_slice(&0_u32.to_le_bytes());
    dialogue.extend_from_slice(&0_u32.to_le_bytes());
    dialogue.push(0);
    dialogue.extend_from_slice(&0_u16.to_le_bytes());

    let mut hirc = Vec::new();
    hirc.extend_from_slice(&1_u32.to_le_bytes());
    hirc.push(15);
    hirc.extend_from_slice(&(dialogue.len() as u32).to_le_bytes());
    hirc.extend_from_slice(&dialogue);

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BKHD");
    bytes.extend_from_slice(&12_u32.to_le_bytes());
    bytes.extend_from_slice(&140_u32.to_le_bytes());
    bytes.extend_from_slice(&7_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(b"HIRC");
    bytes.extend_from_slice(&(hirc.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&hirc);

    let options = DecodeOptions {
        limits: DecodeLimits {
            max_hierarchy_fields: 2,
            ..DecodeLimits::default()
        },
        validation: ValidationMode::Strict,
    };
    let error = from_bytes_with_options(&bytes, options).unwrap_err();
    assert!(error.to_string().contains("HIRC fields"));
}
