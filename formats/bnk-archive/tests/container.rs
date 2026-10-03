use bnk_archive::{
    BankChunk, BankHeader, BankVersion, ChunkId, DecodeOptions, RawChunk, SoundBank, from_bytes,
    from_bytes_with_options, to_bytes,
};

#[test]
fn unknown_chunks_and_order_are_lossless() {
    let bank = SoundBank {
        header: BankHeader {
            version: BankVersion::V140,
            id: 7,
            language: 0,
            header_expand: vec![1, 2, 3, 4],
        },
        chunks: vec![
            BankChunk::Unknown(RawChunk {
                id: ChunkId(*b"ABCD"),
                data: vec![9, 8, 7],
            }),
            BankChunk::MediaData(vec![1, 2, 3]),
        ],
    };
    let bytes = to_bytes(&bank).unwrap();
    assert_eq!(from_bytes(&bytes).unwrap(), bank);
}

#[test]
fn permissive_mode_retains_an_invalid_known_chunk() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BKHD");
    bytes.extend_from_slice(&12u32.to_le_bytes());
    bytes.extend_from_slice(&140u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"DIDX");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.push(0xff);

    assert!(from_bytes(&bytes).is_err());
    let bank = from_bytes_with_options(&bytes, DecodeOptions::permissive()).unwrap();
    assert!(matches!(
        &bank.chunks[0],
        BankChunk::Unknown(chunk) if chunk.id == ChunkId::DIDX && chunk.data == [0xff]
    ));
    assert_eq!(to_bytes(&bank).unwrap(), bytes);
}

#[test]
fn permissive_mode_round_trips_an_unknown_bank_version() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BKHD");
    bytes.extend_from_slice(&12u32.to_le_bytes());
    bytes.extend_from_slice(&999u32.to_le_bytes());
    bytes.extend_from_slice(&7u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(b"HIRC");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 2, 3]);

    assert!(from_bytes(&bytes).is_err());
    let bank = from_bytes_with_options(&bytes, DecodeOptions::permissive()).unwrap();
    assert_eq!(bank.version().number(), 999);
    assert!(matches!(
        &bank.chunks[0],
        BankChunk::Unknown(chunk) if chunk.id == ChunkId::HIRC && chunk.data == [1, 2, 3]
    ));
    assert_eq!(to_bytes(&bank).unwrap(), bytes);
}
