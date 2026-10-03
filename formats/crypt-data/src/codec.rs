use crate::{
    CryptDataError, CryptDataMetadata, DEFAULT_LIMIT, HEADER_SIZE, Result, header::Header,
};

pub fn encrypt(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    encrypt_with_limit(data, key, DEFAULT_LIMIT)
}

pub fn encrypt_with_limit(data: &[u8], key: &[u8], limit: usize) -> Result<Vec<u8>> {
    if data.len() < limit {
        return Ok(data.to_vec());
    }
    validate_key(key, limit)?;

    let encoded_size = HEADER_SIZE
        .checked_add(data.len())
        .ok_or(CryptDataError::EncodedSizeOverflow)?;
    let mut output = Vec::with_capacity(encoded_size);
    Header {
        raw_size: data.len(),
    }
    .write(&mut output);
    xor_prefix(data, key, limit, &mut output);
    output.extend_from_slice(&data[limit..]);
    Ok(output)
}

pub fn decrypt(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    decrypt_with_limit(data, key, DEFAULT_LIMIT)
}

pub fn decrypt_with_limit(data: &[u8], key: &[u8], limit: usize) -> Result<Vec<u8>> {
    let wrapped_threshold = HEADER_SIZE
        .checked_add(limit)
        .ok_or(CryptDataError::EncodedSizeOverflow)?;
    if data.len() < wrapped_threshold {
        return Ok(data.to_vec());
    }
    decrypt_wrapped(data, key, limit)
}

pub fn decrypt_wrapped(data: &[u8], key: &[u8], limit: usize) -> Result<Vec<u8>> {
    let metadata = inspect_wrapped(data, limit)?;
    validate_key(key, limit)?;

    let payload = &data[HEADER_SIZE..];
    let mut output = Vec::with_capacity(metadata.raw_size);
    xor_prefix(payload, key, limit, &mut output);
    output.extend_from_slice(&payload[limit..]);
    Ok(output)
}

pub fn inspect(data: &[u8], limit: usize) -> Result<CryptDataMetadata> {
    let wrapped_threshold = HEADER_SIZE
        .checked_add(limit)
        .ok_or(CryptDataError::EncodedSizeOverflow)?;
    if data.len() < wrapped_threshold {
        return Ok(CryptDataMetadata {
            wrapped: false,
            raw_size: data.len(),
            payload_size: data.len(),
            limit,
        });
    }
    inspect_wrapped(data, limit)
}

fn inspect_wrapped(data: &[u8], limit: usize) -> Result<CryptDataMetadata> {
    let header = Header::parse(data)?;
    if header.raw_size < limit {
        return Err(CryptDataError::RawSizeBelowLimit {
            raw_size: header.raw_size,
            limit,
        });
    }
    let payload_size = data.len() - HEADER_SIZE;
    if payload_size != header.raw_size {
        return Err(CryptDataError::PayloadSizeMismatch {
            expected: header.raw_size,
            actual: payload_size,
        });
    }
    Ok(CryptDataMetadata {
        wrapped: true,
        raw_size: header.raw_size,
        payload_size,
        limit,
    })
}

fn validate_key(key: &[u8], limit: usize) -> Result<()> {
    if limit != 0 && key.is_empty() {
        return Err(CryptDataError::EmptyKey);
    }
    Ok(())
}

fn xor_prefix(data: &[u8], key: &[u8], limit: usize, output: &mut Vec<u8>) {
    if limit == 0 {
        return;
    }
    output.extend(
        data[..limit]
            .iter()
            .zip(key.iter().cycle())
            .map(|(byte, key_byte)| byte ^ key_byte),
    );
}
