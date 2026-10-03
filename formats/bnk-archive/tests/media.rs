use bnk_archive::{
    BankChunk, BankHeader, BankVersion, EmbeddedMediaLocation, OwnedEmbeddedMedia, RawChunk,
    SoundBank, from_bytes, to_bytes,
};

#[test]
fn embedded_media_can_be_created_resolved_and_replaced() {
    let mut bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V140,
            id: 1,
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
                data: vec![4, 5],
            },
        ],
        16,
    )
    .unwrap();

    let bytes = to_bytes(&bank).unwrap();
    let decoded = from_bytes(&bytes).unwrap();
    let media = decoded.embedded_media().unwrap();
    assert_eq!(media[0].id, 10);
    assert_eq!(media[0].data, [1, 2, 3]);
    assert_eq!(media[1].id, 20);
    assert_eq!(media[1].data, [4, 5]);
}

#[test]
fn one_located_media_can_be_replaced_without_touching_other_pairs_or_chunks() {
    let mut first = SoundBank {
        header: BankHeader {
            version: BankVersion::V112,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: Vec::new(),
    };
    first
        .set_embedded_media(
            [
                OwnedEmbeddedMedia {
                    id: 10,
                    data: vec![1, 2, 3],
                },
                OwnedEmbeddedMedia {
                    id: 20,
                    data: vec![4, 5],
                },
            ],
            16,
        )
        .unwrap();

    let first_pair = first.chunks.clone();
    let mut bank = SoundBank {
        header: first.header,
        chunks: vec![
            first_pair[0].clone(),
            first_pair[1].clone(),
            BankChunk::Unknown(RawChunk {
                id: bnk_archive::ChunkId(*b"TEST"),
                data: vec![9, 8, 7],
            }),
            first_pair[0].clone(),
            first_pair[1].clone(),
        ],
    };
    let untouched_pair = bank.chunks[0..2].to_vec();
    let unknown = bank.chunks[2].clone();

    bank.replace_embedded_media(
        EmbeddedMediaLocation {
            index_chunk: 3,
            data_chunk: 4,
            entry_index: 1,
        },
        vec![6, 7, 8, 9],
        16,
    )
    .unwrap();

    assert_eq!(bank.chunks[0..2], untouched_pair);
    assert_eq!(bank.chunks[2], unknown);
    let index = bank.build_index();
    let locations = index.embedded_media_locations(20);
    assert_eq!(locations.len(), 2);
    assert_eq!(
        index.embedded_media(&bank, 20).unwrap().unwrap().data,
        [4, 5]
    );
    let second = locations[1];
    let BankChunk::MediaIndex(entries) = &bank.chunks[second.index_chunk] else {
        panic!("expected DIDX")
    };
    let BankChunk::MediaData(data) = &bank.chunks[second.data_chunk] else {
        panic!("expected DATA")
    };
    let entry = entries[second.entry_index];
    assert_eq!(
        &data[entry.offset as usize..(entry.offset + entry.size) as usize],
        [6, 7, 8, 9]
    );
    assert!(from_bytes(&to_bytes(&bank).unwrap()).is_ok());
}

#[test]
fn reserved_media_entry_cannot_be_replaced_with_payload() {
    let mut bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V112,
            id: 1,
            language: 0,
            header_expand: Vec::new(),
        },
        chunks: Vec::new(),
    };
    bank.set_embedded_media(
        [OwnedEmbeddedMedia {
            id: 0,
            data: Vec::new(),
        }],
        16,
    )
    .unwrap();
    assert!(
        bank.replace_embedded_media(
            EmbeddedMediaLocation {
                index_chunk: 0,
                data_chunk: 1,
                entry_index: 0,
            },
            vec![1],
            16,
        )
        .is_err()
    );
}
